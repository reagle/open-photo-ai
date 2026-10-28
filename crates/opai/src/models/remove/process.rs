//! The removal pipeline: the mask rasterised at the photograph's size, its regions cropped with context, each inferred
//! at the graph's square, read back and pasted through the feather, at the run's depth.

// The prototype this ports is the fork's `infill` comparison (`inp.py`'s `lama`, `runB.py`), whose results it is
// checked against. What is the prototype's, exactly: the graph's tensors (RGB in [0, 1], the hole as 1, the square
// filled by reflection), the grow and the hole, the context, and the bicubic read-back. What is not: the resample onto
// the square is Lanczos3 rather than OpenCV's area filter, as every family presented on a square here is
// (`imaging::present`); the hole is reduced by any-coverage rather than nearest, so a thin string is not lost; and the
// feather is a smoothstep across the band between the grown marks and the hole rather than a Gaussian of the grown marks
// (see `region::fill_weight`).

use std::sync::Arc;

use image::{DynamicImage, ImageBuffer, Rgb};

use super::gain_map::{self, Filled};
use super::lima::{PLANES_IN, PLANES_OUT};
use super::region::{Bicubic, Marks, Rect, crops, distances, fill_weight, hole_plane, in_hole, regions};

use crate::error::InferenceError;
use crate::image::GainMap;
use crate::mask::Mask;
use crate::models::ArtifactId;
use crate::pipeline::session::GraphShape;
use crate::pipeline::{Backend, ImagePipeline, OnOneGraph, Shared, SingleGraph, checkpoint, reporter};
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

/// The pipeline running one removal, as the family's contract seam hands it back.
pub(crate) fn pipeline<B: Backend>(
    name: String,
    artifact: ArtifactId,
    profile: EpProfile,
    mask: Arc<Mask>,
) -> Shared<B> {
    Arc::new(Removal::new(name, artifact, profile, mask, super::lima::SIDE))
}

/// One removal run: the graph it opens, the square it runs at, and what it takes out.
struct Removal {
    /// The graph this operation runs, and the name it reports a failure in.
    graph: SingleGraph,
    /// The square the graph was exported at.
    side: u32,
    /// What to take out.
    mask: Arc<Mask>,
}

impl Removal {
    fn new(name: String, artifact: ArtifactId, profile: EpProfile, mask: Arc<Mask>, side: u32) -> Self {
        Self { graph: SingleGraph::new(name, artifact, profile), side, mask }
    }

    /// The tensor the graph is shown for `crop` of `input`, whose distances from the marks are `squared`, and the size
    /// the crop was resampled to inside the square.
    fn presented(&self, input: &DynamicImage, crop: Rect, squared: &[u16]) -> (Vec<f32>, (u32, u32)) {
        let side = self.side as usize;
        let area = side * side;
        let planned = plan(crop.width, crop.height, self.side);
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

        let session = &sessions[0];
        let side = self.side as usize;
        let steps = (filling.len() * STEPS_PER_REGION) as f64;
        let at = |region: usize, step: usize| (region * STEPS_PER_REGION + step) as f64 / steps;

        for (index, Filled { crop, squared }) in filling.iter().enumerate() {
            let (crop, squared) = (*crop, squared.as_slice());
            let (tensor, scaled) = self.presented(input, crop, squared);
            checkpoint(&report, cancelled, at(index, 1))?;

            let mut filled = vec![0.0_f32; PLANES_OUT * side * side];
            B::run_graph(
                session,
                &tensor,
                GraphShape::new(PLANES_IN, side, side),
                &mut filled,
                GraphShape::new(PLANES_OUT, side, side),
            )
            .map_err(InferenceError::run(&self.graph.name, index))?;
            checkpoint(&report, cancelled, at(index, 2))?;

            paste(&mut output, crop, squared, &Bicubic::new(&filled, self.side, scaled, (crop.width, crop.height)));
            checkpoint(&report, cancelled, at(index, 3))?;
        }

        Ok(output)
    }
}

impl OnOneGraph for Removal {
    fn graph(&self) -> &SingleGraph {
        &self.graph
    }
}

impl<B: Backend> ImagePipeline<B> for Removal {
    /// One graph, run once per region.
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
            .map_err(|reason| InferenceError::GainMap { operation: self.graph.name.clone(), reason });
        checkpoint(&report, cancelled, 1.0)?;

        Ok((removed, Some(rewritten)))
    }
}

/// Pastes the fill `read` gives for `crop` into `output`, through the feather its `squared` distances give: the fill
/// where the marks were grown, mixed across the band to the hole's edge, and nothing beyond it.
fn paste<T: Channel>(output: &mut ImageBuffer<Rgb<T>, Vec<T>>, crop: Rect, squared: &[u16], read: &Bicubic<'_>)
where
    Rgb<T>: image::Pixel<Subpixel = T>,
{
    // Reading the fill only where it is used: a crop four thousand pixels across is read back at the pixels of its
    // hole rather than resized whole. On one thread, since a channel type generic over both depths is not known to
    // cross one; a hole of a fifth of a 24-megapixel photograph is under a fifth of a second of this.
    let width = output.width() as usize;
    let buffer: &mut [T] = output.as_mut();

    for local in 0..crop.height as usize {
        let y = crop.y as usize + local;
        let distances = &squared[local * crop.width as usize..][..crop.width as usize];
        let row = &mut buffer[y * width * 3..(y + 1) * width * 3];

        for (offset, squared) in distances.iter().enumerate() {
            if !in_hole(*squared) {
                continue;
            }

            let weight = fill_weight(*squared);
            if weight == 0.0 {
                continue;
            }

            let fill = read.rgb(offset as u32, local as u32);
            let column = crop.x as usize + offset;
            let pixel = &mut row[column * 3..column * 3 + 3];

            for (channel, fill) in pixel.iter_mut().zip(fill) {
                let fill = RANGE.decode(fill);

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

    /// A square small enough for a test to run a whole removal over, in place of the 512 Lima ships at.
    const SMALL: u32 = 32;

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

    /// A handle on a fake session, and the log it records into.
    fn session() -> (SessionHandle<FakeSession>, Arc<Mutex<Log>>) {
        let log: Arc<Mutex<Log>> = Arc::default();
        (SessionHandle::held(FakeSession { log: Arc::clone(&log) }, ExecutionProvider::Cpu), log)
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
        Arc::new(Removal::new(removal.display_name(), removal.artifact(), removal.profile(), mask, SMALL))
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
            .run(&source, std::slice::from_ref(&handle), ChannelDepth::Eight, None, &|| false)
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
            .run(&source, std::slice::from_ref(&handle), ChannelDepth::Eight, None, &|| false)
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
            .run(&photograph(200, 150), std::slice::from_ref(&handle), ChannelDepth::Eight, None, &|| false)
            .unwrap();

        let log = log.lock().unwrap();
        let (tensor, input, output) = &log.tensors[0];
        let side = SMALL as usize;
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
    fn two_regions_far_apart_are_two_inferences_and_each_is_filled() {
        let (handle, log) = session();
        let marked = [rect(40, 40, 8, 8), rect(900, 600, 8, 8)];

        let produced = removing(mask(1000, 700, &marked))
            .run(&photograph(1000, 700), std::slice::from_ref(&handle), ChannelDepth::Eight, None, &|| false)
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
            .run(&source, std::slice::from_ref(&handle), ChannelDepth::Sixteen, None, &|| false)
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
            .run(&source, std::slice::from_ref(&handle), ChannelDepth::Eight, None, &|| false)
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
                std::slice::from_ref(&handle),
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
                std::slice::from_ref(&handle),
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
                std::slice::from_ref(&handle),
                ChannelDepth::Eight,
                Some(&|fraction| reported.lock().unwrap().push(fraction)),
                &|| false,
            )
            .unwrap();
        let without = removing(mask(200, 150, &[marked]))
            .run(&source, std::slice::from_ref(&handle), ChannelDepth::Eight, None, &|| false)
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
                std::slice::from_ref(&handle),
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
            std::slice::from_ref(&handle),
            ChannelDepth::Eight,
            None,
            &|| false,
        );

        assert!(matches!(outcome, Err(InferenceError::Untileable { width: 0, height: 5 })), "{outcome:?}");
        assert_eq!(ImagePipeline::<Fake>::stages(removing(mask(4, 4, &[])).as_ref()), 1);
    }
}
