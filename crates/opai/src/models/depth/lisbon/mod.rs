//! Lisbon: MoGe-2 ViT-B, a monocular geometry model that places every pixel of a photograph in space, and the one
//! depth model.
//!
//! Two graphs, one per orientation, of which a run opens the one its photograph needs. One run, three outputs read by
//! name. Progress is reported at three phase boundaries — 0.1 once the tensor is built, 0.9 once the graph returns,
//! 1.0 once the distances are recovered — and cancellation is checked at the same three points.

// A run:
//
//   input::stretched    the photograph resized to fill the graph's frame, unit RGB, CHW
//   run_named_outputs   one graph run -> points, mask, metric_scale, addressed by name (`normal` is not read)
//   recover::recover    the shift along z and the focal length, then metres
//   input::unstretched  the map resampled from the graph's frame to the photograph's aspect
//
// Which weights these are: MoGe-2 ViT-B with normals (`Ruicheng/moge-2-vitb-normal-onnx`, MIT; a DINOv2 backbone,
// Apache-2.0), re-exported at a fixed size with its token count fixed at 3600, the shape arithmetic folded and the 40
// edge-mode `Pad`s rewritten as `Slice` + `Concat` (exact), converted to FP16 with FP32 inputs and outputs. The dynamic
// export is split by Core ML into 21 partitions; this one into 3, taking 1041 of its 1043 nodes. 3600 tokens rather
// than the 2000 that would halve the run, because at 2000 the canopy of a test photograph moved from 2.22 m to 2.04 m.

pub(crate) mod input;

use std::sync::Arc;

use image::DynamicImage;

use super::recover::{PointMap, Unmeasurable, recover};
use super::{Depth, DepthMap};

use crate::error::InferenceError;
use crate::models::ArtifactId;
use crate::models::precision::Precision;
use crate::pipeline::Backend;
use crate::pipeline::session::{GraphShape, NamedOutput};
use crate::pipeline::{DataPipeline, OnOneGraph, SharedData, SingleGraph, checkpoint, reporter};
use crate::providers::profile::{EpProfile, cpu_and_gpu_at_fp16};
use crate::sessions::SessionHandle;

/// Which of the two graphs a photograph is run through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Orientation {
    /// 1280 x 1024: a photograph at least as wide as it is tall.
    Landscape,
    /// 1024 x 1280: a photograph taller than it is wide.
    Portrait,
}

impl Orientation {
    /// Both, in the order the operation lists their artifacts.
    pub(crate) const ALL: [Self; 2] = [Self::Landscape, Self::Portrait];

    /// The graph a `width` by `height` photograph is run through. A square one is landscape.
    pub(crate) const fn of(width: u32, height: u32) -> Self {
        if width >= height { Self::Landscape } else { Self::Portrait }
    }

    // Not tunables: each graph is exported at exactly this input, and its token count was fixed for it.
    /// The graph's input size, width by height.
    pub(crate) const fn size(self) -> (u32, u32) {
        match self {
            Self::Landscape => (1280, 1024),
            Self::Portrait => (1024, 1280),
        }
    }

    /// What the graph's artifact name carries between the codename and the precision.
    pub(crate) const fn suffix(self) -> &'static str {
        match self {
            Self::Landscape => "_landscape",
            Self::Portrait => "_portrait",
        }
    }
}

/// The graph's three outputs this reads, by the names it declares them under. The fourth, `normal`, is not read.
const OUTPUTS: [&str; 3] = ["points", "mask", "metric_scale"];

/// The fractions a run reports at.
const AFTER_INPUT: f64 = 0.1;
const AFTER_GRAPH: f64 = 0.9;
const AFTER_RECOVERY: f64 = 1.0;

/// The execution-provider tuning measured for Lisbon at `precision`: Core ML off the Neural Engine at FP16.
pub(crate) fn profile(precision: Precision) -> EpProfile {
    // Measured on an M4 Pro Mac mini (24 GB) against ONNX Runtime's Core ML provider at FP16, 3600 tokens: the graph
    // runs in 0.46 s on the CPU and GPU after a first load of 65 to 68 s per graph (the compile, kept with the model
    // afterwards, so a later process loads it in about half a second), against 2.9 to 3.1 s on the CPU provider. These
    // time the graph run alone; a whole call adds the input and the recovery (0.62 to 0.68 s on Core ML, and 6.5 to
    // 9.6 s on the CPU provider in a test build, which runs them unoptimised). The
    // two agree to a correlation of 0.999998, and the distances they recover to within 5 mm at two metres. Peak
    // memory is 7.8 GB in a process that compiles both graphs, 2.2 GB once they are compiled, and 7.2 GB on the CPU
    // provider. The Neural Engine is left out: its compile is slower still, and on a vision transformer of the same
    // kind it ran no faster — measured on that graph rather than on this one.
    cpu_and_gpu_at_fp16(precision)
}

/// The pipeline running `operation` over a photograph of `dimensions`, as the family's contract seam hands it back.
pub(crate) fn pipeline<B: Backend>(operation: Depth, dimensions: (u32, u32)) -> SharedData<B, DepthMap> {
    let orientation = Orientation::of(dimensions.0, dimensions.1);

    Arc::new(Lisbon::new(
        operation.graph(orientation),
        operation.profile(),
        operation.display_name(),
        orientation,
    ))
}

/// One Lisbon run: the graph of one orientation, the settings it opens it under, and what it is called in a failure.
struct Lisbon {
    /// The graph this run opens, and the name it reports a failure in.
    graph: SingleGraph,
    /// Which of the two graphs that is, and so the frame the photograph is fitted into.
    orientation: Orientation,
}

impl Lisbon {
    fn new(artifact: ArtifactId, profile: EpProfile, name: String, orientation: Orientation) -> Self {
        Self { graph: SingleGraph::new(name, artifact, profile), orientation }
    }
}

impl OnOneGraph for Lisbon {
    fn graph(&self) -> &SingleGraph {
        &self.graph
    }
}

impl<B: Backend> DataPipeline<B> for Lisbon {
    type Output = DepthMap;

    fn run(
        &self,
        input: &DynamicImage,
        sessions: &[SessionHandle<B::Session>],
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<DepthMap, InferenceError> {
        let report = reporter(progress);

        if cancelled() {
            return Err(InferenceError::Cancelled);
        }

        let tensor = input::stretched(input, self.orientation)?;

        checkpoint(&report, cancelled, AFTER_INPUT)?;

        let (width, height) = self.orientation.size();
        let pixels = (width * height) as usize;
        let (mut points, mut mask, mut metric_scale) = (vec![0.0_f32; pixels * 3], vec![0.0_f32; pixels], [0.0_f32]);

        let mut outputs = [
            NamedOutput { name: OUTPUTS[0], buffer: &mut points },
            NamedOutput { name: OUTPUTS[1], buffer: &mut mask },
            NamedOutput { name: OUTPUTS[2], buffer: &mut metric_scale },
        ];

        B::run_named_outputs(&sessions[0], &tensor, GraphShape::new(3, width as usize, height as usize), &mut outputs)
            .map_err(InferenceError::run(&self.graph.name, 0))?;

        checkpoint(&report, cancelled, AFTER_GRAPH)?;

        let map = PointMap {
            points: &points,
            mask: &mask,
            metric_scale: metric_scale[0],
            width: width as usize,
            height: height as usize,
        };
        let recovered = recover(&map).map_err(|unmeasurable| match unmeasurable {
            Unmeasurable::TooFewPoints { valid } => {
                InferenceError::Unmeasurable { operation: self.graph.name.clone(), valid }
            }
            // The graph's own output is wrong, not the photograph's content: a failed run, said as one.
            Unmeasurable::NoMetricScale => InferenceError::run(&self.graph.name, 0)(std::io::Error::other(
                "the model's metric scale is not a positive number",
            )),
        })?;

        // The focal length, measured against the graph's diagonal, is the photograph's: the photograph fills the
        // frame. It is an approximation for a photograph of another aspect, whose pixels the stretch made non-square
        // while the model's image plane assumes square ones; the error is that of reading a field of view off a
        // picture squeezed by up to a sixth, well inside the model's own uncertainty about focal length.
        let focal_35mm = recovered.focal_35mm();
        tracing::debug!(
            operation = %self.graph.name,
            shift = recovered.shift,
            metric_scale = metric_scale[0],
            focal_35mm,
            "depth recovered"
        );
        let size = input::map_size(input.width(), input.height(), self.orientation);
        let metres = input::unstretched(recovered.metres, (width, height), size);

        let map = DepthMap::new(size.0, size.1, &metres, focal_35mm);

        report(AFTER_RECOVERY);

        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    use crate::models::depth::recover::tests::synthetic;
    use crate::pipeline::Model;
    use crate::pipeline::test_support::stub_backend_runs;
    use crate::providers::ExecutionProvider;

    /// What the fake was asked: the input's length and the outputs' names, per run.
    #[derive(Default)]
    struct Log {
        runs: Vec<(usize, Vec<String>)>,
    }

    /// A session answering with a synthetic point map of a known focal length and shift.
    struct FakeSession {
        log: Arc<Mutex<Log>>,
        /// Whether the mask marks every point valid, or none.
        valid: bool,
    }

    /// A backend with no ONNX Runtime that answers the named-output call as the graph would.
    struct Fake;

    /// The focal length, shift and metric scale the fake's point map is made from.
    const FOCAL: f64 = 1.6;
    const SHIFT: f64 = 0.7;
    const SCALE: f64 = 3.0;

    /// The fake scene's distance at a pixel of the graph's frame, in metres: a slope from two metres to five.
    fn scene(x: usize, _y: usize, width: usize) -> f64 {
        2.0 + 3.0 * x as f64 / width as f64
    }

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

        fn run_named_outputs(
            handle: &SessionHandle<Self::Session>,
            input: &[f32],
            input_shape: GraphShape,
            outputs: &mut [NamedOutput<'_>],
        ) -> Result<(), Self::Error> {
            let session = handle.session();
            session
                .log
                .lock()
                .unwrap()
                .runs
                .push((input.len(), outputs.iter().map(|o| o.name.to_string()).collect()));

            assert_eq!(input.len(), input_shape.len(), "the graph was fed a buffer its declared shape does not fit");

            let (width, height) = (input_shape.width, input_shape.height);
            for output in outputs.iter_mut() {
                match output.name {
                    "points" => {
                        output
                            .buffer
                            .copy_from_slice(&synthetic(width, height, FOCAL, SHIFT, SCALE, |x, y| scene(x, y, width)))
                    }
                    "mask" => output.buffer.fill(if session.valid { 1.0 } else { 0.0 }),
                    "metric_scale" => output.buffer[0] = SCALE as f32,
                    other => panic!("Lisbon asked for an output named {other}"),
                }
            }

            Ok(())
        }

        stub_backend_runs!("a depth run takes the named-output seam"; run_tile, run_graph, run_weighted);
    }

    fn session(valid: bool) -> (SessionHandle<FakeSession>, Arc<Mutex<Log>>) {
        let log: Arc<Mutex<Log>> = Arc::default();

        (SessionHandle::held(FakeSession { log: Arc::clone(&log), valid }, ExecutionProvider::Cpu), log)
    }

    fn source(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(image::RgbImage::from_pixel(width, height, image::Rgb([90, 120, 150])))
    }

    fn run(width: u32, height: u32, valid: bool) -> (Result<DepthMap, InferenceError>, Arc<Mutex<Log>>) {
        let pipeline = pipeline::<Fake>(Depth::lisbon(), (width, height));
        let (handle, log) = session(valid);

        (pipeline.run(&source(width, height), &[handle], None, &|| false), log)
    }

    #[test]
    fn each_orientation_opens_its_own_graph_and_only_that_one() {
        for ((width, height), expected) in [
            ((3000, 2000), "dp_lisbon_landscape_fp16"),
            ((2000, 3000), "dp_lisbon_portrait_fp16"),
            ((800, 800), "dp_lisbon_landscape_fp16"),
        ] {
            let pipeline = Lisbon::new(
                Depth::lisbon().graph(Orientation::of(width, height)),
                Depth::lisbon().profile(),
                Depth::lisbon().display_name(),
                Orientation::of(width, height),
            );

            let sessions = Model::<Fake>::sessions(&pipeline);
            assert_eq!(sessions.len(), 1, "a depth run opens one graph");
            assert_eq!(sessions[0].0.as_str(), expected, "{width}x{height}");
            assert_eq!(Model::<Fake>::required(&pipeline).len(), 1);
        }
    }

    #[test]
    fn the_graph_is_fed_its_orientations_frame_and_asked_for_three_outputs_by_name() {
        for (width, height) in [(3000, 2000), (2000, 3000)] {
            let (map, log) = run(width, height, true);
            map.expect("a run");

            let runs = &log.lock().unwrap().runs;
            assert_eq!(runs.len(), 1, "a depth estimate is one graph run");
            assert_eq!(runs[0].0, 3 * 1280 * 1024);
            assert_eq!(runs[0].1, vec!["points", "mask", "metric_scale"]);
        }
    }

    #[test]
    fn the_map_covers_the_photograph_at_its_own_aspect() {
        // The spec's portrait scenario, and a 3:2 landscape one.
        for ((width, height), size) in [
            ((3064, 4080), (961, 1280)),
            ((3000, 2000), (1280, 853)),
            ((1280, 1024), (1280, 1024)),
            // A photograph smaller than the graph keeps its own size.
            ((640, 480), (640, 480)),
        ] {
            let map = run(width, height, true).0.expect("a run");

            assert_eq!(map.dimensions(), size, "a {width}x{height} photograph");
        }
    }

    #[test]
    fn the_distances_are_the_scenes_in_metres() {
        // A photograph of exactly the graph's frame is not resampled, so the map is the recovered scene itself.
        let map = run(1280, 1024, true).0.expect("a run");

        for x in [0, 300, 640, 1279] {
            let expected = scene(x as usize, 0, 1280);
            let found = f64::from(map.metres_at(x, 512));
            assert!((found - expected).abs() < 1e-3 * expected, "at column {x}: {expected} m came back as {found} m");
        }

        // `FOCAL` half-diagonals is a 34.6 mm lens on a full frame.
        let focal = map.focal_35mm().expect("a focal length");
        assert!((f64::from(focal) - FOCAL * 21.633).abs() < 0.05, "{focal}");
    }

    #[test]
    fn a_photograph_the_model_placed_nothing_of_is_refused() {
        let outcome = run(1280, 1024, false).0;

        assert!(matches!(outcome, Err(InferenceError::Unmeasurable { valid: 0, .. })), "{outcome:?}");
    }

    #[test]
    fn the_three_phase_fractions_are_reported_in_order() {
        let reported: Arc<Mutex<Vec<f64>>> = Arc::default();
        let sink = Arc::clone(&reported);
        let (handle, _log) = session(true);

        pipeline::<Fake>(Depth::lisbon(), (640, 480))
            .run(
                &source(640, 480),
                &[handle],
                Some(&|fraction: f64| sink.lock().unwrap().push(fraction)),
                &|| false,
            )
            .expect("a run");

        assert_eq!(*reported.lock().unwrap(), vec![AFTER_INPUT, AFTER_GRAPH, AFTER_RECOVERY]);
    }

    #[test]
    fn a_run_cancelled_at_any_of_the_three_points_produces_no_map() {
        for after in 0..3 {
            let checks = std::cell::Cell::new(0_usize);
            let (handle, _log) = session(true);

            let outcome =
                pipeline::<Fake>(Depth::lisbon(), (640, 480)).run(&source(640, 480), &[handle], None, &|| {
                    let seen = checks.get();
                    checks.set(seen + 1);
                    seen >= after
                });

            assert!(matches!(outcome, Err(InferenceError::Cancelled)), "cancelled at check {after}: {outcome:?}");
        }
    }

    #[test]
    fn lisbon_runs_off_the_neural_engine_at_half_precision() {
        assert_eq!(profile(Precision::Fp16), cpu_and_gpu_at_fp16(Precision::Fp16));
        assert_ne!(profile(Precision::Fp16), EpProfile::default());
    }
}
