//! Porto: ViTMatte-S, a matting model that estimates how much of each unknown pixel of a trimap a subject covers, and
//! the one matting model.
//!
//! One graph, run once per 1024-pixel tile that holds an unknown pixel — 17 to 21 tiles on a 12 to 24 megapixel
//! photograph whose trimap is a lens blur's — with one session held for all of them. Progress is reported once per
//! tile, and cancellation is checked before each.

// A run:
//
//   tiles::tiles     the tiles holding an unknown pixel
//   tiles::tensor    per tile: RGB to [-1, 1], the trimap as the fourth plane, zeros off the frame
//   run_graph        1 x 4 x 1024 x 1024 -> `alphas`, 1 x 1 x 1024 x 1024
//   tiles::Joined    the tiles' alphas cross-faded over their overlap; known pixels the trimap's own
//
// Which weights these are: `hustvl/vitmatte-small-composition-1k` (weights Apache-2.0, code MIT) as exported to ONNX by
// `Xenova/vitmatte-small-composition-1k` (opset 12), its input fixed to 1 x 4 x 1024 x 1024 so Core ML compiles it for
// one shape; checked against the dynamic graph to 1e-5. The graph is the fork's own for now, installed beside the
// published models through the fork's local listing, until its hosting is decided.

pub(crate) mod tiles;

use std::sync::Arc;

use image::DynamicImage;
use imaging::plane::Plane;
use imaging::tensor::Sampler;

use super::{Matte, Matting};

use crate::error::{InferenceError, UnsupportedReason};
use crate::models::precision::Precision;
use crate::pipeline::Backend;
use crate::pipeline::session::GraphShape;
use crate::pipeline::{DataPipeline, OnOneGraph, SharedData, SingleGraph, reporter};
use crate::providers::profile::{EpProfile, cpu_and_gpu_at_fp16};
use crate::sessions::SessionHandle;

use tiles::{Joined, PLANES, TILE};

/// The execution-provider tuning measured for Porto at `precision`: the provider defaults at FP32.
pub(crate) fn profile(precision: Precision) -> EpProfile {
    // Measured on an M4 Pro Mac mini (24 GB) with ONNX Runtime's Core ML provider (ML Program) against the CPU
    // provider, on the tiles of three photographs: 0.41 s a tile on Core ML's CPU and GPU after a first tile of 3.0 s
    // (the compile), 0.58 s on Core ML's CPU alone, and 0.65 s on the CPU provider. Core ML takes 896 of the graph's
    // 968 nodes; the rest (the windowed attention's rank-5 reshapes, an `Einsum` and a `Pad`) run on the CPU between
    // its partitions. The matte agrees between the two to 1.3e-5. At FP32 Core ML keeps a program off the Neural
    // Engine anyway, so the shared profile's FP16 setting does not apply.
    cpu_and_gpu_at_fp16(precision)
}

/// The pipeline running `operation`, or a refusal where it has not been handed a trimap.
pub(crate) fn pipeline<B: Backend>(operation: &Matting) -> Result<SharedData<B, Matte>, InferenceError> {
    let trimap = operation.trimap().ok_or_else(|| InferenceError::Unsupported {
        operation: operation.display_name(),
        reason: UnsupportedReason::Uninjected { missing: "a trimap" },
    })?;

    Ok(Arc::new(Porto {
        graph: SingleGraph::new(operation.display_name(), operation.artifact(), operation.profile()),
        trimap: Arc::clone(trimap),
    }))
}

/// One Porto run: the graph, and the trimap it mattes.
struct Porto {
    graph: SingleGraph,
    trimap: Arc<Plane>,
}

impl OnOneGraph for Porto {
    fn graph(&self) -> &SingleGraph {
        &self.graph
    }
}

impl<B: Backend> DataPipeline<B> for Porto {
    type Output = Matte;

    fn run(
        &self,
        input: &DynamicImage,
        sessions: &[SessionHandle<B::Session>],
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Matte, InferenceError> {
        let report = reporter(progress);
        let (width, height) = (input.width(), input.height());

        if (self.trimap.width(), self.trimap.height()) != (width, height) {
            return Err(InferenceError::run(&self.graph.name, 0)(std::io::Error::other(format!(
                "the trimap is {}x{}, the photograph {width}x{height}",
                self.trimap.width(),
                self.trimap.height()
            ))));
        }

        let tiles = tiles::tiles(&self.trimap);
        let sampler = Sampler::new(input);
        let side = TILE as usize;
        let mut tensor = vec![0.0_f32; PLANES * side * side];
        let mut alpha = vec![0.0_f32; side * side];
        let mut joined = Joined::new(width, height);

        for (index, &(x, y)) in tiles.iter().enumerate() {
            if cancelled() {
                return Err(InferenceError::Cancelled);
            }

            tiles::tensor(&sampler, &self.trimap, x, y, &mut tensor);
            B::run_graph(
                &sessions[0],
                &tensor,
                GraphShape::new(PLANES, side, side),
                &mut alpha,
                GraphShape::new(1, side, side),
            )
            .map_err(InferenceError::run(&self.graph.name, index))?;
            joined.add(&alpha, x, y);

            #[allow(clippy::cast_precision_loss, reason = "a few dozen tiles")]
            report((index + 1) as f64 / tiles.len() as f64);
        }

        tracing::debug!(operation = %self.graph.name, width, height, tiles = tiles.len(), "matte estimated");

        // A trimap with nothing unknown ran no tile, and is done.
        if tiles.is_empty() {
            report(1.0);
        }

        Ok(Matte::new(width, height, &joined.finished(&self.trimap)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    use crate::models::ArtifactId;
    use crate::pipeline::Model;
    use crate::pipeline::test_support::stub_backend_runs;
    use crate::providers::ExecutionProvider;

    /// What the fake was asked: each run's input and output shapes, and the trimap plane's mean, per tile.
    #[derive(Default)]
    struct Log {
        runs: Vec<(String, String, f32)>,
    }

    /// A session answering every tile with the mean of its trimap plane, so what a tile was fed is visible in the
    /// matte.
    struct FakeSession {
        log: Arc<Mutex<Log>>,
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
            assert_eq!(input.len(), input_shape.len());
            assert_eq!(output.len(), output_shape.len());

            let plane = &input[3 * output.len()..];
            let mean = plane.iter().sum::<f32>() / plane.len() as f32;
            output.fill(mean);
            handle
                .session()
                .log
                .lock()
                .unwrap()
                .runs
                .push((input_shape.to_string(), output_shape.to_string(), mean));

            Ok(())
        }

        stub_backend_runs!("Porto takes the shaped seam"; run_tile, run_named_outputs, run_weighted);
    }

    fn run(trimap: Plane, cancel_after: Option<usize>) -> (Result<Matte, InferenceError>, Arc<Mutex<Log>>) {
        let (width, height) = (trimap.width(), trimap.height());
        let log: Arc<Mutex<Log>> = Arc::default();
        let handle = SessionHandle::held(FakeSession { log: Arc::clone(&log) }, ExecutionProvider::Cpu);
        let pipeline = pipeline::<Fake>(&Matting::porto().with_trimap(Arc::new(trimap))).expect("a trimap was handed");
        let image = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(width, height, image::Rgb([90, 120, 150])));
        let checks = std::cell::Cell::new(0_usize);
        let cancelled = || {
            checks.set(checks.get() + 1);
            cancel_after.is_some_and(|after| checks.get() > after)
        };

        (pipeline.run(&image, &[handle], None, &cancelled), log)
    }

    #[test]
    fn porto_opens_its_one_graph() {
        let pipeline =
            pipeline::<Fake>(&Matting::porto().with_trimap(Arc::new(Plane::new(8, 8, 0.5)))).expect("a pipeline");
        let sessions = Model::<Fake>::sessions(pipeline.as_ref());

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].0.as_str(), "mt_porto_fp32");
    }

    #[test]
    fn a_matting_without_a_trimap_is_refused() {
        let Err(InferenceError::Unsupported { reason, .. }) = pipeline::<Fake>(&Matting::porto()) else {
            panic!("a matting with no trimap reached a pipeline");
        };

        assert_eq!(reason, UnsupportedReason::Uninjected { missing: "a trimap" });
    }

    #[test]
    fn each_tile_with_an_unknown_pixel_is_one_run_of_the_graphs_shape() {
        // Unknown pixels down column 1500 of a 2000 x 1200 photograph: of the three tiles across it, at 0, 896 and
        // 976, the last two hold it, in each of the two rows of tiles.
        let mut trimap = Plane::new(2000, 1200, 0.0);
        for y in 0..1200 {
            trimap.as_mut_slice()[y * 2000 + 1500] = 0.5;
        }

        let (matte, log) = run(trimap, None);
        let matte = matte.expect("a matte");

        let runs = &log.lock().unwrap().runs;
        assert_eq!(runs.len(), 4, "{runs:?}");
        assert!(runs.iter().all(|(input, output, _)| input == "[1,4,1024,1024]" && output == "[1,1,1024,1024]"));

        // Known pixels are exactly the trimap's; the unknown ones, the fake's answer (a tile's trimap mean, small).
        assert_eq!(matte.alpha_at(10, 10), 0.0);
        let unknown = matte.alpha_at(1500, 600);
        assert!(unknown > 0.0 && unknown < 0.01, "{unknown}");
    }

    #[test]
    fn a_stop_between_tiles_produces_no_matte() {
        let (matte, log) = run(Plane::new(3000, 2000, 0.5), Some(2));

        assert!(matches!(matte, Err(InferenceError::Cancelled)), "{matte:?}");
        assert_eq!(log.lock().unwrap().runs.len(), 2);
    }

    #[test]
    fn a_trimap_of_another_size_is_refused_rather_than_misread() {
        let log: Arc<Mutex<Log>> = Arc::default();
        let handle = SessionHandle::held(FakeSession { log }, ExecutionProvider::Cpu);
        let pipeline = pipeline::<Fake>(&Matting::porto().with_trimap(Arc::new(Plane::new(8, 8, 0.5)))).unwrap();
        let image = DynamicImage::ImageRgb8(image::RgbImage::new(16, 8));

        assert!(matches!(pipeline.run(&image, &[handle], None, &|| false), Err(InferenceError::Run { .. })));
    }

    #[test]
    fn progress_is_reported_once_a_tile_and_ends_at_one() {
        let log: Arc<Mutex<Log>> = Arc::default();
        let handle = SessionHandle::held(FakeSession { log }, ExecutionProvider::Cpu);
        let pipeline = pipeline::<Fake>(&Matting::porto().with_trimap(Arc::new(Plane::new(1920, 300, 0.5)))).unwrap();
        let image = DynamicImage::ImageRgb8(image::RgbImage::new(1920, 300));
        let seen = Mutex::new(Vec::new());

        pipeline
            .run(&image, &[handle], Some(&|fraction: f64| seen.lock().unwrap().push(fraction)), &|| false)
            .expect("a matte");

        assert_eq!(seen.into_inner().unwrap(), vec![0.5, 1.0]);
    }

    #[test]
    fn porto_runs_on_the_provider_defaults_at_full_precision() {
        assert_eq!(profile(Precision::Fp32), EpProfile::default());
    }
}
