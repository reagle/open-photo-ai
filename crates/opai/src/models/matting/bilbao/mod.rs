//! Bilbao: BiRefNet's matting weights, a model that mattes the salient subject of a whole photograph with no trimap,
//! read by the lens blur for where that subject has holes.
//!
//! One graph, one run per photograph, at the graph's fixed 1024 x 1024: the photograph stretched to fill it, and the
//! matte kept at that size — what the lens blur reads of it is coarse (where a hole is, not its edge, which Porto
//! draws), so a megapixel is all it needs and all the run cache stores. Progress is reported at three phase
//! boundaries — 0.1 once the tensor is built, 0.9 once the graph returns, 1.0 once the matte is made — and cancellation
//! is checked before the graph only: once it has returned, the matte is finished and handed back even to a stopped run,
//! so the run cache keeps it.

// A run:
//
//   tensor      the photograph resized to 1024 x 1024 (a triangle filter), ImageNet-normalised RGB, planar CHW
//   run_graph   1 x 3 x 1024 x 1024 -> logits, 1 x 1 x 1024 x 1024
//   sigmoid     the logits as coverages, a Matte of 1024 x 1024
//
// Which weights these are: `ZhengPeng7/BiRefNet` (code MIT, `Copyright (c) 2024 ZhengPeng`), the matting weights
// `BiRefNet-matting-epoch_100` as its v1 release exports them to ONNX (the model card `ZhengPeng7/BiRefNet-matting`
// says MIT; they were trained on P3M-10k, TR-humans, AM-2k, AIM-500, Human-2k, Distinctions-646, HIM2K and PPM-100,
// several of which are released for non-commercial research only, which the card does not address), converted to
// FP16 with FP32 input and output (490 MB). The graph is the fork's own, installed beside the published models
// through the fork's local listing, as Porto is.

use std::sync::Arc;

use image::DynamicImage;
use image::imageops::FilterType::Triangle;

use super::{Matte, Matting};

use crate::error::InferenceError;
use crate::models::precision::Precision;
use crate::pipeline::Backend;
use crate::pipeline::session::GraphShape;
use crate::pipeline::{DataPipeline, OnOneGraph, SharedData, SingleGraph, checkpoint, reporter};
use crate::providers::ExecutionProvider;
use crate::providers::profile::EpProfile;
use crate::sessions::SessionHandle;

/// The graph's input and output side, in pixels.
pub(crate) const SIDE: u32 = 1024;

/// The ImageNet mean and standard deviation of each channel, which the graph's input is normalised by.
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DEVIATION: [f32; 3] = [0.229, 0.224, 0.225];

/// The fractions a run reports at.
const AFTER_INPUT: f64 = 0.1;
const AFTER_GRAPH: f64 = 0.9;
const AFTER_MATTE: f64 = 1.0;

/// The execution-provider tuning for Bilbao at `precision`: the provider defaults, on the CPU (see [`provider`]).
pub(crate) fn profile(_precision: Precision) -> EpProfile {
    EpProfile::default()
}

/// The provider Bilbao runs on when `requested` is asked for: the CPU wherever Core ML would be tried.
///
/// Measured on an M4 Pro Mac mini (24 GB) with ONNX Runtime's Core ML provider (ML Program, CPU and GPU and all
/// units): the compile fails after about seven minutes (`Unable to parse ML Program: in operation
/// .../aspp_deforms.2/atrous_conv/Conv: Required param 'pad' is missing`, the decoder's deformable atrous convolutions),
/// so `Auto` would spend that long before falling back. On the CPU provider the graph runs in 6 s at FP16 (4.4 s at
/// FP32), once per photograph. Other providers are left as asked: not measured.
pub(crate) fn provider(requested: ExecutionProvider) -> ExecutionProvider {
    if cfg!(target_os = "macos") && matches!(requested, ExecutionProvider::Auto | ExecutionProvider::CoreMl) {
        ExecutionProvider::Cpu
    } else {
        requested
    }
}

/// The pipeline running `operation`, which takes nothing but the photograph.
pub(crate) fn pipeline<B: Backend>(operation: &Matting) -> SharedData<B, Matte> {
    Arc::new(Bilbao { graph: SingleGraph::new(operation.display_name(), operation.artifact(), operation.profile()) })
}

/// One Bilbao run: the graph.
struct Bilbao {
    graph: SingleGraph,
}

impl OnOneGraph for Bilbao {
    fn graph(&self) -> &SingleGraph {
        &self.graph
    }
}

/// The graph's input for `source`: the photograph stretched to [`SIDE`] square, ImageNet-normalised, planar CHW.
///
/// # Errors
///
/// [`InferenceError::Untileable`] where `source` has no area.
fn tensor(source: &DynamicImage) -> Result<Vec<f32>, InferenceError> {
    let (width, height) = (source.width(), source.height());

    if width == 0 || height == 0 {
        return Err(InferenceError::Untileable { width, height });
    }

    // A triangle filter, which downscales by averaging over the reduction as the reference's bilinear resize does.
    let resized = source.resize_exact(SIDE, SIDE, Triangle).into_rgb32f();
    let plane = (SIDE * SIDE) as usize;
    let mut tensor = vec![0.0_f32; 3 * plane];

    for (index, pixel) in resized.pixels().enumerate() {
        for channel in 0..3 {
            tensor[channel * plane + index] = (pixel[channel].clamp(0.0, 1.0) - MEAN[channel]) / DEVIATION[channel];
        }
    }

    Ok(tensor)
}

impl<B: Backend> DataPipeline<B> for Bilbao {
    type Output = Matte;

    fn run(
        &self,
        input: &DynamicImage,
        sessions: &[SessionHandle<B::Session>],
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Matte, InferenceError> {
        let report = reporter(progress);

        if cancelled() {
            return Err(InferenceError::Cancelled);
        }

        let tensor = tensor(input)?;

        checkpoint(&report, cancelled, AFTER_INPUT)?;

        let side = SIDE as usize;
        let mut logits = vec![0.0_f32; side * side];
        B::run_graph(
            &sessions[0],
            &tensor,
            GraphShape::new(3, side, side),
            &mut logits,
            GraphShape::new(1, side, side),
        )
        .map_err(InferenceError::run(&self.graph.name, 0))?;

        // Reported, but not a place a stop is honoured. The graph is one call — 6 s on its own, and minutes on a CPU
        // shared with other runs — which a stop now terminates while it runs (`session::terminable`); one that arrives
        // after it has returned used to be noticed here, after all of that was spent, and the matte thrown away with it:
        // paging through a batch of lens blurs cancelled each photograph's Bilbao this way and ran it again from nothing
        // on the way back. What is left is a sigmoid over a megapixel, so the matte is finished and handed back, which
        // is what lets `execute` store it; the run it was for notices the stop at its own next checkpoint.
        report(AFTER_GRAPH);

        let coverage: Vec<f32> = logits.iter().map(|&logit| 1.0 / (1.0 + (-logit).exp())).collect();
        tracing::debug!(operation = %self.graph.name, width = input.width(), height = input.height(), "salient subject matted");

        report(AFTER_MATTE);

        Ok(Matte::new(SIDE, SIDE, &coverage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    use crate::models::ArtifactId;
    use crate::pipeline::Model;
    use crate::pipeline::test_support::stub_backend_runs;

    /// A session answering with logits that rise left to right across the frame, and remembering the input's mean.
    struct FakeSession {
        means: Arc<Mutex<Vec<f32>>>,
    }

    struct Fake;

    impl crate::pipeline::Backend for Fake {
        type Session = FakeSession;
        type Error = std::io::Error;

        async fn acquire(
            &self,
            _artifact: &ArtifactId,
            _profile: &EpProfile,
            _requested: ExecutionProvider,
            _interest: &crate::sessions::Interest,
        ) -> Result<SessionHandle<Self::Session>, crate::error::SessionError> {
            unreachable!("the pipeline is handed its sessions; it never acquires one")
        }

        fn run_graph(
            handle: &SessionHandle<Self::Session>,
            input: &[f32],
            input_shape: GraphShape,
            output: &mut [f32],
            output_shape: GraphShape,
        ) -> Result<(), Self::Error> {
            assert_eq!(input_shape.to_string(), "[1,3,1024,1024]");
            assert_eq!(output_shape.to_string(), "[1,1,1024,1024]");
            handle.session().means.lock().unwrap().push(input.iter().sum::<f32>() / input.len() as f32);
            for (index, value) in output.iter_mut().enumerate() {
                *value = (index % 1024) as f32 / 64.0 - 8.0;
            }

            Ok(())
        }

        stub_backend_runs!("Bilbao takes the shaped seam"; run_tile, run_named_outputs, run_weighted);
    }

    #[test]
    fn bilbao_opens_its_one_graph() {
        let pipeline = pipeline::<Fake>(&Matting::bilbao());
        let sessions = Model::<Fake>::sessions(pipeline.as_ref());

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].0.as_str(), "mt_bilbao_fp16");
    }

    #[test]
    fn a_photograph_of_any_size_is_matted_at_the_graphs_size_from_its_logits() {
        let means: Arc<Mutex<Vec<f32>>> = Arc::default();
        let handle = SessionHandle::held(FakeSession { means: Arc::clone(&means) }, ExecutionProvider::Cpu);
        // ImageNet's mean grey normalises to nothing.
        let grey = image::Rgb([124, 116, 104]);
        let image = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(1500, 1000, grey));
        let seen = Mutex::new(Vec::new());

        let matte = pipeline::<Fake>(&Matting::bilbao())
            .run(&image, &[handle], Some(&|fraction: f64| seen.lock().unwrap().push(fraction)), &|| false)
            .expect("a matte");

        assert_eq!(matte.dimensions(), (1024, 1024));
        // A logit of -8 is next to nothing, 0 is half, +8 next to all of it.
        assert!(matte.alpha_at(0, 10) < 0.001);
        assert!((matte.alpha_at(512, 10) - 0.5).abs() < 0.001);
        assert!(matte.alpha_at(1023, 10) > 0.999);
        assert!(means.lock().unwrap()[0].abs() < 0.01, "{:?}", means.lock().unwrap());
        assert_eq!(seen.into_inner().unwrap(), vec![AFTER_INPUT, AFTER_GRAPH, AFTER_MATTE]);
    }

    #[test]
    fn a_stop_before_the_graph_produces_no_matte() {
        let handle = SessionHandle::held(FakeSession { means: Arc::default() }, ExecutionProvider::Cpu);
        let image = DynamicImage::ImageRgb8(image::RgbImage::new(64, 48));
        let checks = std::cell::Cell::new(0_usize);
        let cancelled = || {
            checks.set(checks.get() + 1);
            checks.get() > 1
        };

        let outcome = pipeline::<Fake>(&Matting::bilbao()).run(&image, &[handle], None, &cancelled);

        assert!(matches!(outcome, Err(InferenceError::Cancelled)), "{outcome:?}");
    }

    #[test]
    fn a_stop_that_arrives_while_the_graph_runs_still_hands_back_its_matte() {
        let handle = SessionHandle::held(FakeSession { means: Arc::default() }, ExecutionProvider::Cpu);
        let image = DynamicImage::ImageRgb8(image::RgbImage::new(64, 48));
        // Clear at the two checks before the graph, stopped at every one after it.
        let checks = std::cell::Cell::new(0_usize);
        let cancelled = || {
            checks.set(checks.get() + 1);
            checks.get() > 2
        };

        let outcome = pipeline::<Fake>(&Matting::bilbao()).run(&image, &[handle], None, &cancelled);

        // Thrown away, the graph's whole run would be paid for again the next time the photograph is shown.
        assert!(outcome.is_ok(), "a matte the graph had finished was discarded for a stop: {outcome:?}");
    }

    #[test]
    fn bilbao_is_moved_off_core_ml_and_nothing_else_is() {
        let moved = if cfg!(target_os = "macos") { ExecutionProvider::Cpu } else { ExecutionProvider::Auto };

        assert_eq!(provider(ExecutionProvider::Auto), moved);
        assert_eq!(provider(ExecutionProvider::Cpu), ExecutionProvider::Cpu);
        assert_eq!(provider(ExecutionProvider::Cuda), ExecutionProvider::Cuda);
    }
}
