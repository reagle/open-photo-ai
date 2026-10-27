//! The lens blur's pipeline: no graph, a depth map and a lens in, the photograph with the depth of field a wider
//! aperture would have given it out.

// An `ImagePipeline` with no session of its own. Its required artifacts are the depth model's, so installing what a
// chain needs and the "needs a model" checks work as for any family, while `sessions` names none and the chain's
// acquisition opens nothing for it: the depth was estimated, through its own cached run, before the chain began.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use image::{DynamicImage, GenericImageView as _};
use imaging::ChannelDepth;
use imaging::defocus::{Defocus, defocus};
use imaging::guided::{dilated_nearer, upsampled};
use imaging::linear::{decoded_linear, encoded};
use imaging::plane::Plane;
use imaging::rows::for_each_row;
use imaging::tensor::{Sampler, flattened};

use super::gain_map::{box_reduced, largest_reduced, rewritten};
use super::lens::{Blur, Lens};
use crate::error::InferenceError;
use crate::image::{GainMap, GainMapParams};
use crate::models::{ArtifactId, DepthMap};
use crate::pipeline::{Backend, ImagePipeline, Model, checkpoint, reporter};
use crate::providers::profile::EpProfile;
use crate::sessions::SessionHandle;

/// The diagonal, in pixels, the guided filter's and the edge grow's radii are stated at: a 6000 x 4000 photograph's,
/// near enough.
const REFERENCE_DIAGONAL: f32 = 7200.0;

/// The guided filter's window radius at [`REFERENCE_DIAGONAL`]: wide enough to follow a subject's outline across the
/// depth model's soft 1280-pixel edge, narrow enough not to pull a distant edge in.
const GUIDE_RADIUS: f32 = 24.0;

/// The guided filter's regularisation: an edge in the photograph fainter than this variance is treated as flat.
const GUIDE_EPS: f32 = 1e-3;

/// The edge grow's radius at [`REFERENCE_DIAGONAL`]: how far the nearer side of a depth edge is pulled out, so a pixel
/// that mixes a subject and the background behind it stays with the sharp subject.
const EDGE_GROW: f32 = 3.0;

/// The share of the run's progress the depth's refinement takes, and the share encoding the result takes after the
/// layers.
const REFINE_SHARE: f64 = 0.1;
const ENCODE_SHARE: f64 = 0.1;

/// What a run is asked for: the aperture, the focus point and the photograph's own lens.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Settings {
    /// The equivalent f-number to simulate.
    pub(crate) f_number: f32,
    /// The focus point, as fractions across and down the frame.
    pub(crate) focus: (f64, f64),
    /// The photograph's own lens, in equivalent terms.
    pub(crate) lens: Lens,
}

/// The lens blur, as the chain holds it.
pub(crate) struct LensBlurPipeline {
    /// The operation's display name, for naming it in a failure.
    name: String,
    /// The depth model's graphs, for [`Model::required`].
    required: Vec<ArtifactId>,
    settings: Settings,
    /// The depth of the chain's input, or `None` for a blur the chain knew adds nothing and so never estimated one.
    depth: Option<Arc<DepthMap>>,
    keep_sharp: Option<Arc<Plane>>,
}

impl LensBlurPipeline {
    pub(crate) fn new(
        name: String,
        required: Vec<ArtifactId>,
        settings: Settings,
        depth: Option<Arc<DepthMap>>,
        keep_sharp: Option<Arc<Plane>>,
    ) -> Self {
        Self { name, required, settings, depth, keep_sharp }
    }

    /// The signed radius every pixel of the `input` is blurred by, or `None` where the blur adds nothing anywhere.
    ///
    /// Decided before anything is built from the photograph: the guide the depth is refined along is a float copy of
    /// every pixel, which a blur at or above the photograph's own f-number would have made for nothing.
    fn radii(&self, input: &DynamicImage) -> Option<Plane> {
        let (width, height) = (input.width(), input.height());
        let diagonal = (width as f32).hypot(height as f32);
        // None where the chain knew the blur adds nothing and estimated no depth, which `Blur::new` then agrees with.
        let depth = self.depth.as_ref()?;
        let (fx, fy) = self.settings.focus;
        // Read from the map as estimated, before the edge grow moves any edge: the focus is the depth at the point.
        let focus = depth.focus_metres(fx, fy);
        let blur = Blur::new(&self.settings.lens, self.settings.f_number, focus, diagonal)?;

        let guide = srgb(input);

        // In disparity, which the radius is linear in, normalised to `0..=1` for the filter's regularisation.
        let disparity = depth.disparity();
        let nearest = disparity.iter().copied().fold(f32::MIN_POSITIVE, f32::max);
        let (map_width, map_height) = depth.dimensions();
        let low = Plane::from_vec(map_width, map_height, disparity.iter().map(|value| value / nearest).collect())
            .expect("a depth map holds one distance a pixel");

        let scale = diagonal / REFERENCE_DIAGONAL;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a radius of a few dozen pixels"
        )]
        let radius = (GUIDE_RADIUS * scale).round().max(1.0) as u32;
        let refined = upsampled(&low, &guide, width, height, radius, GUIDE_EPS);
        let mut grown = dilated_nearer(&refined, (EDGE_GROW * scale).max(1.0));

        for value in grown.as_mut_slice() {
            *value = blur.radius(*value * nearest);
        }

        // What explains a result: where it focused, and how far the discs reach either side of the plane of focus.
        let (behind, front) =
            grown.as_slice().iter().fold((0.0_f32, 0.0_f32), |(low, high), &r| (low.min(r), high.max(r)));
        tracing::debug!(operation = %self.name, focus_m = focus, behind, front, "lens blur radii");

        Some(grown)
    }
}

impl<B: Backend> Model<B> for LensBlurPipeline {
    fn required(&self) -> &[ArtifactId] {
        &self.required
    }

    /// None: the depth was estimated before the chain began, and the blur runs no graph.
    fn sessions(&self) -> Vec<(&ArtifactId, &EpProfile)> {
        Vec::new()
    }
}

impl<B: Backend> ImagePipeline<B> for LensBlurPipeline {
    /// No model run at all.
    fn stages(&self) -> usize {
        0
    }

    fn run(
        &self,
        input: &DynamicImage,
        _sessions: &[SessionHandle<B::Session>],
        depth: ChannelDepth,
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<DynamicImage, InferenceError> {
        self.render(input, None, depth, progress, cancelled).map(|(image, _)| image)
    }

    fn rewrites_gain_map(&self, gain_map: &GainMap) -> bool {
        !matches!(self.rewrite(gain_map), Rewrite::Nothing)
    }

    /// The photograph blurred and its gain map with it, at the gain map's own resolution (the series design's H2): the
    /// HDR rendition rebuilt from the photograph in linear light and the gain map, both blurred by this run's radii,
    /// and their ratio encoded under the source's parameters.
    fn run_with_gain_map(
        &self,
        input: &DynamicImage,
        gain_map: &GainMap,
        _sessions: &[SessionHandle<B::Session>],
        depth: ChannelDepth,
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(DynamicImage, Option<Result<GainMap, InferenceError>>), InferenceError> {
        self.render(input, Some(gain_map), depth, progress, cancelled)
    }
}

/// What a lens blur does with a picture's gain map.
enum Rewrite {
    /// Nothing: the blur adds nothing, so the gain map is still true of the result.
    Nothing,
    /// Drops it, for this reason: what its values mean could not be read or is not what this rebuilds from, and left
    /// as it was it would lift the old, sharp highlights over a blurred base.
    Drop(String),
    /// Rewrites it under these parameters.
    Under(GainMapParams),
}

impl LensBlurPipeline {
    /// What this blur does with `gain_map`. Settled without the photograph, so the run cache can ask it of a step it
    /// serves; a gain map larger than the photograph, which only the photograph can show, is refused by the render.
    fn rewrite(&self, gain_map: &GainMap) -> Rewrite {
        if self.depth.is_none() || Blur::adds_nothing(&self.settings.lens, self.settings.f_number) {
            return Rewrite::Nothing;
        }
        match gain_map.params() {
            Err(error) => Rewrite::Drop(error.to_string()),
            Ok(params) if params.base_is_hdr => {
                Rewrite::Drop("its base rendition is the HDR one, which this does not rebuild from".to_string())
            }
            // The ratio is taken in the base's linear RGB, which is right for one gain on all three channels and for
            // three in the base's colour space, and not for three in the HDR rendition's primaries, which no
            // converter here knows.
            Ok(params) if params.channels == 3 && !params.in_base_colour_space => Rewrite::Drop(
                "its three channels apply in the HDR rendition's colour space, which this does not convert to"
                    .to_string(),
            ),
            Ok(params) => Rewrite::Under(params),
        }
    }

    /// The error that drops a gain map, for `reason`.
    fn dropped(&self, reason: String) -> InferenceError {
        InferenceError::GainMap { operation: self.name.clone(), reason }
    }

    /// The blurred photograph at `depth`, and what became of `gain_map`: `run` and `run_with_gain_map` both.
    ///
    /// A gain map that is rewritten is given a share of the bar by the pixels it renders against the photograph's, and
    /// reuses what the photograph's render computed: the radii, brought to its size, and the photograph in linear light
    /// — and, at the photograph's own size, its render too.
    fn render(
        &self,
        input: &DynamicImage,
        gain_map: Option<&GainMap>,
        depth: ChannelDepth,
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(DynamicImage, Option<Result<GainMap, InferenceError>>), InferenceError> {
        let (width, height) = (input.width(), input.height());
        if width == 0 || height == 0 {
            return Err(InferenceError::Untileable { width, height });
        }

        let report = reporter(progress);
        if cancelled() {
            return Err(InferenceError::Cancelled);
        }

        let Some(radius) = self.radii(input) else {
            // At or above the photograph's own f-number: the photograph as it is, at the depth asked for, and never
            // decoded to linear light and back. `rewrite` answered `Nothing` for its gain map on the same two numbers.
            report(1.0);
            return Ok((unchanged(input, depth), None));
        };

        // Settled before anything is rendered, so a rewrite can be given its share of the bar.
        let (rewriting, dropped) = match gain_map.map(|map| (map, self.rewrite(map))) {
            None | Some((_, Rewrite::Nothing)) => (None, None),
            Some((_, Rewrite::Drop(reason))) => (None, Some(self.dropped(reason))),
            Some((map, Rewrite::Under(params))) => {
                let (map_width, map_height) = map.pixels().dimensions();
                if map_width == 0 || map_height == 0 || map_width > width || map_height > height {
                    let reason = format!("it is {map_width}x{map_height}, larger than the {width}x{height} photograph");
                    (None, Some(self.dropped(reason)))
                } else {
                    (Some((map, params)), None)
                }
            }
        };

        // The rewrite's share against the photograph's render, by the pixels it renders.
        let map_renders = rewriting.map_or(0.0, |(map, _)| renders(map.pixels(), (width, height)));
        let weight = rewriting.map_or(0.0, |(map, _)| {
            let (map_width, map_height) = map.pixels().dimensions();
            map_renders * f64::from(map_width) * f64::from(map_height) / (f64::from(width) * f64::from(height))
        });
        let rendering = 1.0 - REFINE_SHARE - ENCODE_SHARE;
        let blur_share = rendering / (1.0 + weight);

        checkpoint(&report, cancelled, REFINE_SHARE)?;

        let (linear, _) = decoded_linear(input);

        // The render asks whether to stop before each layer, which is the one place it can be heard from: each
        // question after the first is a layer done. About `layers + 1` of them a render, one being the plane of focus.
        let layers = (Defocus::DEFAULT.layers + 1) as f64;
        let per_layer = |from: f64, share: f64, renders: f64| {
            let asked = AtomicUsize::new(0);
            let report = &report;
            move || {
                let done = asked.fetch_add(1, Ordering::Relaxed) as f64;
                report(from + share * (done / (layers * renders)).min(1.0));
                cancelled()
            }
        };

        let blurred = defocus(
            &linear,
            width,
            height,
            &radius,
            self.keep_sharp.as_deref(),
            Defocus::DEFAULT,
            &per_layer(REFINE_SHARE, blur_share, 1.0),
        )
        .ok_or(InferenceError::Cancelled)?;

        let gain_map = match rewriting {
            Some((map, params)) => {
                let progress = per_layer(REFINE_SHARE + blur_share, rendering - blur_share, map_renders);
                Some(Ok(self.rewritten_gain_map(map, &params, linear, &blurred, radius, &progress)?))
            }
            None => {
                drop(linear);
                dropped.map(Err)
            }
        };

        checkpoint(&report, cancelled, 1.0 - ENCODE_SHARE)?;

        let result = encoded(&blurred, width, height, depth);
        report(1.0);

        tracing::debug!(operation = %self.name, width, height, "lens blur rendered");

        Ok((result, gain_map))
    }

    /// `gain_map` rewritten under `params` for the photograph `linear` (in linear light) that `radius` blurred into
    /// `blurred`: at the photograph's size from both as they are, and at a smaller one from `linear` box-reduced to it
    /// and `radius` reduced by its largest blur a cell and scaled by the ratio of the diagonals.
    ///
    /// # Errors
    ///
    /// [`InferenceError::Cancelled`] where `cancelled`, asked before each layer, answers true.
    fn rewritten_gain_map(
        &self,
        gain_map: &GainMap,
        params: &GainMapParams,
        linear: Vec<f32>,
        blurred: &[f32],
        radius: Plane,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<GainMap, InferenceError> {
        let started = std::time::Instant::now();
        let (width, height) = (radius.width(), radius.height());
        let map = gain_map.pixels();
        let (map_width, map_height) = (map.width(), map.height());

        let pixels = if (map_width, map_height) == (width, height) {
            rewritten(map, linear, Some(blurred), params, &radius, self.keep_sharp.as_deref(), cancelled)
        } else {
            // The radii are in the photograph's pixels: a gain map a quarter of its size blurs by a quarter as many.
            let scale = (map_width as f32).hypot(map_height as f32) / (width as f32).hypot(height as f32);
            let mut reduced = largest_reduced(radius.as_slice(), (width, height), (map_width, map_height));
            for value in &mut reduced {
                *value *= scale;
            }
            let radius = Plane::from_vec(map_width, map_height, reduced).expect("one radius a pixel");
            let sdr = box_reduced(&linear, 3, (width, height), (map_width, map_height));
            drop(linear);
            rewritten(map, sdr, None, params, &radius, self.keep_sharp.as_deref(), cancelled)
        };
        let pixels = pixels.ok_or(InferenceError::Cancelled)?;

        tracing::debug!(
            operation = %self.name,
            map_width,
            map_height,
            elapsed = ?started.elapsed(),
            "lens blur gain map rewritten"
        );

        Ok(gain_map.rewritten(pixels))
    }
}

/// How many renders rewriting `map` takes beside a photograph of `size`: the HDR rendition at the gain map's size, and
/// the SDR one too where that is not the photograph's, whose render is reused, and the map has three channels.
fn renders(map: &DynamicImage, size: (u32, u32)) -> f64 {
    if map.dimensions() != size && map.color().has_color() { 2.0 } else { 1.0 }
}

/// `input`'s sRGB values in `0..=1`, three interleaved a pixel: what the depth is refined along.
fn srgb(input: &DynamicImage) -> Vec<f32> {
    let (width, height) = (input.width() as usize, input.height() as usize);
    let sampler = Sampler::new(input);
    let mut guide = vec![0.0_f32; 3 * width * height];

    for_each_row(&mut guide, 3 * width, height, |y, out| {
        let mut pixels = vec![[0_u16; 3]; width];
        #[allow(clippy::cast_possible_truncation, reason = "a row index of a picture whose height is a u32")]
        sampler.row(y as u32, &mut pixels);

        for (value, sample) in out.iter_mut().zip(pixels.iter().flatten()) {
            *value = f32::from(*sample) / f32::from(u16::MAX);
        }
    });

    guide
}

/// `input` unchanged, at `depth`, read as every pipeline reads a photograph (alpha composited against black).
fn unchanged(input: &DynamicImage, depth: ChannelDepth) -> DynamicImage {
    let sampler = Sampler::new(input);
    let (width, height) = (input.width(), input.height());

    match depth {
        ChannelDepth::Eight => DynamicImage::ImageRgb8(flattened(&sampler, width, height)),
        ChannelDepth::Sixteen => DynamicImage::ImageRgb16(flattened(&sampler, width, height)),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use image::{Rgb, RgbImage};

    use rust_sak::image::Metadata;

    use super::super::lens::LensOrigin;
    use super::super::tests::two_planes;
    use super::*;
    use crate::image::test_support::{HAWK_GAIN_MAP_ISO, HAWK_GAIN_MAP_XMP};
    use crate::pipeline::test_support::NoBackend;

    /// A checkerboard of 4-pixel squares: detail any blur visibly removes.
    fn checkerboard(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
            if (x / 4 + y / 4) % 2 == 0 { Rgb([230, 200, 40]) } else { Rgb([20, 40, 120]) }
        }))
    }

    /// A pipeline over a two-plane map — a square at 1 m before a background at 10 m — focused on the square, for a
    /// 36 mm f/6 photograph, at `f_number`.
    fn pipeline(f_number: f32, keep_sharp: Option<Arc<Plane>>) -> LensBlurPipeline {
        LensBlurPipeline::new(
            "Lisbon (FP16)".to_string(),
            Vec::new(),
            Settings { f_number, focus: (0.5, 0.5), lens: Lens::new(36.0, 6.0, LensOrigin::Exif) },
            Some(Arc::new(two_planes(96, 72, 1.0, 10.0))),
            keep_sharp,
        )
    }

    fn run(pipeline: &LensBlurPipeline, input: &DynamicImage) -> DynamicImage {
        ImagePipeline::<NoBackend>::run(pipeline, input, &[], ChannelDepth::Eight, None, &|| false)
            .expect("the blur runs")
    }

    /// How far `region` of `result` strays from the same region of `input`, as a mean absolute difference per channel.
    fn difference(input: &DynamicImage, result: &DynamicImage, region: (u32, u32, u32, u32)) -> f64 {
        let (input, result) = (input.to_rgb8(), result.to_rgb8());
        let (x0, y0, x1, y1) = region;
        let mut total = 0.0;
        let mut count = 0.0;

        for y in y0..y1 {
            for x in x0..x1 {
                for channel in 0..3 {
                    total +=
                        (f64::from(input.get_pixel(x, y)[channel]) - f64::from(result.get_pixel(x, y)[channel])).abs();
                    count += 1.0;
                }
            }
        }

        total / count
    }

    /// The square's middle, well inside it, and a band of the background along the top.
    const SQUARE: (u32, u32, u32, u32) = (130, 100, 190, 140);
    const BACKGROUND: (u32, u32, u32, u32) = (0, 0, 320, 40);

    #[test]
    fn the_subject_in_focus_stays_sharp_and_the_background_is_blurred() {
        let input = checkerboard(320, 240);
        let result = run(&pipeline(1.4, None), &input);

        assert_eq!(result.dimensions(), input.dimensions());
        assert!(difference(&input, &result, SQUARE) < 2.0, "{}", difference(&input, &result, SQUARE));
        assert!(difference(&input, &result, BACKGROUND) > 30.0, "{}", difference(&input, &result, BACKGROUND));

        // Wider is blurrier.
        let narrower = run(&pipeline(4.0, None), &input);
        assert!(difference(&input, &narrower, BACKGROUND) < difference(&input, &result, BACKGROUND));
    }

    #[test]
    fn nothing_changes_at_or_above_the_photographs_own_f_number() {
        let input = checkerboard(320, 240);

        for f_number in [6.0, 8.0] {
            assert_eq!(run(&pipeline(f_number, None), &input).to_rgb8(), input.to_rgb8(), "f/{f_number}");
        }

        // At the depth asked for, still unchanged.
        let sixteen =
            ImagePipeline::<NoBackend>::run(&pipeline(8.0, None), &input, &[], ChannelDepth::Sixteen, None, &|| false)
                .expect("runs");
        assert_eq!(sixteen.to_rgb8(), input.to_rgb8());
        assert!(matches!(sixteen, DynamicImage::ImageRgb16(_)));
    }

    #[test]
    fn a_keep_sharp_weight_keeps_what_it_covers_sharp() {
        let input = checkerboard(320, 240);
        let everything = Arc::new(Plane::new(8, 6, 1.0));

        let result = run(&pipeline(1.4, Some(everything)), &input);
        assert!(difference(&input, &result, BACKGROUND) < 1.0);
    }

    #[test]
    fn the_progress_rises_to_one_and_a_stop_produces_no_image() {
        let input = checkerboard(320, 240);
        let seen = Mutex::new(Vec::new());
        let record = |fraction: f64| seen.lock().unwrap().push(fraction);

        ImagePipeline::<NoBackend>::run(&pipeline(1.4, None), &input, &[], ChannelDepth::Eight, Some(&record), &|| {
            false
        })
        .expect("runs");

        let seen = seen.into_inner().unwrap();
        assert!(seen.windows(2).all(|pair| pair[1] >= pair[0]), "{seen:?}");
        assert_eq!(seen.last(), Some(&1.0));
        assert!(seen.len() > 4, "the layers reported nothing: {seen:?}");

        let stopped =
            ImagePipeline::<NoBackend>::run(&pipeline(1.4, None), &input, &[], ChannelDepth::Eight, None, &|| true);
        assert!(matches!(stopped, Err(InferenceError::Cancelled)));
    }

    #[test]
    fn the_radii_double_on_an_output_twice_the_size() {
        let blur = pipeline(1.4, None);
        let small = checkerboard(320, 240);
        let large = checkerboard(640, 480);

        let small = blur.radii(&small).expect("blurs");
        let large = blur.radii(&large).expect("blurs");

        // The background's radius, far from any edge, where neither the refinement nor the grow reaches.
        let (far_small, far_large) = (small.get(4, 4), large.get(8, 8));
        assert!(far_small < -1.0, "{far_small}");
        assert!((far_large - 2.0 * far_small).abs() < 1e-3 * far_small.abs(), "{far_small} {far_large}");
        assert!(small.get(160, 120).abs() < 1e-3, "the subject in focus has a radius");
    }

    /// A dim grey photograph of 640 x 480, and a gain map lifting a small highlight in the background, at (120, 120) in
    /// the photograph's pixels, and a second one on the square in focus, at (320, 240): at `1 / reduction` of the
    /// photograph's resolution.
    fn highlighted(reduction: u32) -> (DynamicImage, GainMap) {
        let input = DynamicImage::ImageRgb8(RgbImage::from_pixel(640, 480, Rgb([60, 60, 60])));
        let (width, height) = (640 / reduction, 480 / reduction);
        let lit = |x: u32, y: u32, (cx, cy): (u32, u32)| {
            let (x, y) = ((x * reduction + reduction / 2) as f32, (y * reduction + reduction / 2) as f32);
            (x - cx as f32).hypot(y - cy as f32) <= 2.0 * reduction as f32
        };
        let map = DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
            if lit(x, y, (120, 120)) || lit(x, y, (320, 240)) {
                Rgb([255, 255, 255])
            } else {
                Rgb([40, 40, 40])
            }
        }));

        (input, GainMap::holding(map, (640, 480)))
    }

    /// The blur of `input` carrying `gain_map`, and what became of the gain map.
    fn carried(
        pipeline: &LensBlurPipeline,
        input: &DynamicImage,
        gain_map: &GainMap,
        progress: Option<&dyn Fn(f64)>,
    ) -> (DynamicImage, Option<Result<GainMap, InferenceError>>) {
        ImagePipeline::<NoBackend>::run_with_gain_map(
            pipeline,
            input,
            gain_map,
            &[],
            ChannelDepth::Eight,
            progress,
            &|| false,
        )
        .expect("the blur runs")
    }

    fn rewritten_map(pipeline: &LensBlurPipeline, input: &DynamicImage, gain_map: &GainMap) -> Option<GainMap> {
        let (pixels, rewritten) = carried(pipeline, input, gain_map, None);
        assert_eq!(pixels, run(pipeline, input), "the photograph is blurred as without a gain map");
        rewritten.map(|rewritten| rewritten.expect("the gain map is rewritten"))
    }

    /// The highlight scenario at `1 / reduction` of the photograph's resolution: dimmer than the original peak at its
    /// centre, lifted out to about the radius of the blur there, and not beyond it, and the highlight in focus and the
    /// square around it unchanged but for rounding.
    fn blooms(reduction: u32) {
        let blur = pipeline(1.0, None);
        let (input, gain_map) = highlighted(reduction);
        assert!(ImagePipeline::<NoBackend>::rewrites_gain_map(&blur, &gain_map));
        let radius = -blur.radii(&input).expect("blurs").get(120, 120) / reduction as f32;
        assert!(radius > 2.5, "{radius}");

        let rewritten = rewritten_map(&blur, &input, &gain_map).expect("rewritten");
        assert_eq!(rewritten.metadata().xmp, gain_map.metadata().xmp, "the parameters are kept");
        assert_eq!(rewritten.scale(), gain_map.scale());
        assert_eq!(rewritten.source(), None);
        assert_eq!(rewritten.pixels().dimensions(), gain_map.pixels().dimensions());
        assert!(matches!(rewritten.pixels(), DynamicImage::ImageRgb8(_)));

        let (before, after) = (gain_map.pixels().to_rgb8(), rewritten.pixels().to_rgb8());
        let centre = 120.0 / reduction as f32;
        let at = |out: f32| after.get_pixel((centre + out).round() as u32, centre as u32).0[1];
        assert!(at(0.0) < 255 && at(0.0) > 40, "the centre: {}", at(0.0));
        assert!(at(radius * 0.6) > 41, "inside the disc: {}", at(radius * 0.6));
        assert!(at(radius + 2.0 + 3.0 / reduction as f32) <= 41, "outside it: {}", at(radius + 3.0));

        let r = reduction;
        for (x, y) in [(320 / r, 240 / r), (321 / r, 241 / r), (260 / r, 200 / r), (380 / r, 280 / r)] {
            let (was, is) = (before.get_pixel(x, y).0, after.get_pixel(x, y).0);
            assert!(was.iter().zip(is).all(|(was, is)| was.abs_diff(is) <= 1), "({x}, {y}): {was:?} to {is:?}");
        }
    }

    #[test]
    fn a_highlight_in_the_background_blooms_into_a_disc_and_one_in_focus_stays() {
        blooms(1);
    }

    #[test]
    fn a_quarter_resolution_gain_map_is_blurred_at_its_own_size() {
        blooms(4);
    }

    #[test]
    fn a_single_channel_gain_map_stays_single_channel() {
        let (input, _) = highlighted(1);
        let pixels = DynamicImage::ImageLuma8(image::GrayImage::from_pixel(160, 120, image::Luma([90])));
        let gray = GainMap::holding(pixels, (640, 480));

        let rewritten = rewritten_map(&pipeline(1.4, None), &input, &gray).expect("rewritten");

        assert!(matches!(rewritten.pixels(), DynamicImage::ImageLuma8(_)));
        assert!(rewritten.pixels().to_luma8().pixels().all(|pixel| pixel.0[0].abs_diff(90) <= 1));
    }

    #[test]
    fn a_gain_map_is_left_as_it_was_where_nothing_blurs() {
        let (input, gain_map) = highlighted(1);

        // At the photograph's own f-number.
        let unchanged = pipeline(6.0, None);
        assert!(!ImagePipeline::<NoBackend>::rewrites_gain_map(&unchanged, &gain_map));
        assert!(rewritten_map(&unchanged, &input, &gain_map).is_none());
    }

    #[test]
    fn a_gain_map_is_dropped_where_its_parameters_do_not_read_its_base_is_hdr_or_it_is_larger_than_the_photograph() {
        let (input, gain_map) = highlighted(1);
        let blur = pipeline(1.4, None);
        let blurred = run(&blur, &input);

        let mut unreadable = Metadata::default();
        unreadable.xmp = Some(b"<x:xmpmeta xmlns:x='adobe:ns:meta/'/>".to_vec());
        let hdr_base = String::from_utf8_lossy(HAWK_GAIN_MAP_XMP).replace("\"False\"", "\"True\"");
        let mut hdr = Metadata::default();
        hdr.xmp = Some(hdr_base.into_bytes());
        let mut alternate_space = HAWK_GAIN_MAP_ISO.to_vec();
        alternate_space[4] &= !0x40;
        let mut alternate = Metadata::default();
        alternate.iso_gain_map = Some(alternate_space);

        let cases = [
            ("unreadable", gain_map.with_metadata(unreadable), "its hdrgm XMP is unreadable"),
            ("HDR base", gain_map.with_metadata(hdr), "its base rendition is the HDR one"),
            ("alternate colour space", gain_map.with_metadata(alternate), "the HDR rendition's colour space"),
            (
                "larger",
                GainMap::holding(DynamicImage::new_rgb8(641, 480), (641, 480)),
                "641x480, larger than the 640x480",
            ),
        ];
        for (name, other, reason) in cases {
            assert!(ImagePipeline::<NoBackend>::rewrites_gain_map(&blur, &other), "{name}");
            let (pixels, dropped) = carried(&blur, &input, &other, None);
            assert_eq!(pixels, blurred, "{name}: the photograph is blurred all the same");
            let Some(Err(error @ InferenceError::GainMap { .. })) = dropped else {
                panic!("{name}: {dropped:?}");
            };
            assert!(error.to_string().contains(reason), "{name}: {error}");
        }
    }

    #[test]
    fn a_rewrite_has_its_own_share_of_the_bar_after_the_photographs_render() {
        let (input, gain_map) = highlighted(1);
        let blur = pipeline(1.4, None);
        let reports = |with_gain_map: bool| {
            let seen = Mutex::new(Vec::new());
            let progress = |fraction: f64| seen.lock().expect("unpoisoned").push(fraction);
            if with_gain_map {
                carried(&blur, &input, &gain_map, Some(&progress));
            } else {
                ImagePipeline::<NoBackend>::run(&blur, &input, &[], ChannelDepth::Eight, Some(&progress), &|| false)
                    .expect("the blur runs");
            }
            seen.into_inner().expect("unpoisoned")
        };

        let (without, with) = (reports(false), reports(true));

        for seen in [&without, &with] {
            assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]), "{seen:?}");
            assert_eq!(seen.last(), Some(&1.0));
        }
        // A full-resolution map renders as many pixels again, so the photograph's render ends half way through the
        // rendering share, and the rewrite reports its own layers after it.
        let rendering = |seen: &[f64]| seen.iter().filter(|&&f| f > REFINE_SHARE && f < 1.0 - ENCODE_SHARE).count();
        assert!(rendering(&with) > rendering(&without) + 5, "{without:?} against {with:?}");
        let half = REFINE_SHARE + (1.0 - REFINE_SHARE - ENCODE_SHARE) / 2.0;
        assert!(with.iter().any(|&f| f > half + 0.05 && f < 1.0 - ENCODE_SHARE), "{with:?}");
    }
}
