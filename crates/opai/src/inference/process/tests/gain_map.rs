//! An Ultra HDR photograph's gain map, end to end: loaded, cropped, enhanced, served from the run cache and exported as
//! an Ultra HDR JPEG whose gain map is in register with the written image.

use std::sync::Arc;

use ::image::DynamicImage;
use rust_sak::image::{ExifIfd, ExifValue, decode_bytes, exif_field, metadata_bytes};

use super::*;
use crate::export::{WriteOptions, write};
use crate::image::test_support::{written_turned_ultra_hdr_of, written_ultra_hdr_of};
use crate::image::{GainMap, ImageFormat};

/// The first channel of `image` at `(x, y)`.
fn first(image: &DynamicImage, x: u32, y: u32) -> i32 {
    i32::from(image.to_rgb8().get_pixel(x, y).0[0])
}

/// The MP Entry of the gain map in the JPEG `file`: its size and its offset in the file.
fn gain_map_entry(file: &[u8]) -> (usize, usize) {
    let at = file.windows(4).position(|w| w == b"MPF\0").expect("an MPF index") + 4;
    let u32_at = |offset: usize| u32::from_be_bytes(file[offset..offset + 4].try_into().expect("four")) as usize;
    (u32_at(at + 50 + 20), at + u32_at(at + 50 + 24))
}

#[tokio::test]
async fn a_cropped_and_upscaled_ultra_hdr_photograph_keeps_its_gain_map_in_register_fresh_or_cached() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("hawk.jpg");
    // A half-resolution gain map, so the crop has to be re-expressed at the gain map's own scale.
    std::fs::write(&path, written_ultra_hdr_of((40, 32), (20, 16), 3)).expect("writable");
    let loaded = crate::image::load_blocking(&path).expect("decodable");
    let source_map = loaded.gain_map().expect("carried").clone();
    assert_eq!(source_map.scale(), (0.5, 0.5));

    // A crop as the GUI makes one: the same rectangle cut from the pixels, and from the gain map at its scale.
    let cropped = loaded
        .derive(loaded.pixels().crop_imm(8, 4, 24, 16), format!("{}-cropped", loaded.identity()))
        .with_gain_map(Some(source_map.reframed(|map, scale| {
            let at = |value: u32| (f64::from(value) * scale).round() as u32;
            map.crop_imm(at(8), at(4), at(24), at(16))
        })));
    let cropped_map = cropped.shared_gain_map().expect("reframed");
    assert_eq!((cropped_map.pixels().width(), cropped_map.pixels().height()), (12, 8));

    // A 2x upscale on the fake backend, then the same chain again, served from the cache.
    let backend = Fake::new();
    let cache = store();
    let fresh = process(&backend, Some(&cache), &cropped, &[kyoto(2.0)], None).await.expect("ran").picture;
    let ran = backend.log().ran.len();
    let cached = process(&backend, Some(&cache), &cropped, &[kyoto(2.0)], None).await.expect("ran").picture;
    assert_eq!(backend.log().ran.len(), ran, "the second run was not served from the cache");
    assert_eq!(fresh.dimensions(), (48, 32));

    let mut maps = Vec::new();
    for (name, picture) in [("fresh.jpg", &fresh), ("cached.jpg", &cached)] {
        assert!(
            Arc::ptr_eq(&picture.shared_gain_map().expect("kept"), &cropped_map),
            "{name}: not the cropped one"
        );

        let written = write(picture, &dir.path().join(name), ImageFormat::Jpeg, WriteOptions::default())
            .await
            .expect("written");
        assert_eq!(written.losses, [], "{name}");
        let file = std::fs::read(&written.path).expect("readable");
        let base = decode_bytes(&file).expect("decodable");
        let gain_map = metadata_bytes(&file).gain_map.expect("written");
        let (size, offset) = gain_map_entry(&file);
        assert_eq!((size, &file[offset..]), (gain_map.len(), &gain_map[..]), "{name}: the index locates it");
        let map = decode_bytes(&gain_map).expect("decodable");
        assert_eq!((base.width(), base.height()), (48, 32), "{name}");
        assert_eq!((map.width(), map.height()), (12, 8), "{name}: kept at the cropped source's resolution");

        // In register: the base and the gain map carry the same ramp across the scene, so each gain-map column
        // matches the base four columns to its right of it (the base was cropped then doubled; the map only cropped).
        for x in [0, 5, 11] {
            let (in_base, in_map) = (first(&base, x * 4 + 2, 16), first(&map, x, 4));
            assert!((in_base - in_map).abs() <= 12, "{name}: column {x}: base {in_base}, gain map {in_map}");
        }
        maps.push(gain_map);
    }
    assert_eq!(maps[0], maps[1], "a cached result writes the same gain map as a fresh one");
}

#[tokio::test]
async fn a_turned_ultra_hdr_photograph_is_exported_upright_with_its_gain_map_turned_in_register() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("turned.jpg");
    // Stored 40x32 with a half-resolution gain map, and Orientation 6: a portrait photograph held on its side.
    std::fs::write(&path, written_turned_ultra_hdr_of((40, 32), (20, 16))).expect("writable");
    let loaded = crate::image::load_blocking(&path).expect("decodable");
    assert_eq!(loaded.dimensions(), (32, 40));

    let backend = Fake::new();
    let enhanced = process(&backend, None, &loaded, &[kyoto(2.0)], None).await.expect("ran").picture;
    let written = write(&enhanced, &dir.path().join("out.jpg"), ImageFormat::Jpeg, WriteOptions::default())
        .await
        .expect("written");
    assert_eq!(written.losses, []);

    let file = std::fs::read(&written.path).expect("readable");
    let read = metadata_bytes(&file);
    // Written upright and saying so: a viewer turns neither the base nor the gain map again.
    let exif = read.exif.as_deref().expect("EXIF");
    assert_eq!(exif_field(exif, ExifIfd::Primary, 0x0112), Some(ExifValue::Short(1)));
    let base = decode_bytes(&file).expect("decodable");
    let map = decode_bytes(&read.gain_map.expect("written")).expect("decodable");
    assert_eq!((base.width(), base.height()), (64, 80));
    assert_eq!((map.width(), map.height()), (16, 20), "the gain map was not turned with the base");

    // In register: each gain-map pixel matches the base four pixels to its one (the map is half the source's size,
    // the base twice it), down the ramp the turn made of the stored frame's left-to-right one.
    for y in [0, 9, 19] {
        let (in_base, in_map) = (first(&base, 34, y * 4 + 2), first(&map, 8, y));
        assert!((in_base - in_map).abs() <= 12, "row {y}: base {in_base}, gain map {in_map}");
    }
}

/// A step that hands its input back as it is and rewrites the gain map — inverting it, or failing to — and counts how
/// many times it was asked to.
struct Rewriting {
    fails: bool,
    asked: Arc<std::sync::atomic::AtomicUsize>,
}

impl crate::pipeline::Model<Fake> for Rewriting {
    fn required(&self) -> &[ArtifactId] {
        &[]
    }

    fn sessions(&self) -> Vec<(&ArtifactId, &EpProfile)> {
        Vec::new()
    }
}

impl crate::pipeline::ImagePipeline<Fake> for Rewriting {
    fn stages(&self) -> usize {
        0
    }

    fn run(
        &self,
        input: &DynamicImage,
        _sessions: &[SessionHandle<FakeSession>],
        _depth: ChannelDepth,
        _progress: Option<&dyn Fn(f64)>,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<DynamicImage, InferenceError> {
        Ok(input.clone())
    }

    fn rewrites_gain_map(&self, _gain_map: &GainMap) -> bool {
        true
    }

    fn run_with_gain_map(
        &self,
        input: &DynamicImage,
        gain_map: &GainMap,
        _sessions: &[SessionHandle<FakeSession>],
        _depth: ChannelDepth,
        _progress: Option<&dyn Fn(f64)>,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<(DynamicImage, Option<Result<GainMap, InferenceError>>), InferenceError> {
        self.asked.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if self.fails {
            let reason = "it would not".to_string();
            return Ok((
                input.clone(),
                Some(Err(InferenceError::GainMap { operation: "Rewriting".to_string(), reason })),
            ));
        }
        let mut inverted = gain_map.pixels().clone();
        inverted.invert();
        Ok((input.clone(), Some(Ok(gain_map.rewritten(inverted)))))
    }
}

/// A gain map of the ramp `gain_map_jpeg` writes, beside a base of 48 x 32.
fn ramp_gain_map() -> Arc<GainMap> {
    let bytes = crate::image::test_support::gain_map_jpeg((24, 16), 3);
    let map = GainMap::read(&bytes, (48, 32), ::image::metadata::Orientation::NoTransforms).expect("readable");
    Arc::new(map)
}

/// The chain `Rewriting` then a 2x Kyoto upscale, as `run` is handed it.
fn rewriting_then_kyoto(fails: bool) -> (Vec<Planned<Fake>>, Arc<std::sync::atomic::AtomicUsize>) {
    let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let rewriting = Planned {
        operation: kyoto(1.5),
        pipeline: Arc::new(Rewriting { fails, asked: Arc::clone(&asked) }) as crate::pipeline::Shared<Fake>,
    };
    let mut planned = vec![rewriting];
    planned.extend(plan::<Fake>(&[kyoto(1.5), kyoto(2.0)]).expect("planned").into_iter().skip(1));
    (planned, asked)
}

/// `run` over `planned` from a 48 x 32 picture carrying `gain_map`, through `cache`: the gain map it hands back.
async fn gain_map_after(
    backend: &Fake,
    planned: &[Planned<Fake>],
    gain_map: Option<Arc<GainMap>>,
    cache: Option<&RunCache>,
) -> Option<Arc<GainMap>> {
    let ran = run(
        backend,
        source(48, 32),
        gain_map,
        planned,
        Steps { provider: ExecutionProvider::Auto, depth: ChannelDepth::Eight, keep: 0, from: 0, part: None },
        None,
        &CancellationToken::new(),
        cache,
    )
    .await
    .map_err(|failure| failure.error)
    .expect("ran");
    settled();
    ran.gain_map
}

fn inverted(map: &GainMap) -> DynamicImage {
    let mut pixels = map.pixels().clone();
    pixels.invert();
    pixels
}

#[tokio::test]
async fn a_rewritten_gain_map_is_handed_on_by_the_following_steps_and_onto_the_result() {
    let (planned, asked) = rewriting_then_kyoto(false);
    let source_map = ramp_gain_map();

    let result = gain_map_after(&Fake::new(), &planned, Some(Arc::clone(&source_map)), None)
        .await
        .expect("carried");

    assert_eq!(asked.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert!(!Arc::ptr_eq(&result, &source_map));
    assert_eq!(result.pixels(), &inverted(&source_map), "the upscale after it kept the rewritten one");
    assert_eq!(result.metadata().xmp, source_map.metadata().xmp);

    // No gain map, nothing asked.
    let (planned, asked) = rewriting_then_kyoto(false);
    assert!(gain_map_after(&Fake::new(), &planned, None, None).await.is_none());
    assert_eq!(asked.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[tokio::test]
async fn a_gain_map_that_could_not_be_rewritten_is_dropped_with_a_warning_and_the_run_goes_on() {
    let (planned, _) = rewriting_then_kyoto(true);
    let backend = Fake::new();

    let (log, map) =
        logging::records_of("warn", || gain_map_after(&backend, &planned, Some(ramp_gain_map()), None)).await;

    assert!(map.is_none());
    let [record] = records(&log, "HDR gain map dropped: it could not be carried")[..] else {
        panic!("one warning: {log}");
    };
    assert_eq!(field(record, "index"), Some("0"), "{record}");
}

#[tokio::test]
async fn a_dropped_gain_map_is_stored_so_a_second_run_is_served_and_drops_it_too() {
    let cache = RunCache::new(store(), "cafebabecafebabe");
    let backend = Fake::new();
    let (planned, asked) = rewriting_then_kyoto(true);

    assert!(gain_map_after(&backend, &planned, Some(ramp_gain_map()), Some(&cache)).await.is_none());
    let ran = backend.log().ran.len();
    let (log, map) =
        logging::records_of("warn", || gain_map_after(&backend, &planned, Some(ramp_gain_map()), Some(&cache))).await;

    assert!(map.is_none());
    assert_eq!(asked.load(std::sync::atomic::Ordering::Relaxed), 1, "the step that dropped it ran again");
    assert_eq!(backend.log().ran.len(), ran, "the second run was not served from the store");
    let [record] = records(&log, "HDR gain map dropped: it could not be carried")[..] else {
        panic!("one warning: {log}");
    };
    assert!(record.contains("it would not"), "the stored reason: {record}");
}

#[tokio::test]
async fn a_rewritten_gain_map_is_served_from_the_store_and_a_step_stored_without_one_runs_again() {
    let memo = store();
    let cache = RunCache::new(memo.clone(), "cafebabecafebabe");
    let backend = Fake::new();
    let source_map = ramp_gain_map();

    let (planned, asked) = rewriting_then_kyoto(false);
    let first = gain_map_after(&backend, &planned, Some(Arc::clone(&source_map)), Some(&cache))
        .await
        .expect("one");
    let ran = backend.log().ran.len();
    let second = gain_map_after(&backend, &planned, Some(Arc::clone(&source_map)), Some(&cache))
        .await
        .expect("one");

    assert_eq!(asked.load(std::sync::atomic::Ordering::Relaxed), 1, "the second run rewrote it again");
    assert_eq!(backend.log().ran.len(), ran, "the second run was not served from the store");
    assert_eq!(second.pixels(), first.pixels());
    assert_eq!(second.metadata().xmp, source_map.metadata().xmp);

    // A store that holds the steps' pixels alone, as one written for a picture with no gain map is.
    let (memo, backend) = (store(), Fake::new());
    let cache = RunCache::new(memo, "cafebabecafebabe");
    let (planned, asked) = rewriting_then_kyoto(false);
    assert!(gain_map_after(&backend, &planned, None, Some(&cache)).await.is_none());
    let ran = backend.log().ran.len();

    let map = gain_map_after(&backend, &planned, Some(Arc::clone(&source_map)), Some(&cache))
        .await
        .expect("one");

    assert_eq!(
        asked.load(std::sync::atomic::Ordering::Relaxed),
        1,
        "the step stored without its gain map was served"
    );
    assert_eq!(map.pixels(), &inverted(&source_map));
    // The upscale after it is still served: the step that ran again produced the pixels it had stored.
    assert_eq!(backend.log().ran.len(), ran);
}

/// A 960 x 640 checkerboard carrying a full-resolution gain map that lifts a highlight in the corner, where the fake
/// depth puts the scene far from a focus at the centre.
fn checkered_with_highlight() -> Picture {
    let pixels = image::RgbImage::from_fn(960, 640, |x, y| {
        if (x / 4 + y / 4) % 2 == 0 { image::Rgb([230, 200, 40]) } else { image::Rgb([20, 40, 120]) }
    });
    let map = image::RgbImage::from_fn(960, 640, |x, y| {
        let lit = (x as f32 - 100.0).hypot(y as f32 - 100.0) <= 3.0;
        if lit { image::Rgb([255, 255, 255]) } else { image::Rgb([40, 40, 40]) }
    });
    let gain_map = GainMap::holding(DynamicImage::ImageRgb8(map), (960, 640));

    Picture::new("/pictures/holiday.jpg", DynamicImage::ImageRgb8(pixels), "cafebabecafebabe")
        .with_gain_map(Some(gain_map))
}

#[tokio::test]
async fn a_lens_blur_rewrites_the_gain_map_fresh_or_cached_and_a_denoise_leaves_it_shared() {
    let (backend, cache) = (Fake::new(), store());
    let source = checkered_with_highlight();
    let source_map = source.shared_gain_map().expect("carried");
    let blur = crate::models::LensBlur::lisbon(
        crate::models::FNumber::new(1.4).expect("in range"),
        crate::models::Fraction::new(0.5).expect("in range"),
        crate::models::Fraction::new(0.5).expect("in range"),
    );

    let fresh = cached_run(&backend, Some(&cache), &source, std::slice::from_ref(&blur), OutputDepth::Eight)
        .await
        .expect("ran");
    settled();
    let ran = backend.log().ran.len();
    let cached = cached_run(&backend, Some(&cache), &source, &[blur], OutputDepth::Eight).await.expect("ran");
    assert_eq!(backend.log().ran.len(), ran);

    let (fresh_map, cached_map) = (fresh.gain_map().expect("carried"), cached.gain_map().expect("carried"));
    assert_ne!(fresh_map.pixels(), source_map.pixels(), "the lens blur left the gain map as it was");
    assert_eq!(cached_map.pixels(), fresh_map.pixels(), "the cached result carries another gain map");
    // The highlight spread over a disc: more of the gain map lifts than did.
    let lifted = |map: &GainMap| map.pixels().to_rgb8().pixels().filter(|pixel| pixel.0[1] > 42).count();
    assert!(
        lifted(fresh_map) > 2 * lifted(&source_map),
        "{} against {}",
        lifted(fresh_map),
        lifted(&source_map)
    );

    let denoised = cached_run(
        &backend,
        Some(&cache),
        &source,
        &[crate::models::Denoise::stockholm(
            FloatPrecision::Fp32,
            crate::models::Strength::new(0.5).expect("in range"),
        )],
        OutputDepth::Eight,
    )
    .await
    .expect("ran");
    assert!(Arc::ptr_eq(&denoised.shared_gain_map().expect("kept"), &source_map));
}
