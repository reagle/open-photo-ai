//! The lens blur's pipeline: no graph, a depth map and a lens in, the photograph with the depth of field a wider
//! aperture would have given it out.

// An `ImagePipeline` with no session of its own. Its required artifacts are the depth model's, so installing what a
// chain needs and the "needs a model" checks work as for any family, while `sessions` names none and the chain's
// acquisition opens nothing for it: the depth was estimated, through its own cached run, before the chain began.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use image::DynamicImage;
use imaging::ChannelDepth;
use imaging::defocus::{Defocus, defocus};
use imaging::guided::{dilated_nearer, upsampled};
use imaging::linear::{decoded_linear, encoded};
use imaging::plane::Plane;
use imaging::rows::for_each_row;
use imaging::tensor::{Sampler, flattened};

use super::lens::{Blur, Lens};
use crate::error::InferenceError;
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
            // decoded to linear light and back.
            report(1.0);
            return Ok(unchanged(input, depth));
        };

        checkpoint(&report, cancelled, REFINE_SHARE)?;

        let (linear, _) = decoded_linear(input);

        // The render asks whether to stop before each layer, which is the one place it can be heard from: each
        // question after the first is a layer done. About `layers + 1` of them, one being the plane of focus.
        let layers = (Defocus::DEFAULT.layers + 1) as f64;
        let asked = AtomicUsize::new(0);
        let per_layer = || {
            let done = asked.fetch_add(1, Ordering::Relaxed) as f64;
            report(REFINE_SHARE + (1.0 - REFINE_SHARE - ENCODE_SHARE) * (done / layers).min(1.0));
            cancelled()
        };

        let blurred =
            defocus(&linear, width, height, &radius, self.keep_sharp.as_deref(), None, Defocus::DEFAULT, &per_layer)
                .ok_or(InferenceError::Cancelled)?;
        drop(linear);

        checkpoint(&report, cancelled, 1.0 - ENCODE_SHARE)?;

        let result = encoded(&blurred, width, height, depth);
        report(1.0);

        tracing::debug!(operation = %self.name, width, height, "lens blur rendered");

        Ok(result)
    }
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

    use image::{GenericImageView as _, Rgb, RgbImage};

    use super::super::lens::LensOrigin;
    use super::super::tests::two_planes;
    use super::*;
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
}
