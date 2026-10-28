//! Matting the subject of a chain's lens blur, as `process` does it after the depth: through the real trimap, the real
//! Porto pipeline and the real cached run, on the fake backend.

use super::*;
use crate::inference::depth_map::ChainMatte;
use crate::models::lens_blur::tests::two_planes;
use crate::models::{Analysis, FNumber, Fraction, LensBlur, LensBlurEdges, Subject};

/// The matting graph.
const PORTO: &str = "mt_porto_fp32";

/// The landscape depth graph.
const LANDSCAPE: &str = "dp_lisbon_landscape_fp16";

fn blur(f_number: f64, x: f64, edges: LensBlurEdges) -> Operation {
    let Operation::LensBlur(blur) = LensBlur::lisbon(
        FNumber::new(f_number).expect("in range"),
        Fraction::new(x).expect("in range"),
        Fraction::CENTRE,
    ) else {
        unreachable!("Lisbon builds a lens blur");
    };

    Operation::LensBlur(blur.with_edges(edges))
}

fn matted(f_number: f64, x: f64) -> Operation {
    blur(f_number, x, LensBlurEdges::Matte)
}

/// How many times `artifact`'s graph ran: once a tile for Porto.
fn runs(backend: &Fake, artifact: &str) -> usize {
    backend.log().ran.iter().filter(|ran| *ran == artifact).count()
}

/// A checkerboard of 4-pixel squares, as the depth tests blur: the fake scene behind it is a slope from two metres on
/// the left to five on the right. With no EXIF it is assumed at f/8 and its focal length is the fake depth's, about
/// 35 mm, so on 960 x 640 pixels only an aperture near f/1 blurs the far end by the 3 px a matte needs.
fn checkered() -> Picture {
    let pixels = image::RgbImage::from_fn(960, 640, |x, y| {
        if (x / 4 + y / 4) % 2 == 0 { image::Rgb([230, 200, 40]) } else { image::Rgb([20, 40, 120]) }
    });

    Picture::new("/pictures/holiday.jpg", DynamicImage::ImageRgb8(pixels), "cafebabecafebabe")
}

async fn enhanced(backend: &Fake, cache: &Memo, chain: &[Operation]) -> Picture {
    outcome(backend, cache, chain).await.picture
}

/// [`enhanced`] with what matting came to.
async fn outcome(backend: &Fake, cache: &Memo, chain: &[Operation]) -> Enhanced {
    let enhanced = cached_enhancement(backend, Some(cache), &checkered(), chain, OutputDepth::Eight).await;
    settled();

    enhanced.expect("the chain runs")
}

#[tokio::test]
async fn the_subject_is_matted_once_across_a_change_of_aperture_and_again_for_a_change_of_focus() {
    let (backend, cache) = (Fake::new(), store());

    let first = outcome(&backend, &cache, &[matted(1.0, 0.1)]).await;
    assert!(matches!(first.matte, ChainMatte::Matted), "{:?}", first.matte);
    let first = first.picture;
    let mattes = runs(&backend, PORTO);
    assert!(mattes > 0, "a lens blur at f/1 matted nothing");

    // Another aperture: the trimap is the same, so neither the depth nor the matte is computed again.
    let opened = outcome(&backend, &cache, &[matted(1.1, 0.1)]).await;
    assert!(
        matches!(opened.matte, ChainMatte::Matted),
        "a matte served from the store is a matte all the same"
    );
    let opened = opened.picture;
    assert_eq!(runs(&backend, PORTO), mattes, "an f-number change matted the subject again");
    assert_eq!(runs(&backend, LANDSCAPE), 1);
    assert_ne!(first.pixels(), opened.pixels(), "opening the aperture changed nothing");

    // Another focus: the band moves, so the trimap does, and the subject is matted again; the depth is not.
    enhanced(&backend, &cache, &[matted(1.0, 0.0)]).await;
    assert!(runs(&backend, PORTO) > mattes, "a focus change was served the old matte");
    assert_eq!(runs(&backend, LANDSCAPE), 1, "a focus change estimated the depth again");
}

#[tokio::test]
async fn edges_from_the_depth_alone_matte_nothing_and_render_as_before_matting() {
    let (backend, cache) = (Fake::new(), store());

    let depth_only = outcome(&backend, &cache, &[blur(1.0, 0.1, LensBlurEdges::Depth)]).await;
    assert!(matches!(depth_only.matte, ChainMatte::NotAsked), "{:?}", depth_only.matte);
    let depth_only = depth_only.picture;
    assert_eq!(runs(&backend, PORTO), 0, "--lb-edges depth ran the matting model");

    let matted = enhanced(&backend, &cache, &[matted(1.0, 0.1)]).await;
    assert!(runs(&backend, PORTO) > 0);
    assert_ne!(depth_only.pixels(), matted.pixels(), "the matte changed nothing");
    assert_ne!(depth_only.identity(), matted.identity());
}

#[tokio::test]
async fn a_matting_model_that_cannot_be_installed_or_opened_leaves_the_render_to_the_depth() {
    let reference = {
        let (backend, cache) = (Fake::new(), store());
        enhanced(&backend, &cache, &[blur(1.0, 0.1, LensBlurEdges::Depth)]).await
    };

    for (broken, installed) in [
        (Fake { fails_to_install: Some(PORTO.to_string()), ..Fake::new() }, false),
        (Fake { fails_to_open: Some(PORTO.to_string()), ..Fake::new() }, true),
        (Fake { fails_to_run: Some(PORTO.to_string()), ..Fake::new() }, true),
    ] {
        let cache = store();
        let fallen_back = outcome(&broken, &cache, &[matted(1.0, 0.1)]).await;

        assert_eq!(fallen_back.picture.pixels(), reference.pixels(), "the fallback is not the depth-only render");
        // Not installed is expected, and said quietly; a model that is there and fails is a failure to show.
        match (&fallen_back.matte, installed) {
            (ChainMatte::Unavailable(_), false) | (ChainMatte::Failed(_), true) => {}
            (other, _) => panic!("{other:?} for a graph {}installed", if installed { "" } else { "not " }),
        }
    }

    // A graph that is not installed is asked for once, not once per lens blur.
    let backend = Fake { fails_to_install: Some(PORTO.to_string()), ..Fake::new() };
    let cache = store();
    let twice = outcome(&backend, &cache, &[matted(1.0, 0.1), matted(1.0, 0.1)]).await;
    assert!(matches!(twice.matte, ChainMatte::Unavailable(_)), "{:?}", twice.matte);
    assert!(backend.log().ran.iter().all(|ran| ran != PORTO));
}

#[tokio::test]
async fn a_stop_while_matting_stops_the_run() {
    let cancel = CancellationToken::new();
    let backend = Fake { cancels: Some((PORTO.to_string(), cancel.clone())), ..Fake::new() };
    let options = ProcessOptions { cancel, cache: false, ..Default::default() };

    let outcome = process(&backend, None, &checkered(), &[matted(1.0, 0.1)], Some(options)).await;

    assert!(matches!(outcome, Err(InferenceError::Cancelled)), "{:?}", outcome.map(|_| ()));
}

#[tokio::test]
async fn a_blur_too_small_to_show_an_edge_mattes_nothing() {
    // The fake photograph has no EXIF, so it is assumed at f/8: f/7 adds well under 3 px anywhere on 960 x 640.
    let (backend, cache) = (Fake::new(), store());

    let small = outcome(&backend, &cache, &[matted(7.0, 0.5)]).await;

    assert!(matches!(small.matte, ChainMatte::Skipped), "{:?}", small.matte);
    assert_eq!(runs(&backend, PORTO), 0, "a blur of a pixel or two was matted");
    assert_eq!(runs(&backend, LANDSCAPE), 1);
}

#[tokio::test]
async fn two_matted_lens_blurs_report_one_bar_that_only_moves_forward() {
    // The depth and two mattes, each reporting its own run from 0 to 1 as `execute` does: the head of the first slot
    // in turn, the second matte from where the first left off.
    let reporting = |analysis: Analysis, options: Option<ExecuteOptions>| {
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
    };
    let (reports, options) = recording();
    let options = ProcessOptions { cache: false, ..options };

    let enhanced = process_with(
        &Fake::new(),
        None,
        &checkered(),
        &[matted(1.4, 0.5), matted(2.0, 0.5)],
        Some(options),
        |_| std::future::ready(Ok(Faces::empty())),
        |depth, options| {
            reporting(Analysis::Depth(depth), options);
            std::future::ready(Ok(two_planes(8, 8, 1.0, 2.0)))
        },
        |matting, options| {
            reporting(Analysis::Matting(matting), options);
            std::future::ready(Ok(Matte::new(8, 8, &[1.0; 64])))
        },
    )
    .await
    .expect("the chain runs");

    assert!(matches!(enhanced.matte, ChainMatte::Matted), "{:?}", enhanced.matte);
    let reports = reports.lock().unwrap();
    assert_never_decreases(&reports, "over a depth estimate, two mattes and their chain");
    let mattes = reports
        .iter()
        .filter(|report| matches!(&*report.subject, Subject::Analysis(Analysis::Matting(_))));
    // Bilbao once, and each blur's matte; Bilbao's matte of all 1 finds no hole, so neither is matted a second time.
    assert_eq!(mattes.count(), 9, "Bilbao and both mattes did not report");
    assert_eq!(reports.last().map(|report| report.chain_fraction), Some(1.0));
}
