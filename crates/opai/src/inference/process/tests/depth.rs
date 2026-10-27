//! Estimating the depth a chain's lens blurs are driven by, as `process` does it before the chain: through the real
//! depth pipeline and the real cached run, on the fake backend.

use std::sync::Arc;

use super::*;
use crate::inference::faces::FaceSelection;
use crate::inference::progress::{InferenceProgress, Stage};
use crate::models::lens_blur::tests::two_planes;
use crate::models::{
    Analysis, Denoise, Depth, Detection, FNumber, FaceRecovery, Faces, Fraction, LensBlur, Strength, Subject,
};

/// The landscape depth graph, which a landscape picture is estimated through.
const LANDSCAPE: &str = "dp_lisbon_landscape_fp16";

fn blur(f_number: f64, x: f64, y: f64) -> Operation {
    LensBlur::lisbon(
        FNumber::new(f_number).expect("in range"),
        Fraction::new(x).expect("in range"),
        Fraction::new(y).expect("in range"),
    )
}

fn denoise(strength: f64) -> Operation {
    Denoise::stockholm(FloatPrecision::Fp32, Strength::new(strength).expect("in range"))
}

/// How many times the depth graph ran.
fn estimates(backend: &Fake) -> usize {
    backend.log().ran.iter().filter(|artifact| *artifact == LANDSCAPE).count()
}

/// A checkerboard of 4-pixel squares, which any blur visibly changes: the shared gradient is linear, and a disc's
/// mean of a linear ramp is the ramp. Large enough that the fake scene, two to five metres, blurs by whole pixels.
fn checkered() -> Picture {
    let pixels = image::RgbImage::from_fn(960, 640, |x, y| {
        if (x / 4 + y / 4) % 2 == 0 { image::Rgb([230, 200, 40]) } else { image::Rgb([20, 40, 120]) }
    });

    Picture::new("/pictures/holiday.jpg", DynamicImage::ImageRgb8(pixels), "cafebabecafebabe")
}

async fn enhanced(backend: &Fake, cache: &Memo, chain: &[Operation]) -> Picture {
    let picture = cached_run(backend, Some(cache), &checkered(), chain, OutputDepth::Eight).await;
    settled();

    picture.expect("the chain runs")
}

#[tokio::test]
async fn the_depth_is_estimated_once_across_a_change_of_focus_and_of_aperture() {
    let (backend, cache) = (Fake::new(), store());

    let first = enhanced(&backend, &cache, &[blur(1.4, 0.1, 0.5)]).await;
    let refocused = enhanced(&backend, &cache, &[blur(1.4, 0.9, 0.5)]).await;
    let opened = enhanced(&backend, &cache, &[blur(1.0, 0.9, 0.5)]).await;

    assert_eq!(estimates(&backend), 1, "the depth was estimated again for a lens blur's own parameters");
    assert_ne!(first.pixels(), refocused.pixels(), "moving the focus changed nothing");
    assert_ne!(refocused.pixels(), opened.pixels(), "opening the aperture changed nothing");
    assert_ne!(first.identity(), refocused.identity());
}

#[tokio::test]
async fn the_depth_is_estimated_once_across_a_change_to_an_earlier_enhancement() {
    let (backend, cache) = (Fake::new(), store());

    enhanced(&backend, &cache, &[denoise(0.5), blur(1.4, 0.5, 0.5)]).await;
    enhanced(&backend, &cache, &[denoise(0.8), blur(1.4, 0.5, 0.5)]).await;

    assert_eq!(estimates(&backend), 1, "editing the denoise estimated the depth again");
}

#[tokio::test]
async fn an_identical_chain_is_served_and_a_chain_without_a_lens_blur_estimates_nothing() {
    let (backend, cache) = (Fake::new(), store());

    enhanced(&backend, &cache, &[kyoto(2.0)]).await;
    assert_eq!(estimates(&backend), 0, "a chain with no lens blur estimated depth");

    let first = enhanced(&backend, &cache, &[blur(1.4, 0.1, 0.5)]).await;
    let ran = backend.log().ran.len();
    let second = enhanced(&backend, &cache, &[blur(1.4, 0.1, 0.5)]).await;

    assert_eq!(backend.log().ran.len(), ran, "the repeated chain ran a graph");
    assert_eq!(first.pixels(), second.pixels());
    assert_eq!(first.identity(), second.identity());
}

#[tokio::test]
async fn the_lens_blur_is_applied_after_an_upscale_whatever_order_it_was_given_in() {
    let (backend, cache) = (Fake::new(), store());

    let blurred = enhanced(&backend, &cache, &[blur(1.4, 0.1, 0.5), kyoto(2.0)]).await;

    assert_eq!(blurred.dimensions(), (1920, 1280));
    // Estimated on the chain's input, not on the upscale's output: one graph, at the photograph's own orientation.
    assert_eq!(estimates(&backend), 1);
}

#[tokio::test]
async fn the_estimate_and_the_chain_report_on_one_bar_that_only_moves_forward() {
    let (backend, cache) = (Fake::new(), store());
    let (reports, options) = recording();

    process(&backend, Some(&cache), &checkered(), &[denoise(0.5), blur(1.4, 0.5, 0.5)], Some(options))
        .await
        .expect("the chain runs");
    settled();

    let reports = reports.lock().unwrap();
    assert_never_decreases(&reports, "over a depth estimate and its chain");
    assert!(
        reports.iter().any(|report| matches!(&*report.subject, crate::models::Subject::Analysis(_))),
        "the estimate reported nothing"
    );
    assert_eq!(reports.last().map(|report| report.chain_fraction), Some(1.0));
}

#[tokio::test]
async fn a_chain_of_a_face_recovery_and_a_lens_blur_reports_one_bar_that_only_moves_forward() {
    // Two analyses before the chain, each reporting its own run from 0 to 1 as `execute` does, handed the callback that
    // `process_with` gave it: the detection's is the first slot's head, and the depth's is the head of what the
    // detection left of it.
    let reporting = |analysis: Analysis| {
        move |options: Option<ExecuteOptions>| {
            let report = options.and_then(|options| options.on_progress).expect("progress was wanted");
            let subject = Arc::new(Subject::Analysis(analysis));
            for fraction in [0.0, 0.5, 1.0] {
                report(&InferenceProgress {
                    subject: Arc::clone(&subject),
                    stage: Stage::Running,
                    operation_fraction: fraction,
                    chain_fraction: fraction,
                });
            }
        }
    };
    let detecting = reporting(Detection::for_face_recovery());
    let estimating = reporting(Analysis::Depth(Depth::lisbon()));

    let (reports, options) = recording();
    let options = ProcessOptions { faces: FaceSelection::restorable(), cache: false, ..options };
    let chain = [FaceRecovery::santorini(FloatPrecision::Fp32, Faces::empty()), blur(1.4, 0.5, 0.5)];

    process_with(
        &Fake::new(),
        None,
        &checkered(),
        &chain,
        Some(options),
        |options| {
            detecting(options);
            std::future::ready(Ok(Faces::empty()))
        },
        |_, options| {
            estimating(options);
            std::future::ready(Ok(two_planes(8, 8, 1.0, 2.0)))
        },
    )
    .await
    .expect("the chain runs");

    let reports = reports.lock().unwrap();
    assert_never_decreases(&reports, "over a detection, a depth estimate and their chain");
    let subjects = |wanted: fn(&Subject) -> bool| reports.iter().filter(|report| wanted(&report.subject)).count();
    assert!(
        subjects(|subject| matches!(subject, Subject::Analysis(Analysis::Detection(_)))) > 0,
        "no detection"
    );
    assert!(
        subjects(|subject| matches!(subject, Subject::Analysis(Analysis::Depth(_)))) > 0,
        "no depth estimate"
    );
    assert!(
        subjects(|subject| matches!(subject, Subject::Enhancement(Operation::LensBlur(_)))) > 0,
        "the lens blur reported nothing"
    );
    assert_eq!(reports.last().map(|report| report.chain_fraction), Some(1.0));
}
