//! The lens blur's pipeline: no graph, a depth map and a lens in, the photograph with the depth of field a wider
//! aperture would have given it out.

// An `ImagePipeline` with no session of its own. Its required artifacts are the depth model's, so installing what a
// chain needs and the "needs a model" checks work as for any family, while `sessions` names none and the chain's
// acquisition opens nothing for it: the depth was estimated, through its own cached run, before the chain began.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use image::DynamicImage;
use imaging::ChannelDepth;
use imaging::defocus::{Defocus, Spacing, Subject, defocus};
use imaging::fill::filled;
use imaging::fusion::foreground;
use imaging::guided::{dilated_nearer, upsampled};
use imaging::linear::{decoded_linear, encoded};
use imaging::morph::dilated;
use imaging::plane::Plane;
use imaging::rows::for_each_row;
use imaging::tensor::{Sampler, flattened};

use super::lens::{Blur, Lens};
use super::trimap::{Band, matted_focus, unknown_radius};
use crate::error::InferenceError;
use crate::models::{ArtifactId, DepthMap, Matte};
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

/// The layers a matted render is spread over: from a pixel either side of focus, each 1.4 times the last.
///
/// Evenly spaced layers at a long lens's radii (-400 to +150 px at 181 mm f/1.4) are 40 px apart, so nothing near focus
/// is rendered at its own radius, and the matted subject's filled-in background shows through the gaps that leaves.
const MATTED_SPACING: Spacing = Spacing::Geometric { first: 1.0, ratio: 1.4 };

/// The coverage above which a pixel is part of the subject, and has the background filled in behind it.
const COVERED: f32 = 0.004;

// Not the radius of the subject's median distance, which the design proposed: on a long lens the band of distances is
// deep in radius (at 181 mm f/1.4 a street portrait's spans -20 to +20 px), and its median, pulled back by the hair,
// fur and leaf tips it is generous to, put the face focused on 9 px out of focus. Everything in the band is sharp: a
// focus range.
/// The one radius the matted subject is rendered at: none, so the whole subject in focus is sharp.
const SUBJECT_RADIUS: f32 = 0.0;

/// Blur fusion's two window radii at [`REFERENCE_DIAGONAL`]: a wide pass for the colours either side of the edge, then
/// a narrow one along the strands.
const FUSION_RADII: [f32; 2] = [90.0, 6.0];

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
    /// The subject in focus, matted on the chain's input, or `None` to render from the depth alone.
    matte: Option<Arc<Matte>>,
    /// The depth refined to the chain's input when its trimap was built, if it was: the render's own, where the input
    /// is still that size, rather than refined a second time.
    refined: Option<Arc<Refined>>,
}

impl LensBlurPipeline {
    pub(crate) fn new(
        name: String,
        required: Vec<ArtifactId>,
        settings: Settings,
        depth: Option<Arc<DepthMap>>,
        keep_sharp: Option<Arc<Plane>>,
        matte: Option<Arc<Matte>>,
        refined: Option<Arc<Refined>>,
    ) -> Self {
        Self { name, required, settings, depth, keep_sharp, matte, refined }
    }

    /// The blur at the `input`'s size and the depth refined to it, or `None` where the blur adds nothing anywhere.
    ///
    /// Decided before anything is built from the photograph: the guide the depth is refined along is a float copy of
    /// every pixel, which a blur at or above the photograph's own f-number would have made for nothing.
    fn prepared(&self, input: &DynamicImage) -> Option<Prepared> {
        let diagonal = (input.width() as f32).hypot(input.height() as f32);
        // None where the chain knew the blur adds nothing and estimated no depth, which `Blur::new` then agrees with.
        let depth = self.depth.as_ref()?;
        let (fx, fy) = self.settings.focus;
        // Read from the map as estimated, before the edge grow moves any edge: the focus is the depth at the point. A
        // matted subject's is the rounded distance its band was drawn around, so the render's band is the trimap's.
        let focus_m = depth.focus_metres(fx, fy);
        let focus_m = if self.matte.is_some() { matted_focus(focus_m) } else { focus_m };
        let blur = Blur::new(&self.settings.lens, self.settings.f_number, focus_m, diagonal)?;
        let refined = match &self.refined {
            Some(refined) if (refined.plane.width(), refined.plane.height()) == (input.width(), input.height()) => {
                Arc::clone(refined)
            }
            _ => Arc::new(Refined::of(depth, input)),
        };

        Some(Prepared { blur, focus_m, refined, scale: diagonal / REFERENCE_DIAGONAL })
    }

    /// The signed radius every pixel of the `input` is blurred by, or `None` where the blur adds nothing anywhere.
    #[cfg(test)]
    fn radii(&self, input: &DynamicImage) -> Option<Plane> {
        self.prepared(input).map(|prepared| prepared.radius(&prepared.refined.plane, &self.name))
    }

    /// The scene filled in behind the subject `matte` covers, and the subject, for a render at `prepared`'s size.
    ///
    /// The composite of the OpenSpec design of `add-lens-blur-8-matting` (X8): the colour behind the subject filled
    /// from the background around it; the disparity filled likewise, over the subject and over a ring around it within
    /// the band, where the depth map's soft edge put the background at the subject's distance and would leave a crisp
    /// halo; and the subject's own colour by blur fusion. The subject is rendered sharp.
    fn matted(&self, linear: Vec<f32>, prepared: &Prepared, matte: &Matte, width: u32, height: u32) -> Matted {
        let pixels = width as usize * height as usize;
        let alpha = matte.plane().resized(width, height);
        let coverage = alpha.as_slice();
        let band = Band::around(prepared.focus_m);
        let nearest = prepared.refined.nearest;
        let disparity = prepared.refined.plane.as_slice();
        let ring = unknown_radius(width, height);
        let plane_of = |at: &dyn Fn(usize) -> bool| {
            Plane::from_vec(width, height, (0..pixels).map(|index| f32::from(u8::from(at(index)))).collect())
                .expect("one value a pixel")
        };

        // Each full-size mask is dropped as soon as what it was made for is: at 24 megapixels each is 96 MB.

        // What the subject covers, and the ring around it within the band where the depth bled the subject outward.
        let covered = |index: usize| coverage[index] > COVERED;
        let around = dilated(&plane_of(&covered), ring);
        let near_band = |index: usize| around.as_slice()[index] > 0.5 && band.holds(disparity[index] * nearest);
        let hidden = plane_of(&|index| covered(index) || near_band(index));
        drop(around);

        // The background's distance there, from what lies behind the band and is not hidden; never nearer than the
        // depth already says, so a ring pixel that truly was background keeps its own.
        let behind = plane_of(&|index| hidden.as_slice()[index] < 0.5 && band.behind(disparity[index] * nearest));
        let filled_disparity = filled(disparity, 1, width, height, &behind);
        drop(behind);
        let background_disparity = Plane::from_vec(
            width,
            height,
            (0..pixels)
                .map(|index| {
                    if hidden.as_slice()[index] > 0.5 {
                        filled_disparity[index].min(disparity[index])
                    } else {
                        disparity[index]
                    }
                })
                .collect(),
        )
        .expect("one value a pixel");
        drop((filled_disparity, hidden));
        let radius = prepared.radius(&background_disparity, &self.name);
        drop(background_disparity);

        // The subject's own colour, from the photograph as it is: what of each edge pixel is not the background.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a window of a few dozen pixels"
        )]
        let radii = FUSION_RADII.map(|radius| (radius * prepared.scale).round().max(1.0) as u32);
        let colour = foreground(&linear, &alpha, width, height, &radii);

        // The background's colour behind the subject, from what lies behind the band and is not covered. Not from what is
        // in front of it: focused low on a tree, the canopy above is nearer, and its colour would wash the background.
        let known = plane_of(&|index| !covered(index) && band.behind(disparity[index] * nearest));
        let filled_colour = filled(&linear, 3, width, height, &known);
        // Except beside a nearer thing, where the subject goes on behind it: what shows through its blurred edge is
        // the subject, so the subject's own colour is kept there, as far as the farthest disc reaches, and wherever
        // no disc from there reaches the background around the subject, which it would otherwise tint.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a radius of a few hundred pixels"
        )]
        let reach = (radius.as_slice().iter().fold(0.0_f32, |far, &r| far.min(r)).abs().ceil() as u32).max(ring);
        let beside_nearer = dilated(&plane_of(&|index| band.in_front(disparity[index] * nearest)), reach);
        let beside_behind = dilated(&known, reach);
        drop(known);
        // Filled only for the subject's share of a pixel: the photograph is `a F + (1 - a) B`, so what is left of it
        // with the subject's own colour taken out is the background's own share, which is kept. Filled whole, a faint
        // alpha in a gap between leaves or branches painted the background there with a colour from far away — a pale
        // veil over a dark bush — that the blur then spread.
        let mut background = linear;
        for (index, ((pixel, fill), own)) in background
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .zip(filled_colour.as_chunks::<3>().0)
            .zip(colour.as_chunks::<3>().0)
            .enumerate()
        {
            let kept = beside_nearer.as_slice()[index] > 0.5 && beside_behind.as_slice()[index] < 0.5;
            if covered(index) && !kept {
                let share = coverage[index].min(1.0);
                for ((value, fill), own) in pixel.iter_mut().zip(fill).zip(own) {
                    *value = (*value - share * own).max(0.0) + share * fill;
                }
            }
        }
        drop((beside_nearer, beside_behind));
        drop(filled_colour);

        tracing::debug!(operation = %self.name, ring, "lens blur subject matted");

        Matted { background, radius, colour, alpha }
    }
}

/// The depth brought to a picture's size along its edges: disparity over the map's nearest, guided-upsampled.
pub(crate) struct Refined {
    /// Disparity divided by [`nearest`](Self::nearest), so in `0..=1`, at the picture's size.
    pub(crate) plane: Plane,
    /// The largest disparity in the map, in `1 / m`.
    pub(crate) nearest: f32,
}

impl Refined {
    /// `depth`, refined along `input`'s edges to its size by D9's guided filter.
    pub(crate) fn of(depth: &DepthMap, input: &DynamicImage) -> Self {
        let (width, height) = (input.width(), input.height());
        let diagonal = (width as f32).hypot(height as f32);
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

        Self { plane: upsampled(&low, &guide, width, height, radius, GUIDE_EPS), nearest }
    }
}

/// What a run settles before it renders: the blur, the distance focused at, and the depth at the input's size.
struct Prepared {
    blur: Blur,
    /// The distance focused at as the depth map reads it, which the subject's band is drawn around: the band is a
    /// ratio of distances, so the lens's depth scale would not move it.
    focus_m: f32,
    refined: Arc<Refined>,
    /// The input's diagonal over [`REFERENCE_DIAGONAL`].
    scale: f32,
}

impl Prepared {
    /// The signed radius of every pixel of `disparity`, a plane like [`Refined::plane`], after the edge grow.
    fn radius(&self, disparity: &Plane, name: &str) -> Plane {
        let mut grown = dilated_nearer(disparity, (EDGE_GROW * self.scale).max(1.0));

        for value in grown.as_mut_slice() {
            *value = self.blur.radius(*value * self.refined.nearest);
        }

        // What explains a result: where it focused, and how far the discs reach either side of the plane of focus.
        let (behind, front) =
            grown.as_slice().iter().fold((0.0_f32, 0.0_f32), |(low, high), &r| (low.min(r), high.max(r)));
        // The focus as the lens saw it: the depth map's reading times the lens's depth scale.
        tracing::debug!(
            operation = %name,
            focus_m = self.blur.focus_metres(),
            map_focus_m = self.focus_m,
            behind,
            front,
            "lens blur radii"
        );

        grown
    }
}

/// A matted render's inputs: the scene with the subject taken out and filled in, and the subject.
struct Matted {
    /// Linear colour, three values a pixel, filled in behind the subject.
    background: Vec<f32>,
    /// The background's signed radius a pixel, filled in behind the subject and around it.
    radius: Plane,
    /// The subject's own colour, linear, three values a pixel.
    colour: Vec<f32>,
    /// How much of each pixel the subject covers, at the input's size.
    alpha: Plane,
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

        let Some(prepared) = self.prepared(input) else {
            // At or above the photograph's own f-number: the photograph as it is, at the depth asked for, and never
            // decoded to linear light and back.
            report(1.0);
            return Ok(unchanged(input, depth));
        };

        let (linear, _) = decoded_linear(input);

        // With a matte, the scene behind the subject and the subject, spread over geometric layers; without one, the
        // depth's own radii over even ones, exactly as before there was a matte.
        let (scene, radius, subject, settings) = match &self.matte {
            Some(matte) => {
                let matted = self.matted(linear, &prepared, matte, width, height);
                let settings = Defocus { spacing: MATTED_SPACING, ..Defocus::DEFAULT };

                (matted.background, matted.radius, Some((matted.colour, matted.alpha)), settings)
            }
            None => (linear, prepared.radius(&prepared.refined.plane, &self.name), None, Defocus::DEFAULT),
        };
        drop(prepared);

        checkpoint(&report, cancelled, REFINE_SHARE)?;

        // The render asks whether to stop before each layer, which is the one place it can be heard from: each
        // question after the first is a layer done.
        let layers = layer_count(&radius, settings, subject.is_some()) as f64;
        let asked = AtomicUsize::new(0);
        let per_layer = || {
            let done = asked.fetch_add(1, Ordering::Relaxed) as f64;
            report(REFINE_SHARE + (1.0 - REFINE_SHARE - ENCODE_SHARE) * (done / layers).min(1.0));
            cancelled()
        };

        let subject = subject.as_ref().map(|(colour, alpha)| Subject { colour, alpha, radius: SUBJECT_RADIUS });
        let blurred =
            defocus(&scene, width, height, &radius, self.keep_sharp.as_deref(), subject, settings, &per_layer)
                .ok_or(InferenceError::Cancelled)?;
        drop(scene);

        checkpoint(&report, cancelled, 1.0 - ENCODE_SHARE)?;

        let result = encoded(&blurred, width, height, depth);
        report(1.0);

        tracing::debug!(operation = %self.name, width, height, matted = self.matte.is_some(), "lens blur rendered");

        Ok(result)
    }
}

/// About how many times a render over `radius` with `settings` asks whether to stop: once a layer, one being the plane
/// of focus, and once more for a subject.
///
/// With even spacing that is the settings' layer count and one; with geometric spacing it is counted as the render
/// spaces them, from a pixel either side of focus out to the extreme radii. Only the progress bar reads it.
fn layer_count(radius: &Plane, settings: Defocus, subject: bool) -> usize {
    let layers = match settings.spacing {
        Spacing::Even => settings.layers + 1,
        Spacing::Geometric { first, ratio } => {
            let (behind, front) =
                radius.as_slice().iter().fold((0.0_f32, 0.0_f32), |(low, high), &r| (low.min(r), high.max(r)));
            let side = |extreme: f32| {
                let (mut count, mut at) = (0, first);
                while at < extreme {
                    count += 1;
                    at = (at * ratio).max(at + 1.0);
                }

                count + usize::from(extreme >= first)
            };

            1 + side(-behind) + side(front)
        }
    };

    layers + usize::from(subject)
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
            None,
            None,
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

    /// FNV-1a over a picture's bytes: a fingerprint a test can pin a render to.
    fn fingerprint(picture: &DynamicImage) -> u64 {
        picture
            .as_bytes()
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3))
    }

    #[test]
    fn without_a_matte_the_render_is_exactly_what_it_was_before_matting() {
        // Fingerprints of the render as slice 3 made it, taken before matting existed: the depth-only path, which
        // `--lb-edges depth` and a blur too small to matte both take, must not move by a bit.
        let input = checkerboard(320, 240);
        let everything = Arc::new(Plane::new(8, 6, 0.25));

        assert_eq!(fingerprint(&run(&pipeline(1.4, None), &input)), 0x2da7_4a6e_90f7_36d5);
        assert_eq!(fingerprint(&run(&pipeline(2.0, Some(everything)), &input)), 0x510b_b171_6e9f_72fd);
    }

    /// The square the two-plane map's near plane covers on a 320 x 240 picture, inset a little, and a soft ring of
    /// strands 6 px wide around it: alpha 1 inside, 0.5 on the ring, 0 beyond.
    const INSIDE: (u32, u32, u32, u32) = (110, 84, 210, 156);
    const RING: u32 = 6;

    fn on_ring(x: u32, y: u32) -> bool {
        let (x0, y0, x1, y1) = INSIDE;
        let within = |inset: u32| x + inset >= x0 && x < x1 + inset && y + inset >= y0 && y < y1 + inset;

        within(RING) && !within(0)
    }

    /// A red subject over the checkerboard, its strands half red and half the checkerboard behind them, and its matte.
    fn subject() -> (DynamicImage, Matte) {
        let (x0, y0, x1, y1) = INSIDE;
        let board = checkerboard(320, 240).to_rgb8();
        let red = Rgb([200, 30, 30]);
        let picture = RgbImage::from_fn(320, 240, |x, y| {
            let behind = *board.get_pixel(x, y);
            if (x0..x1).contains(&x) && (y0..y1).contains(&y) {
                red
            } else if on_ring(x, y) {
                Rgb(std::array::from_fn(|channel| {
                    ((u16::from(red[channel]) + u16::from(behind[channel])) / 2) as u8
                }))
            } else {
                behind
            }
        });
        let alpha: Vec<f32> = (0..240)
            .flat_map(|y| (0..320).map(move |x| (x, y)))
            .map(|(x, y)| {
                if (x0..x1).contains(&x) && (y0..y1).contains(&y) {
                    1.0
                } else if on_ring(x, y) {
                    0.5
                } else {
                    0.0
                }
            })
            .collect();

        (DynamicImage::ImageRgb8(picture), Matte::new(320, 240, &alpha))
    }

    fn matted_pipeline(matte: Matte) -> LensBlurPipeline {
        let base = pipeline(1.4, None);

        LensBlurPipeline { matte: Some(Arc::new(matte)), ..base }
    }

    #[test]
    fn a_matted_subject_stays_sharp_and_the_background_between_its_strands_is_blurred() {
        let (input, matte) = subject();
        let matted = run(&matted_pipeline(matte), &input);
        let plain = run(&pipeline(1.4, None), &input);

        // The subject itself, as it was.
        assert!(difference(&input, &matted, (130, 100, 190, 140)) < 1.0);
        // The background, well away from it, blurred as it is without a matte.
        assert!(difference(&input, &matted, BACKGROUND) > 30.0, "{}", difference(&input, &matted, BACKGROUND));

        // Down the left side of the ring, the checkerboard seen between the strands alternates yellow and blue in the
        // photograph. Matted, what shows between them is the blurred background, so it hardly varies; and it varies
        // less than where the depth alone decides which strand pixel is sharp and which is blurred.
        let spread = |picture: &DynamicImage| {
            let picture = picture.to_rgb8();
            let blues: Vec<u8> = (90..150).map(|y| picture.get_pixel(INSIDE.0 - 2, y)[2]).collect();

            blues.iter().max().unwrap() - blues.iter().min().unwrap()
        };
        assert!(spread(&input) >= 40, "{}", spread(&input));
        assert!(3 * spread(&matted) < spread(&input), "{} against {}", spread(&matted), spread(&input));
        assert!(spread(&matted) < spread(&plain), "{} against {}", spread(&matted), spread(&plain));

        // Half the strand pixel is the subject's red.
        let strand = matted.to_rgb8();
        let strand = strand.get_pixel(INSIDE.0 - 2, 120);
        assert!(strand[0] > 90 && strand[0] < 200, "{strand:?}");
    }

    #[test]
    fn a_faint_alpha_keeps_the_background_it_mostly_is() {
        // A red subject with a wide gap in it, through which a dark bush shows, before a pale wall: the matte gives the
        // gap a faint alpha, as a matting model does between leaves. The gap stays the bush's dark, not a veil of the
        // wall's colour filled in from around the subject.
        let (x0, y0, x1, y1) = INSIDE;
        let gap = |x: u32, y: u32| (130..190).contains(&x) && (100..140).contains(&y);
        let inside = |x: u32, y: u32| (x0..x1).contains(&x) && (y0..y1).contains(&y);
        let picture = DynamicImage::ImageRgb8(RgbImage::from_fn(320, 240, |x, y| {
            if gap(x, y) {
                Rgb([20, 40, 30])
            } else if inside(x, y) {
                Rgb([200, 30, 30])
            } else {
                Rgb([210, 210, 200])
            }
        }));
        let alpha: Vec<f32> = (0..240)
            .flat_map(|y| (0..320).map(move |x| (x, y)))
            .map(|(x, y)| {
                if gap(x, y) {
                    0.05
                } else if inside(x, y) {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();

        let result = run(&matted_pipeline(Matte::new(320, 240, &alpha)), &picture).to_rgb8();
        // Filled whole, the gap's middle reads [205, 205, 195]: the wall.
        let middle = result.get_pixel(160, 120);
        assert!(middle.0.iter().all(|&value| value < 90), "{middle:?}");
    }

    #[test]
    fn beside_a_nearer_thing_the_subject_and_not_the_background_shows_through_its_blurred_edge() {
        // A red subject at 1 m, its left part hidden by a grey post at 0.6 m that runs the picture's height, before a
        // checkerboard at 10 m. Where the post's edge blurs over the subject, what shows through is the subject's red,
        // not the background's colour filled in behind the subject; away from the post, the fill is as before.
        let metres: Vec<f32> = (0..72)
            .flat_map(|y| (0..96).map(move |x| (x, y)))
            .map(|(x, y)| {
                if (24..40).contains(&x) {
                    0.6
                } else if (32..64).contains(&x) && (24..48).contains(&y) {
                    1.0
                } else {
                    10.0
                }
            })
            .collect();
        let depth = Arc::new(DepthMap::new(96, 72, &metres, Some(36.0)));
        let (post, subject) = ((80..133), (133..213, 80..160));
        let board = checkerboard(320, 240).to_rgb8();
        let picture = DynamicImage::ImageRgb8(RgbImage::from_fn(320, 240, |x, y| {
            if post.contains(&x) {
                Rgb([128, 128, 128])
            } else if subject.0.contains(&x) && subject.1.contains(&y) {
                Rgb([200, 30, 30])
            } else {
                *board.get_pixel(x, y)
            }
        }));
        let alpha: Vec<f32> = (0..240)
            .flat_map(|y| (0..320).map(move |x| (x, y)))
            .map(|(x, y)| if subject.0.contains(&x) && subject.1.contains(&y) { 1.0 } else { 0.0 })
            .collect();
        let pipeline = LensBlurPipeline {
            depth: Some(depth),
            matte: Some(Arc::new(Matte::new(320, 240, &alpha))),
            ..pipeline(1.4, None)
        };

        // Just inside the subject, beside the post: red mixed with the post's grey, and with nothing of the
        // checkerboard's yellow (green above blue). Filled from behind the subject, it reads [194, 71, 51].
        let result = run(&pipeline, &picture).to_rgb8();
        for x in [131, 135] {
            let [red, green, blue] = result.get_pixel(x, 120).0;
            assert!(red > green && green.abs_diff(blue) <= 4, "({x}, 120): {:?}", [red, green, blue]);
        }
        assert!(result.get_pixel(135, 120)[1] < 50, "{:?}", result.get_pixel(135, 120));
        assert_eq!(result.get_pixel(140, 120).0, [200, 30, 30]);
    }

    #[test]
    fn a_matted_render_reports_its_progress_forward_to_one_and_can_be_stopped() {
        let (input, matte) = subject();
        let seen = Mutex::new(Vec::new());
        let record = |fraction: f64| seen.lock().unwrap().push(fraction);
        let pipeline = matted_pipeline(matte);

        ImagePipeline::<NoBackend>::run(&pipeline, &input, &[], ChannelDepth::Eight, Some(&record), &|| false)
            .expect("runs");

        let seen = seen.into_inner().unwrap();
        assert!(seen.windows(2).all(|pair| pair[1] >= pair[0]), "{seen:?}");
        assert_eq!(seen.last(), Some(&1.0));

        let stopped = ImagePipeline::<NoBackend>::run(&pipeline, &input, &[], ChannelDepth::Eight, None, &|| true);
        assert!(matches!(stopped, Err(InferenceError::Cancelled)));
    }

    #[test]
    fn the_depth_refined_for_the_trimap_is_the_one_the_render_uses_and_only_at_its_size() {
        let (input, matte) = subject();
        let depth = two_planes(96, 72, 1.0, 10.0);
        let own = run(&matted_pipeline(matte.clone()), &input);

        // Refined on the same picture: the same render, refined once.
        let refined = Arc::new(Refined::of(&depth, &input));
        let handed = LensBlurPipeline { refined: Some(Arc::clone(&refined)), ..matted_pipeline(matte.clone()) };
        assert_eq!(run(&handed, &input).to_rgb8(), own.to_rgb8());

        // Refined at another size — an upscale between the chain's input and the blur — it is refined again.
        let elsewhere = Arc::new(Refined::of(&depth, &checkerboard(160, 120)));
        let resized = LensBlurPipeline { refined: Some(elsewhere), ..matted_pipeline(matte) };
        assert_eq!(run(&resized, &input).to_rgb8(), own.to_rgb8());
    }

    #[test]
    fn geometric_layers_are_counted_as_the_render_spaces_them() {
        // 1, 2, 3, 4.2, 5.9, 8.2 and the extreme 10 behind; 1, 2, 3 in front; and the plane of focus.
        let radius = Plane::from_vec(3, 1, vec![-10.0, 0.0, 3.0]).unwrap();
        let geometric = Defocus { spacing: MATTED_SPACING, ..Defocus::DEFAULT };

        assert_eq!(layer_count(&radius, geometric, false), 1 + 7 + 3);
        assert_eq!(layer_count(&radius, geometric, true), 1 + 7 + 3 + 1);
        assert_eq!(layer_count(&radius, Defocus::DEFAULT, false), Defocus::DEFAULT.layers + 1);
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
