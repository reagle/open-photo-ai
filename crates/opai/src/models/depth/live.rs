//! The live checks: Lisbon's real graphs, the real ONNX Runtime, real photographs, end to end.
//!
//! `#[ignore]`d, as every live check in this crate is, and for two reasons of their own: the graphs are 2 x 324 MB, and
//! the photographs they are checked against are not committed. Run them by hand with [`LIVE_DIR`] naming a directory
//! that holds the photographs and the installation:
//!
//! ```text
//! $OPAI_LIVE_DIR/bonsai.jpg                       a 5:4 landscape, a bonsai at about two metres
//! $OPAI_LIVE_DIR/portrait.jpg                     a 3:4 portrait, a face at about 0.7 m
//! $OPAI_LIVE_DIR/street.jpg                       a 3:2 landscape at 181 mm equivalent, a face at about 3 m
//! $OPAI_LIVE_DIR/opai-live-depth/models/...       dp_lisbon_landscape_fp16 and dp_lisbon_portrait_fp16, installed
//!
//! cargo test -p opai models::depth::live -- --ignored --nocapture --test-threads=1
//! ```
//!
//! A check whose photograph or graph is missing says so and passes, rather than failing a sweep that has neither.

// What the fake-backend suite cannot say: that the graphs' outputs are named and shaped as `lisbon` reads them, that
// the recovered distances are in metres on a real photograph, and how long a run takes on each provider. The distances
// are checked against what the dynamic-shape graph gives each photograph at its own aspect, with room for the fixed
// graph's stretch and FP16: the bonsai's canopy at about 1.9 m and its pot behind it, the portrait's eyes at about
// 0.76 m (its camera recorded a subject distance of 0.69 m), the hooded face in the street at about 3.0 m. The focal
// length is checked against EXIF's 35 mm-equivalent one to 25%, which is what MoGe-2 manages on the bonsai (28.9 mm
// recovered against EXIF's 36, 20% short) and the portrait (EXIF's 24); it reads a telephoto's field of view badly (about 51 mm for the street's 181 mm), so the
// street's is not checked.

use std::path::PathBuf;
use std::time::Instant;

use crate::live_support::{LIVE_DIR, kept_dir, live_application_at};
use crate::models::Depth;
use crate::providers::ExecutionProvider;
use crate::{ExecuteOptions, Opai, Picture};

/// The application name, and the directory under [`LIVE_DIR`] the graphs are installed in.
const NAME: &str = "opai-live-depth";

/// One photograph and what is known about it.
struct Photograph {
    /// Its file name under [`LIVE_DIR`].
    file: &'static str,
    /// Points whose distance is known.
    points: &'static [Point],
    /// EXIF's 35 mm-equivalent focal length, where the model's estimate is checked against it.
    focal_35mm: Option<f32>,
}

/// A point of a photograph and the range its distance must fall in.
struct Point {
    name: &'static str,
    /// As fractions of the frame.
    at: (f64, f64),
    /// In metres.
    metres: (f32, f32),
}

const BONSAI: Photograph = Photograph {
    file: "bonsai.jpg",
    points: &[
        Point { name: "canopy", at: (0.55, 0.30), metres: (1.5, 2.6) },
        Point { name: "pot", at: (0.53, 0.80), metres: (1.9, 3.3) },
    ],
    focal_35mm: Some(36.0),
};

const PORTRAIT: Photograph = Photograph {
    file: "portrait.jpg",
    points: &[Point { name: "eyes", at: (0.60, 0.30), metres: (0.55, 0.95) }],
    focal_35mm: Some(24.0),
};

const STREET: Photograph = Photograph {
    file: "street.jpg",
    points: &[Point { name: "hooded face", at: (0.51, 0.41), metres: (2.2, 3.8) }],
    focal_35mm: None,
};

/// The photograph named `file` under [`LIVE_DIR`], or `None` with the reason printed.
async fn photograph(file: &str) -> Option<Picture> {
    let Some(root) = std::env::var_os(LIVE_DIR).map(PathBuf::from) else {
        println!("skipped: {LIVE_DIR} is not set");
        return None;
    };
    let path = root.join(file);

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

/// The application, installed under [`LIVE_DIR`], or `None` with the reason printed where Lisbon is not installed.
async fn application() -> Option<Opai> {
    let dir = kept_dir(NAME);

    for artifact in Depth::lisbon().required_artifacts() {
        let graph = dir.join("models").join(artifact.as_str()).join(format!("{artifact}.onnx"));
        if !graph.exists() {
            println!("skipped: {} is not installed", graph.display());
            return None;
        }
    }

    Some(live_application_at(NAME, &dir).await)
}

/// Estimates `photograph`'s depth on `provider` and checks the distances and the focal length.
async fn estimates(opai: &Opai, photograph: &Photograph, provider: ExecutionProvider) {
    let Some(picture) = self::photograph(photograph.file).await else {
        return;
    };
    let (width, height) = picture.dimensions();

    // Twice, uncached, so the second run's time is the run alone rather than the first's compile. The whole
    // `estimate_depth` call is timed, input, graph and recovery together, and in a test build rather than an optimised
    // one: longer than the graph run alone that `lisbon::profile` quotes.
    let mut timings = Vec::new();
    let mut map = None;
    for _ in 0..2 {
        let options = ExecuteOptions { provider, cache: false, ..Default::default() };
        let started = Instant::now();
        let executed = opai
            .estimate_depth(&picture, &Depth::lisbon(), Some(options))
            .await
            .unwrap_or_else(|err| panic!("{} on {provider}: {err}", photograph.file));
        timings.push(started.elapsed());

        assert_eq!(executed.providers.actual, vec![provider], "{} ran on another provider", photograph.file);
        map = Some(executed.value);
    }
    let map = map.expect("two runs");

    let (map_width, map_height) = map.dimensions();
    println!(
        "\n{} ({width}x{height}) on {provider}: first {:.2?}, second {:.2?}; map {map_width}x{map_height}, focal \
         {:.1} mm (EXIF {})",
        photograph.file,
        timings[0],
        timings[1],
        map.focal_35mm().unwrap_or(f32::NAN),
        photograph.focal_35mm.map_or_else(|| "not checked".to_string(), |focal| format!("{focal} mm"))
    );

    // The photograph's own aspect, at the graph's long side.
    assert_eq!(map_width.max(map_height), 1280);
    let aspect = f64::from(width) / f64::from(height);
    let map_aspect = f64::from(map_width) / f64::from(map_height);
    assert!(
        (aspect - map_aspect).abs() / aspect < 0.005,
        "a {width}x{height} photograph got a {map_width}x{map_height} map"
    );

    for &Point { name, at: (x, y), metres: (low, high) } in photograph.points {
        let distance = map.focus_metres(x, y);
        println!("  {name} at ({x}, {y}): {distance:.3} m");

        assert!((low..=high).contains(&distance), "{name} is {distance} m, outside {low} to {high} m");
    }

    let focal = map.focal_35mm().expect("a focal length");
    if let Some(exif) = photograph.focal_35mm {
        assert!(
            (focal - exif).abs() / exif <= 0.25,
            "the focal length came back as {focal} mm against EXIF's {exif} mm"
        );
    }
}

/// The three photographs on `provider`.
async fn on(provider: ExecutionProvider) {
    let Some(opai) = application().await else {
        return;
    };

    for photograph in [&BONSAI, &PORTRAIT, &STREET] {
        estimates(&opai, photograph, provider).await;
    }
}

#[tokio::test]
#[ignore = "needs Lisbon's graphs and the test photographs under OPAI_LIVE_DIR; run by hand with --ignored"]
async fn lisbon_measures_three_photographs_on_the_cpu() {
    on(ExecutionProvider::Cpu).await;
}

#[tokio::test]
#[ignore = "needs an Apple GPU, Lisbon's graphs and the photographs under OPAI_LIVE_DIR; run by hand with --ignored"]
async fn lisbon_measures_three_photographs_on_coreml() {
    on(ExecutionProvider::CoreMl).await;
}

#[tokio::test]
#[ignore = "a measurement over Lisbon's graphs and a test photograph under OPAI_LIVE_DIR; run by hand with --ignored"]
async fn the_bonsais_map_round_trips_through_the_run_cache() {
    // Design D5's measurement: what keeping a map in the store costs, written and read back, and its size. The read
    // is what a repeated estimate pays instead of the model, so it is the number that has to stay small.
    let (Some(opai), Some(picture)) = (application().await, photograph(BONSAI.file).await) else {
        return;
    };

    let computed = opai.estimate_depth(&picture, &Depth::lisbon(), None).await.expect("an estimate").value;

    let started = Instant::now();
    let bytes = serde_json::to_vec(&computed).expect("a map serializes");
    let written = started.elapsed();

    let started = Instant::now();
    let read: crate::models::DepthMap = serde_json::from_slice(&bytes).expect("a map deserializes");
    let decoded = started.elapsed();

    let started = Instant::now();
    let served = opai.estimate_depth(&picture, &Depth::lisbon(), None).await.expect("an estimate");
    let through_store = started.elapsed();

    println!(
        "\n{}x{} map: {} bytes of JSON, encoded in {written:.2?}, decoded in {decoded:.2?}; a served estimate took \
         {through_store:.2?} ({:?})",
        computed.dimensions().0,
        computed.dimensions().1,
        bytes.len(),
        served.providers.verdict()
    );

    assert_eq!(read, computed, "the map did not survive its serialized form");
    assert_eq!(served.value, computed, "the served map differs from the computed one");
}
