//! The live checks: Porto's real graph, the real ONNX Runtime, a real photograph and the trimap a lens blur builds for
//! it from Lisbon's real depth, end to end.
//!
//! `#[ignore]`d, as every live check in this crate is: the graphs are 104 MB and 2 x 324 MB, and the photograph is not
//! committed. Run them by hand with [`LIVE_DIR`] naming a directory that holds the photograph and the installation:
//!
//! ```text
//! $OPAI_LIVE_DIR/portrait.jpg                       a 3:4 portrait, a face at about 0.7 m with dark hair beside it
//! $OPAI_LIVE_DIR/opai-live-matting/models/...       mt_porto_fp32, dp_lisbon_landscape_fp16, dp_lisbon_portrait_fp16
//!
//! cargo test -p opai --release models::matting::live -- --ignored --nocapture --test-threads=1
//! ```
//!
//! A check whose photograph or graph is missing says so and passes, rather than failing a sweep that has neither.

// What the fake-backend suite cannot say: that the graph's input and output are named and shaped as Porto feeds and
// reads them, that a real trimap's unknown band comes back with strands in it rather than a hard edge, and how long a
// tile takes on each provider.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::live_support::{LIVE_DIR, kept_dir, live_application_at};
use crate::models::lens_blur::render::Refined;
use crate::models::lens_blur::trimap::{Band, matted_focus, trimap};
use crate::models::matting::porto::tiles::{tiles, unknown};
use crate::models::{Depth, Matting};
use crate::providers::ExecutionProvider;
use crate::{ExecuteOptions, Opai, Picture};

/// The application name, and the directory under [`LIVE_DIR`] the graphs are installed in.
const NAME: &str = "opai-live-matting";

/// The portrait's focus point, on the eyes, as fractions of the frame.
const EYES: (f64, f64) = (0.60, 0.30);

/// The portrait under [`LIVE_DIR`], or `None` with the reason printed.
async fn portrait() -> Option<Picture> {
    let Some(root) = std::env::var_os(LIVE_DIR).map(PathBuf::from) else {
        println!("skipped: {LIVE_DIR} is not set");
        return None;
    };
    let path = root.join("portrait.jpg");

    if !path.exists() {
        println!("skipped: {} is missing", path.display());
        return None;
    }

    Some(
        crate::image::load(&path)
            .await
            .unwrap_or_else(|err| panic!("{} did not decode: {err}", path.display())),
    )
}

/// The application, installed under [`LIVE_DIR`], or `None` with the reason printed where a graph is not installed.
async fn application() -> Option<Opai> {
    let dir = kept_dir(NAME);

    for artifact in Matting::porto().required_artifacts().into_iter().chain(Depth::lisbon().required_artifacts()) {
        let graph = dir.join("models").join(artifact.as_str()).join(format!("{artifact}.onnx"));
        if !graph.exists() {
            println!("skipped: {} is not installed", graph.display());
            return None;
        }
    }

    Some(live_application_at(NAME, &dir).await)
}

/// Mattes the portrait's subject on `provider`: the trimap a lens blur focused on the eyes builds, then Porto over it.
async fn mattes_the_portrait(provider: ExecutionProvider) {
    let (Some(opai), Some(picture)) = (application().await, portrait().await) else {
        return;
    };

    let map = opai.estimate_depth(&picture, &Depth::lisbon(), None).await.expect("an estimate").value;
    let focus_m = matted_focus(map.focus_metres(EYES.0, EYES.1));
    let started = Instant::now();
    let trimap = trimap(&Refined::of(&map, picture.pixels()), Band::around(focus_m));
    let built = started.elapsed();
    let tiles = tiles(&trimap).len();
    let matting = Matting::porto().with_trimap(Arc::new(trimap.clone()));

    // Twice, uncached, so the second run's time is the run alone rather than the first's compile.
    let mut timings = Vec::new();
    let mut matte = None;
    for _ in 0..2 {
        let options = ExecuteOptions { provider, cache: false, ..Default::default() };
        let started = Instant::now();
        let executed = opai.matte(&picture, &matting, Some(options)).await.expect("a matte");
        timings.push(started.elapsed());

        assert_eq!(executed.providers.actual, vec![provider], "the matte ran on another provider");
        matte = Some(executed.value);
    }
    let matte = matte.expect("two runs");

    // Strands rather than a hard edge: a share of the unknown band comes back partly covered.
    let (width, _) = picture.dimensions();
    let band: Vec<f32> = trimap
        .as_slice()
        .iter()
        .enumerate()
        .filter(|(_, value)| unknown(**value))
        .map(|(index, _)| {
            #[allow(clippy::cast_possible_truncation, reason = "a pixel index of a picture whose sides are u32")]
            let (x, y) = ((index % width as usize) as u32, (index / width as usize) as u32);
            matte.alpha_at(x, y)
        })
        .collect();
    let soft = band.iter().filter(|alpha| (0.1..=0.9).contains(*alpha)).count();
    #[allow(clippy::cast_precision_loss, reason = "a share of a pixel count")]
    let share = soft as f64 / band.len().max(1) as f64;

    println!(
        "\nportrait on {provider}: focus {focus_m:.2} m, trimap {built:.2?}, {tiles} tiles, {} unknown pixels; matte \
         first {:.2?}, second {:.2?} ({:.2?} a tile); {:.1}% of the band soft",
        band.len(),
        timings[0],
        timings[1],
        timings[1] / u32::try_from(tiles.max(1)).unwrap_or(1),
        share * 100.0
    );

    assert!(share >= 0.05, "only {:.1}% of the unknown band is partly covered", share * 100.0);
    for (index, known) in trimap.as_slice().iter().enumerate().step_by(997) {
        if !unknown(*known) {
            #[allow(clippy::cast_possible_truncation, reason = "a pixel index of a picture whose sides are u32")]
            let (x, y) = ((index % width as usize) as u32, (index / width as usize) as u32);
            assert_eq!(matte.alpha_at(x, y), *known, "a known pixel at ({x}, {y}) moved");
        }
    }
}

#[tokio::test]
#[ignore = "needs Porto's and Lisbon's graphs and the portrait under OPAI_LIVE_DIR; run by hand with --ignored"]
async fn porto_mattes_the_portraits_hair_on_the_cpu() {
    mattes_the_portrait(ExecutionProvider::Cpu).await;
}

#[tokio::test]
#[ignore = "needs an Apple GPU, Porto's and Lisbon's graphs and the portrait under OPAI_LIVE_DIR; run by hand"]
async fn porto_mattes_the_portraits_hair_on_coreml() {
    mattes_the_portrait(ExecutionProvider::CoreMl).await;
}
