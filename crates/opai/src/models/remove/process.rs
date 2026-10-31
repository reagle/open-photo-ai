//! The removal pipeline: the mask rasterised at the photograph's size, its regions cropped with context, each inferred
//! at the graph's square, read back and pasted through the feather, at the run's depth.

// The prototype this ports is the fork's `infill` comparison (`inp.py`'s `lama`, `runB.py`), whose results it is
// checked against. What is the prototype's, exactly: the graph's tensors (RGB in [0, 1], the hole as 1, the square
// filled by reflection), the grow and the hole, the context, and the bicubic read-back. What is not: the resample onto
// the square is Lanczos3 rather than OpenCV's area filter, as every family presented on a square here is
// (`imaging::present`); the hole is reduced by any-coverage rather than nearest, so a thin string is not lost; and the
// feather is a smoothstep across the band between the grown marks and the hole rather than a Gaussian of the grown marks
// (see `region::fill_weight`).
//
// Two departures since (2026-10-31): a crop larger than the small square is filled on the large one, the same weights
// exported at 1024, so a large region is reduced half as far; and the fill is given the photograph's grain where the
// reduction and the read-back smoothed it away (`grain`). Both judged on the fork's bench (`remove/README.md` in its
// notes): ground truth inside synthetic holes in the background of AIM-500 photographs, and the three prototype cases.

use std::sync::Arc;

use image::{DynamicImage, ImageBuffer, Rgb};

use super::gain_map::{self, Filled};
use super::grain::{self, Composite};
use super::lima::{LARGE_SIDE, PLANES_IN, PLANES_OUT, SIDE};
use super::region::{Bicubic, Marks, Rect, crops, distances, fill_weight, hole_plane, in_hole, regions};

use crate::error::InferenceError;
use crate::image::GainMap;
use crate::mask::Mask;
use crate::models::ArtifactId;
use crate::pipeline::session::GraphShape;
use crate::pipeline::{Backend, ImagePipeline, Model, Shared, checkpoint, reporter};
use crate::providers::profile::EpProfile;
use crate::sessions::SessionHandle;
use imaging::ChannelDepth;
use imaging::pad::source_offset;
use imaging::present::{plan, presented};
use imaging::tensor::{Channel, Normalisation, Sampler, flattened};

/// The range the graph reads its colour planes in and writes its result in: the fork's export divides LaMa's
/// `[0, 255]` output by 255, so both ends are `[0, 1]`.
const RANGE: Normalisation = Normalisation::Unit;

/// How many progress steps one region is: presented, inferred, pasted.
const STEPS_PER_REGION: usize = 3;

/// The share of the bar the colour removal has where a gain map is rewritten after it: the rewrite is one push-pull
/// over the regions' crops at the gain map's resolution, a small fraction of a model run.
const COLOUR_SHARE: f64 = 0.95;

/// What a removal does past the model's own fill at the small square.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Finish {
    /// Whether a crop larger than the small square is filled on the large one.
    pub(crate) large: bool,
    /// Whether the fill is given the photograph's grain.
    pub(crate) grain: bool,
}

/// What every removal does: both.
pub(crate) const FINISH: Finish = Finish { large: true, grain: true };

/// What the bench (`live::bench`) runs in place of [`FINISH`], to compare one with another in a build: never set
/// outside it.
#[cfg(test)]
pub(crate) static BENCH_FINISH: std::sync::Mutex<Option<Finish>> = std::sync::Mutex::new(None);

/// The pipeline running one removal, as the family's contract seam hands it back: `artifacts` are the small graph's
/// and the large one's.
pub(crate) fn pipeline<B: Backend>(
    name: String,
    artifacts: [ArtifactId; 2],
    profile: EpProfile,
    mask: Arc<Mask>,
) -> Shared<B> {
    #[cfg(test)]
    let finish = BENCH_FINISH.lock().expect("the bench's finish is never held across a panic").unwrap_or(FINISH);
    #[cfg(not(test))]
    let finish = FINISH;

    Arc::new(Removal::new(name, artifacts, profile, mask, [SIDE, LARGE_SIDE], finish))
}

/// One removal run: the two graphs it opens, the squares they run at, and what it takes out.
struct Removal {
    /// The small graph's artifact, then the large one's: one model at two squares.
    artifacts: [ArtifactId; 2],
    /// The measured tuning both sessions are opened under: the same weights, so the same answer.
    profile: EpProfile,
    /// The operation's display name, for naming it in a failure.
    name: String,
    /// The square each graph was exported at, the small one's first.
    sides: [u32; 2],
    /// What to take out.
    mask: Arc<Mask>,
    /// What is done past the model's fill.
    finish: Finish,
}

impl Removal {
    fn new(
        name: String,
        artifacts: [ArtifactId; 2],
        profile: EpProfile,
        mask: Arc<Mask>,
        sides: [u32; 2],
        finish: Finish,
    ) -> Self {
        Self { artifacts, profile, name, sides, mask, finish }
    }

    /// Which graph fills `crop`, by its index in [`artifacts`](Self::artifacts): the small one where the crop fits its
    /// square, so a small region is never enlarged further than it was, and the large one otherwise.
    fn graph_for(&self, crop: Rect) -> usize {
        usize::from(self.finish.large && crop.width.max(crop.height) > self.sides[0])
    }

    /// The tensor the graph of `side` is shown for `crop` of `input`, whose distances from the marks are `squared`, and
    /// the size the crop was resampled to inside the square.
    fn presented(&self, input: &DynamicImage, crop: Rect, squared: &[u16], side: u32) -> (Vec<f32>, (u32, u32)) {
        let planned = plan(crop.width, crop.height, side);
        let side = side as usize;
        let area = side * side;
        let scaled = (planned.scaled_width, planned.scaled_height);

        // The colour planes through the presentation every square-run family shares: Lanczos3 onto the planned size,
        // the rest of the square by reflection.
        let mut tensor = vec![0.0_f32; PLANES_IN * area];
        let cropped = input.crop_imm(crop.x, crop.y, crop.width, crop.height);
        presented(&cropped, planned, RANGE, &mut tensor[..3 * area])
            .expect("the scratch's colour planes are allocated at the square the graph accepts");

        // The hole at the same planned size, extended by the same reflection, so the model's hole lies over the very
        // pixels it describes. 1 is hole, LaMa's own polarity.
        let hole = hole_plane(squared, crop.width, crop.height, scaled.0, scaled.1);
        for (y, row) in tensor[3 * area..].chunks_mut(side).enumerate() {
            let from = source_offset(y as u32, scaled.1) as usize * scaled.0 as usize;

            for (x, value) in row.iter_mut().enumerate() {
                *value = hole[from + source_offset(x as u32, scaled.0) as usize];
            }
        }

        (tensor, scaled)
    }

    /// The regions this removal fills in a `width` x `height` picture: each crop with its pixels' distances from the
    /// marks, which the colour paste and the gain map's rewrite both read. Refused, before anything is allocated, for a
    /// picture with no area, as every whole-image pipeline refuses one.
    fn regions(&self, width: u32, height: u32) -> Result<Vec<Filled>, InferenceError> {
        if width == 0 || height == 0 {
            return Err(InferenceError::Untileable { width, height });
        }

        let marks = Marks::of(&self.mask.rasterised(width, height));
        Ok(crops(&regions(&marks), width, height)
            .into_iter()
            .map(|crop| Filled { crop, squared: distances(&marks, crop) })
            .collect())
    }

    /// The photograph at `depth` with `filling` removed.
    fn removed<B: Backend>(
        &self,
        input: &DynamicImage,
        filling: &[Filled],
        sessions: &[SessionHandle<B::Session>],
        depth: ChannelDepth,
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<DynamicImage, InferenceError> {
        match depth {
            ChannelDepth::Eight => {
                self.removed_at::<u8, B>(input, filling, sessions, progress, cancelled).map(u8::into_dynamic)
            }
            ChannelDepth::Sixteen => {
                self.removed_at::<u16, B>(input, filling, sessions, progress, cancelled).map(u16::into_dynamic)
            }
        }
    }

    /// [`removed`](Self::removed)'s body, once, at whichever channel the caller asked for.
    fn removed_at<T: Channel, B: Backend>(
        &self,
        input: &DynamicImage,
        filling: &[Filled],
        sessions: &[SessionHandle<B::Session>],
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<ImageBuffer<Rgb<T>, Vec<T>>, InferenceError>
    where
        Rgb<T>: image::Pixel<Subpixel = T>,
    {
        let (width, height) = (input.width(), input.height());

        if cancelled() {
            return Err(InferenceError::Cancelled);
        }

        let report = reporter(progress);

        // The photograph is the result everywhere the hole does not reach, so it is the buffer the fills are pasted
        // into: borrowed through where it already is this depth's RGB, and flattened once otherwise.
        let sampler = Sampler::new(input);
        let mut output = T::matching(&sampler).cloned().unwrap_or_else(|| flattened::<T>(&sampler, width, height));

        // Nothing marked: the photograph, and the graph is never run.
        if filling.is_empty() {
            report(1.0);
            return Ok(output);
        }

        let steps = (filling.len() * STEPS_PER_REGION) as f64;
        let at = |region: usize, step: usize| (region * STEPS_PER_REGION + step) as f64 / steps;

        for (index, Filled { crop, squared }) in filling.iter().enumerate() {
            let (crop, squared) = (*crop, squared.as_slice());
            let graph = self.graph_for(crop);
            let side = self.sides[graph];
            let (tensor, scaled) = self.presented(input, crop, squared, side);
            checkpoint(&report, cancelled, at(index, 1))?;

            let square = side as usize;
            let mut filled = vec![0.0_f32; PLANES_OUT * square * square];
            B::run_graph(
                &sessions[graph],
                &tensor,
                GraphShape::new(PLANES_IN, square, square),
                &mut filled,
                GraphShape::new(PLANES_OUT, square, square),
            )
            .map_err(InferenceError::run(&self.name, index))?;
            checkpoint(&report, cancelled, at(index, 2))?;

            let read = Bicubic::new(&filled, side, scaled, (crop.width, crop.height));
            let mut pixels = composite(&output, crop, squared, &read);
            let mut composite = Composite { pixels: &mut pixels, width: crop.width as usize, height: crop.height as usize };
            if self.finish.grain
                && let Some((photograph, fill)) = grain::grains(&composite, squared)
            {
                grain::matched(&mut composite, squared, (crop.x, crop.y), photograph, fill);
            }

            paste(&mut output, crop, squared, &pixels);
            checkpoint(&report, cancelled, at(index, 3))?;
        }

        Ok(output)
    }
}

impl<B: Backend> Model<B> for Removal {
    fn required(&self) -> &[ArtifactId] {
        &self.artifacts
    }

    /// Both graphs, under the one profile: which a region runs on is not known until its crop is.
    fn sessions(&self) -> Vec<(&ArtifactId, &EpProfile)> {
        self.artifacts.iter().map(|artifact| (artifact, &self.profile)).collect()
    }
}

impl<B: Backend> ImagePipeline<B> for Removal {
    /// One graph run per region, on one square or the other.
    fn stages(&self) -> usize {
        1
    }

    fn run(
        &self,
        input: &DynamicImage,
        sessions: &[SessionHandle<B::Session>],
        depth: ChannelDepth,
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<DynamicImage, InferenceError> {
        let filling = self.regions(input.width(), input.height())?;
        self.removed::<B>(input, &filling, sessions, depth, progress, cancelled)
    }

    /// Always: whatever the removal takes out, the gain under it is the object's. (A mask that marks nothing past
    /// [`MARKED`](super::region::MARKED) hands the gain map back unchanged, under a rewrite of its own, which the store
    /// keeps; the chain drops a removal of nothing before it plans, so that is a mask of faint greys only.)
    fn rewrites_gain_map(&self, _gain_map: &GainMap) -> bool {
        true
    }

    /// The photograph with the regions removed, and its gain map with the gain under them filled from the gain around
    /// them by a push-pull membrane through the same feather (the series design's D10): not LaMa, whose texture a gain
    /// map has no use for.
    fn run_with_gain_map(
        &self,
        input: &DynamicImage,
        gain_map: &GainMap,
        sessions: &[SessionHandle<B::Session>],
        depth: ChannelDepth,
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(DynamicImage, Option<Result<GainMap, InferenceError>>), InferenceError> {
        let report = reporter(progress);
        let filling = self.regions(input.width(), input.height())?;

        let colour = |fraction: f64| report(fraction * COLOUR_SHARE);
        let removed = self.removed::<B>(input, &filling, sessions, depth, Some(&colour), cancelled)?;

        let rewritten = gain_map::rewritten(gain_map, (input.width(), input.height()), &filling)
            .map_err(|reason| InferenceError::GainMap { operation: self.name.clone(), reason });
        checkpoint(&report, cancelled, 1.0)?;

        Ok((removed, Some(rewritten)))
    }
}

/// `crop` of `output`, the photograph, in the unit range, with the fill `read` gives in place over the hole its
/// `squared` distances describe: what the grain is measured in and matched over before the paste.
fn composite<T: Channel>(output: &ImageBuffer<Rgb<T>, Vec<T>>, crop: Rect, squared: &[u16], read: &Bicubic<'_>) -> Vec<[f32; 3]>
where
    Rgb<T>: image::Pixel<Subpixel = T>,
{
    // Reading the fill only where it is used: a crop four thousand pixels across is read back at the pixels of its
    // hole rather than resized whole. On one thread, since a channel type generic over both depths is not known to
    // cross one; a hole of a fifth of a 24-megapixel photograph is under a fifth of a second of this.
    let (width, crop_width) = (output.width() as usize, crop.width as usize);
    let buffer: &[T] = output.as_raw();
    let mut pixels = Vec::with_capacity(crop_width * crop.height as usize);

    for local in 0..crop.height as usize {
        let y = crop.y as usize + local;
        let distances = &squared[local * crop_width..][..crop_width];
        let row = &buffer[(y * width + crop.x as usize) * 3..][..crop_width * 3];

        for (offset, (squared, pixel)) in distances.iter().zip(row.chunks_exact(3)).enumerate() {
            pixels.push(if in_hole(*squared) {
                read.rgb(offset as u32, local as u32).map(|fill| RANGE.decode(fill))
            } else {
                [pixel[0].to_unit(), pixel[1].to_unit(), pixel[2].to_unit()]
            });
        }
    }

    pixels
}

/// Pastes `filled`, the composite of `crop`, into `output`, through the feather its `squared` distances give: the fill
/// where the marks were grown, mixed across the band to the hole's edge, and nothing beyond it.
fn paste<T: Channel>(output: &mut ImageBuffer<Rgb<T>, Vec<T>>, crop: Rect, squared: &[u16], filled: &[[f32; 3]])
where
    Rgb<T>: image::Pixel<Subpixel = T>,
{
    let width = output.width() as usize;
    let buffer: &mut [T] = output.as_mut();

    for local in 0..crop.height as usize {
        let y = crop.y as usize + local;
        let distances = &squared[local * crop.width as usize..][..crop.width as usize];
        let fills = &filled[local * crop.width as usize..][..crop.width as usize];
        let row = &mut buffer[y * width * 3..(y + 1) * width * 3];

        for (offset, (squared, fill)) in distances.iter().zip(fills).enumerate() {
            if !in_hole(*squared) {
                continue;
            }

            let weight = fill_weight(*squared);
            if weight == 0.0 {
                continue;
            }

            let column = crop.x as usize + offset;
            let pixel = &mut row[column * 3..column * 3 + 3];

            for (channel, fill) in pixel.iter_mut().zip(fill) {
                let fill = fill.clamp(0.0, 1.0);

                // Exactly the fill where the weight is whole, rather than the photograph plus a difference that rounds.
                *channel = if weight >= 1.0 {
                    T::from_unit(fill)
                } else {
                    let kept = channel.to_unit();
                    T::from_unit(weight.mul_add(fill - kept, kept))
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use image::{Rgb, Rgba};
    use imaging::ChannelDepth;

    use super::*;
    use crate::error::SessionError;
    use crate::mask::{Base, Plane};
    use crate::models::precision::FloatPrecision;
    use crate::models::remove::region::{GROW, HOLE};
    use crate::models::remove::{Remove, RemoveVariant};
    use crate::pipeline::ImagePipeline;
    use crate::pipeline::test_support::stub_backend_runs;
    use crate::providers::ExecutionProvider;

    /// A square small enough for a test to run a whole removal over, in place of the 512 Lima ships at: the crop of a
    /// mark a few pixels across fits it (its context alone is 64 px a side), and the crop of a larger one does not.
    const SMALL: u32 = 192;

    /// The large square at the same scale, in place of the 1024.
    const LARGE: u32 = 384;

    /// The colour the fake graph fills with, in its own `[0, 1]` range.
    const FILL: [f32; 3] = [0.0, 1.0, 0.0];

    /// What the fake graph was shown, one tensor per run.
    #[derive(Default)]
    struct Log {
        tensors: Vec<(Vec<f32>, GraphShape, GraphShape)>,
    }

    /// A session standing in for Lima: it records what it was shown and fills the whole square with [`FILL`].
    struct FakeSession {
        log: Arc<Mutex<Log>>,
    }

    /// A backend with **no ONNX Runtime**.
    struct Fake;

    impl Backend for Fake {
        type Session = FakeSession;
        type Error = std::io::Error;

        async fn acquire(
            &self,
            _artifact: &ArtifactId,
            _profile: &EpProfile,
            _requested: ExecutionProvider,
            _interest: &crate::sessions::Interest,
        ) -> Result<SessionHandle<Self::Session>, SessionError> {
            unreachable!("the pipeline is handed its sessions; it never acquires one")
        }

        stub_backend_runs!("a removal runs its graph through the declared-shape seam"; run_tile);

        fn run_graph(
            handle: &SessionHandle<Self::Session>,
            input: &[f32],
            input_shape: GraphShape,
            output: &mut [f32],
            output_shape: GraphShape,
        ) -> Result<(), Self::Error> {
            assert_eq!(input.len(), input_shape.len(), "the graph was fed a buffer {input_shape} does not describe");
            assert_eq!(output.len(), output_shape.len(), "the output buffer is not {output_shape}");

            let session = handle.session();
            session.log.lock().unwrap().tensors.push((input.to_vec(), input_shape, output_shape));

            let area = output_shape.height * output_shape.width;
            for (plane, value) in FILL.iter().enumerate() {
                output[plane * area..(plane + 1) * area].fill(*value);
            }

            Ok(())
        }

        stub_backend_runs!("a removal takes one output and no weight"; run_named_outputs, run_weighted);
    }

    /// Handles on the two fake sessions, the small graph's and the large one's, and the log both record into.
    fn session() -> ([SessionHandle<FakeSession>; 2], Arc<Mutex<Log>>) {
        let log: Arc<Mutex<Log>> = Arc::default();
        let handle = || SessionHandle::held(FakeSession { log: Arc::clone(&log) }, ExecutionProvider::Cpu);

        ([handle(), handle()], log)
    }

    /// A mask whose base is `width` x `height`, white over `marked` and black elsewhere.
    fn mask(width: u32, height: u32, marked: &[Rect]) -> Arc<Mask> {
        let mut plane = Plane::new(width, height, 0.0);
        for rect in marked {
            for y in rect.y..rect.bottom() {
                for x in rect.x..rect.right() {
                    plane.as_mut_slice()[(y * width + x) as usize] = 1.0;
                }
            }
        }

        Arc::new(Mask { base: Some(Base::new(plane)), ..Mask::default() })
    }

    /// The removal pipeline at [`SMALL`], over `mask`, named as Lima's own seam would name it.
    fn removing(mask: Arc<Mask>) -> Shared<Fake> {
        let removal = Remove::new(RemoveVariant::Lima(FloatPrecision::Fp32), Arc::clone(&mask));
        let artifacts = [removal.artifact(), removal.large_artifact()];

        Arc::new(Removal::new(removal.display_name(), artifacts, removal.profile(), mask, [SMALL, LARGE], FINISH))
    }

    /// A photograph of a gradient, so no two neighbouring pixels are alike.
    fn photograph(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([(x * 255 / width) as u8, (y * 255 / height) as u8, ((x + y) % 251) as u8])
        }))
    }

    /// The nearest a pixel is to `marked`, squared.
    fn squared_distance(x: u32, y: u32, marked: Rect) -> u32 {
        let dx = if x < marked.x { marked.x - x } else { x.saturating_sub(marked.right() - 1) };
        let dy = if y < marked.y { marked.y - y } else { y.saturating_sub(marked.bottom() - 1) };
        dx * dx + dy * dy
    }

    const fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
        Rect { x, y, width, height }
    }

    #[test]
    fn nothing_marked_is_the_photograph_and_runs_no_graph() {
        let (handle, log) = session();
        let source = photograph(90, 60);

        let produced = removing(mask(90, 60, &[]))
            .run(&source, &handle, ChannelDepth::Eight, None, &|| false)
            .unwrap();

        assert_eq!(produced.into_rgb8(), source.into_rgb8());
        assert!(log.lock().unwrap().tensors.is_empty(), "a mask that marks nothing ran the graph");
    }

    #[test]
    fn the_fill_replaces_the_grown_marks_and_nothing_past_the_hole_changes() {
        let (handle, log) = session();
        let source = photograph(200, 150);
        let marked = rect(90, 60, 12, 20);

        let produced = removing(mask(200, 150, &[marked]))
            .run(&source, &handle, ChannelDepth::Eight, None, &|| false)
            .unwrap()
            .into_rgb8();
        let source = source.into_rgb8();

        assert_eq!(log.lock().unwrap().tensors.len(), 1);
        for (x, y, pixel) in produced.enumerate_pixels() {
            let squared = squared_distance(x, y, marked);

            if squared <= GROW * GROW {
                assert_eq!(pixel.0, [0, 255, 0], "({x}, {y}) inside the grown marks is not the fill");
            } else if squared >= HOLE * HOLE {
                assert_eq!(pixel, source.get_pixel(x, y), "({x}, {y}) past the hole changed");
            }
        }

        // And the band between is a mix, not a copy of either.
        let (x, y) = (marked.right() - 1 + 12, marked.y + 5);
        assert!(produced.get_pixel(x, y).0[1] > source.get_pixel(x, y).0[1], "the feather left no trace");
        assert!(produced.get_pixel(x, y).0[1] < 255, "the feather was a hard edge");
    }

    #[test]
    fn the_graph_is_shown_the_crop_and_its_hole_at_its_square() {
        let (handle, log) = session();
        let marked = rect(90, 60, 12, 20);

        removing(mask(200, 150, &[marked]))
            .run(&photograph(200, 150), &handle, ChannelDepth::Eight, None, &|| false)
            .unwrap();

        // The crop is the whole 200 x 150 frame, larger than the small square: the large graph fills it.
        let log = log.lock().unwrap();
        let (tensor, input, output) = &log.tensors[0];
        let side = LARGE as usize;
        assert_eq!(*input, GraphShape::new(4, side, side));
        assert_eq!(*output, GraphShape::new(3, side, side));

        // The hole plane is ones and zeros, with ones in the middle of the crop (where the marks are) and none at its
        // corner, and the colour planes are in [0, 1].
        let hole = &tensor[3 * side * side..];
        assert!(hole.iter().all(|value| *value == 0.0 || *value == 1.0));
        assert!(hole.contains(&1.0) && hole[0] == 0.0, "{hole:?}");
        assert!(tensor[..3 * side * side].iter().all(|value| (0.0..=1.0).contains(value)));
    }

    #[test]
    fn a_crop_that_fits_the_small_square_is_filled_on_it_and_a_larger_one_on_the_large_square() {
        let (handle, log) = session();
        // A speck, whose crop is its context, about 150 px across; and a long stroke, whose crop is about 400.
        let marked = [rect(100, 100, 4, 4), rect(500, 300, 200, 20)];

        removing(mask(1000, 700, &marked)).run(&photograph(1000, 700), &handle, ChannelDepth::Eight, None, &|| false).unwrap();

        let sides: Vec<usize> = log.lock().unwrap().tensors.iter().map(|(_, input, _)| input.width).collect();
        assert_eq!(sides, vec![SMALL as usize, LARGE as usize], "the squares chosen for a small and a large crop");
    }

    #[test]
    fn grain_is_matched_in_a_fill_smoother_than_the_photograph_and_nowhere_else() {
        // A grainy grey photograph, a hole in it, and a fill as flat as the fake's.
        let (width, height) = (300, 240);
        let grain = |x: u32, y: u32| {
            let hashed = (x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(2_246_822_519)).rotate_left(13);
            (hashed % 21) as u8
        };
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| Rgb([118 + grain(x, y); 3])));
        let marked = rect(130, 100, 40, 40);
        let (handle, _) = session();

        let produced = removing(mask(width, height, &[marked]))
            .run(&source, &handle, ChannelDepth::Eight, None, &|| false)
            .unwrap()
            .into_rgb8();

        // Inside the marks the fill is green, with grain: its green channel is not all one level, though it is close.
        let greens: Vec<u8> = (marked.y..marked.bottom())
            .flat_map(|y| (marked.x..marked.right()).map(move |x| (x, y)))
            .map(|(x, y)| produced.get_pixel(x, y).0[1])
            .collect();
        let (low, high) = (greens.iter().min().unwrap(), greens.iter().max().unwrap());
        assert!(*low < 250 && *high == 255, "the fill was given no grain: {low} to {high}");

        // Past the hole the photograph is as it was.
        let source = source.into_rgb8();
        assert_eq!(produced.get_pixel(5, 5), source.get_pixel(5, 5));
        assert_eq!(produced.get_pixel(130 + 20, 100 - 17), source.get_pixel(130 + 20, 100 - 17));
    }

    #[test]
    fn two_regions_far_apart_are_two_inferences_and_each_is_filled() {
        let (handle, log) = session();
        let marked = [rect(40, 40, 8, 8), rect(900, 600, 8, 8)];

        let produced = removing(mask(1000, 700, &marked))
            .run(&photograph(1000, 700), &handle, ChannelDepth::Eight, None, &|| false)
            .unwrap()
            .into_rgb8();

        assert_eq!(log.lock().unwrap().tensors.len(), 2, "two regions far apart were not inferred apart");
        assert_eq!(produced.get_pixel(44, 44).0, [0, 255, 0]);
        assert_eq!(produced.get_pixel(904, 604).0, [0, 255, 0]);
    }

    #[test]
    fn a_sixteen_bit_photograph_keeps_its_sixteen_bits_where_nothing_is_removed() {
        let (handle, _) = session();
        let source = DynamicImage::ImageRgb16(ImageBuffer::from_fn(120, 80, |x, y| {
            Rgb([(x * 500 + 3) as u16, (y * 700 + 1) as u16, (x * y + 7) as u16])
        }));

        let produced = removing(mask(120, 80, &[rect(55, 35, 6, 6)]))
            .run(&source, &handle, ChannelDepth::Sixteen, None, &|| false)
            .unwrap();

        let (produced, source) = (produced.into_rgb16(), source.into_rgb16());
        assert_eq!(produced.get_pixel(1, 1), source.get_pixel(1, 1), "a far pixel lost its low byte");
        assert_eq!(produced.get_pixel(57, 37).0, [0, 65535, 0]);
    }

    #[test]
    fn an_alpha_photograph_is_flattened_as_every_pipeline_flattens_one() {
        let (handle, _) = session();
        let source = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(60, 40, Rgba([200, 100, 50, 255])));

        let produced = removing(mask(60, 40, &[rect(28, 18, 4, 4)]))
            .run(&source, &handle, ChannelDepth::Eight, None, &|| false)
            .unwrap()
            .into_rgb8();

        assert_eq!(produced.get_pixel(0, 0).0, [200, 100, 50]);
        assert_eq!(produced.get_pixel(30, 20).0, [0, 255, 0]);
    }

    #[test]
    fn progress_rises_to_the_end_and_a_stop_at_any_step_leaves_no_image() {
        let marked = [rect(40, 40, 8, 8), rect(900, 600, 8, 8)];
        let (handle, _) = session();
        let reported: Mutex<Vec<f64>> = Mutex::default();

        removing(mask(1000, 700, &marked))
            .run(
                &photograph(1000, 700),
                &handle,
                ChannelDepth::Eight,
                Some(&|fraction| reported.lock().unwrap().push(fraction)),
                &|| false,
            )
            .unwrap();

        let reported = reported.lock().unwrap();
        assert_eq!(reported.len(), 2 * STEPS_PER_REGION, "{reported:?}");
        assert!(reported.windows(2).all(|pair| pair[1] > pair[0]), "{reported:?}");
        assert_eq!(*reported.last().unwrap(), 1.0);

        for boundary in 0..=2 * STEPS_PER_REGION {
            let (handle, _) = session();
            let checked = std::sync::atomic::AtomicUsize::new(0);

            let outcome = removing(mask(1000, 700, &marked)).run(
                &photograph(1000, 700),
                &handle,
                ChannelDepth::Eight,
                None,
                &|| checked.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= boundary,
            );

            assert!(matches!(outcome, Err(InferenceError::Cancelled)), "a stop at {boundary} gave {outcome:?}");
        }
    }

    #[test]
    fn with_a_gain_map_the_photograph_is_removed_as_without_and_the_map_rewritten_after_it() {
        let marked = rect(90, 60, 12, 20);
        let source = photograph(200, 150);
        let gain_map = GainMap::holding(
            DynamicImage::ImageLuma8(image::GrayImage::from_pixel(50, 38, image::Luma([90]))),
            (200, 150),
        );
        let (handle, _) = session();
        let reported: Mutex<Vec<f64>> = Mutex::default();

        let (with, rewritten) = removing(mask(200, 150, &[marked]))
            .run_with_gain_map(
                &source,
                &gain_map,
                &handle,
                ChannelDepth::Eight,
                Some(&|fraction| reported.lock().unwrap().push(fraction)),
                &|| false,
            )
            .unwrap();
        let without = removing(mask(200, 150, &[marked]))
            .run(&source, &handle, ChannelDepth::Eight, None, &|| false)
            .unwrap();

        assert_eq!(with.as_bytes(), without.as_bytes());
        let rewritten = rewritten.expect("a removal always answers for the gain map").expect("rewritten");
        assert_eq!(rewritten.pixels().as_bytes(), gain_map.pixels().as_bytes(), "a flat gain fills as itself");
        assert!(ImagePipeline::<Fake>::rewrites_gain_map(removing(mask(200, 150, &[])).as_ref(), &gain_map));

        let reported = reported.lock().unwrap();
        assert!(reported.windows(2).all(|pair| pair[1] > pair[0]), "{reported:?}");
        assert_eq!(*reported.last().unwrap(), 1.0);
    }

    #[test]
    fn a_gain_map_that_cannot_be_rewritten_is_the_gain_maps_failure_not_the_removals() {
        let (handle, _) = session();
        let larger = GainMap::holding(DynamicImage::ImageLuma8(image::GrayImage::new(90, 60)), (90, 60));

        let (removed, rewritten) = removing(mask(60, 40, &[rect(28, 18, 4, 4)]))
            .run_with_gain_map(
                &photograph(60, 40),
                &larger,
                &handle,
                ChannelDepth::Eight,
                None,
                &|| false,
            )
            .unwrap();

        assert_eq!(removed.into_rgb8().get_pixel(30, 20).0, [0, 255, 0]);
        let error = rewritten.expect("answered").expect_err("a map wider than the photograph");
        assert!(
            matches!(&error, InferenceError::GainMap { operation, .. } if operation == "Lima (FP32)"),
            "{error:?}"
        );
    }

    #[test]
    fn an_image_with_no_area_is_refused() {
        let (handle, _) = session();
        let outcome = removing(mask(4, 4, &[rect(1, 1, 1, 1)])).run(
            &DynamicImage::ImageRgb8(ImageBuffer::new(0, 5)),
            &handle,
            ChannelDepth::Eight,
            None,
            &|| false,
        );

        assert!(matches!(outcome, Err(InferenceError::Untileable { width: 0, height: 5 })), "{outcome:?}");
        assert_eq!(ImagePipeline::<Fake>::stages(removing(mask(4, 4, &[])).as_ref()), 1);
    }
}
