//! The live check: Lima's real graph, the real ONNX Runtime, a real photograph, end to end.
//!
//! `#[ignore]`d, as every live check in this crate is, and for two reasons of its own: the graph is 210 MB and exported
//! by the fork, and the photograph it is checked against is not committed. Run it by hand with [`LIVE_DIR`] naming a
//! directory that holds the photograph and the installation:
//!
//! ```text
//! $OPAI_LIVE_DIR/bonsai.jpg                         a 5:4 landscape, a copper tag hanging below the canopy
//! $OPAI_LIVE_DIR/opai-live-remove/models/...        rm_lima_fp32 and rm_lima_1024_fp32, installed
//!                                                   (install_lima.py EXPORT APP_DIR)
//!
//! cargo test -p opai models::remove::live -- --ignored --nocapture --test-threads=1
//! ```
//!
//! A check whose photograph or graph is missing says so and passes, rather than failing a sweep that has neither.

// What the fake-backend suite cannot say: that the graph reads the packed tensor as LaMa reads its two inputs (RGB in
// [0, 1], the hole as 1), that its output is in [0, 1], and that a real fill replaces the tag rather than echoing it.
// Each of those wrong gives a picture and no diagnostic: a hole of the wrong polarity fills the whole crop but the tag,
// a range off by 255 a white or black patch, and either is caught by the two numbers checked here.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use rust_sak::image::ImageFormat;

use crate::live_support::{LIVE_DIR, kept_dir, live_application_at};
use crate::mask::{Mask, Stroke};
use crate::models::Remove;
use crate::models::precision::FloatPrecision;
use crate::models::remove::RemoveVariant;
use crate::providers::ExecutionProvider;
use crate::{Opai, Picture, ProcessOptions};
use imaging::tensor::Sampler;

/// The application name, and the directory under [`LIVE_DIR`] the graph is installed in.
const NAME: &str = "opai-live-remove";

/// The tag and its string, as a stroke down the frame: x 0.637, y 0.60 to 0.735, 0.0136 of the longer side in radius
/// (about 130 px across on the 4796 x 3926 original).
fn tag() -> Arc<Mask> {
    Arc::new(Mask {
        strokes: vec![Stroke::new(false, 0.0136, 0.0, vec![0.637, 0.60, 0.637, 0.735])],
        ..Mask::default()
    })
}

/// The photograph under [`LIVE_DIR`], or `None` with the reason printed.
async fn photograph() -> Option<Picture> {
    let Some(root) = std::env::var_os(LIVE_DIR).map(PathBuf::from) else {
        println!("skipped: {LIVE_DIR} is not set");
        return None;
    };
    let path = root.join("bonsai.jpg");

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

/// The application, installed under [`LIVE_DIR`], or `None` with the reason printed where Lima is not installed.
async fn application() -> Option<Opai> {
    let dir = kept_dir(NAME);
    let removal = Remove::new(RemoveVariant::Lima(FloatPrecision::Fp32), tag());

    for artifact in removal.required_artifacts() {
        let graph = dir.join("models").join(artifact.as_str()).join(format!("{artifact}.onnx"));

        if !graph.exists() {
            println!("skipped: {} is not installed", graph.display());
            return None;
        }
    }

    Some(live_application_at(NAME, &dir).await)
}

/// The mean absolute difference between `left` and `right` over the pixels `inside` selects, in 8-bit levels.
fn mean_difference(left: &Picture, right: &Picture, inside: impl Fn(u32, u32) -> bool) -> f64 {
    let (width, height) = left.dimensions();
    let (left, right) = (Sampler::new(left.pixels()), Sampler::new(right.pixels()));
    let (mut total, mut count) = (0.0, 0_u64);

    for y in 0..height {
        for x in (0..width).filter(|x| inside(*x, y)) {
            let (a, b) = (left.rgb(x, y), right.rgb(x, y));
            total += a.iter().zip(b).map(|(a, b)| f64::from(a.abs_diff(b))).sum::<f64>() / 3.0 / 257.0;
            count += 1;
        }
    }

    total / count.max(1) as f64
}

/// Removes the tag on `provider`, twice uncached so the second time is the run alone, and checks the fill.
async fn removes_on(opai: &Opai, source: &Picture, provider: ExecutionProvider) {
    let (width, height) = source.dimensions();
    let operation = Remove::lima(tag());
    let mut enhanced = None;

    for attempt in ["first", "again"] {
        let options = ProcessOptions { provider, cache: false, ..Default::default() };
        let started = Instant::now();
        let run = opai.process(source, std::slice::from_ref(&operation), Some(options)).await.unwrap();

        println!("  {provider}, {attempt}: {:?} on {:?}", started.elapsed(), run.providers.actual);
        enhanced = Some(run);
    }

    let produced = enhanced.expect("two runs").picture;
    assert_eq!(produced.dimensions(), (width, height));

    // Inside the stroke the tag is gone: the copper and its shadow are replaced by paving, tens of levels away. Far
    // outside it nothing has changed at all.
    let (cx, top, bottom) = (0.637 * width as f64, 0.60 * height as f64, 0.735 * height as f64);
    let radius = 0.0136 * f64::from(width.max(height));
    let core = |x: u32, y: u32| {
        let (x, y) = (f64::from(x), f64::from(y));
        (x - cx).abs() < radius * 0.6 && (top..bottom).contains(&y)
    };
    // The stroke is a capsule: its round caps reach a radius past its end points, and the hole 16 px past that.
    let reach = radius + 24.0;
    let far =
        |x: u32, y: u32| (f64::from(x) - cx).abs() > reach || !(top - reach..bottom + reach).contains(&f64::from(y));

    let (changed, kept) = (mean_difference(&produced, source, core), mean_difference(&produced, source, far));
    println!("  inside the tag {changed:.1} levels from the photograph, outside it {kept:.3}");

    assert!(changed > 8.0, "the fill echoes the tag: {changed:.1} levels");
    assert_eq!(kept, 0.0, "pixels far from the tag changed");

    let keep = kept_dir(NAME).join(format!("removed-{provider}.png"));
    crate::image::save(produced.shared_pixels(), produced.shared_metadata(), &keep, ImageFormat::Png, None)
        .await
        .expect("the result encodes");
    println!("  inspect at {}", keep.display());
}

#[tokio::test]
#[ignore = "needs Lima installed and the bonsai photograph under OPAI_LIVE_DIR"]
async fn lima_removes_the_tag_and_leaves_the_rest_on_the_cpu_and_under_auto() {
    let (Some(source), Some(opai)) = (photograph().await, application().await) else { return };
    println!("\nbonsai {}x{}", source.dimensions().0, source.dimensions().1);

    removes_on(&opai, &source, ExecutionProvider::Cpu).await;
    removes_on(&opai, &source, ExecutionProvider::Auto).await;
}

#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "needs Lima installed and the bonsai photograph under OPAI_LIVE_DIR; compiles Core ML once"]
async fn lima_removes_the_tag_under_core_ml() {
    let (Some(source), Some(opai)) = (photograph().await, application().await) else { return };

    removes_on(&opai, &source, ExecutionProvider::CoreMl).await;
}

/// The environment variable naming the bench's jobs: a file of lines `photograph<TAB>mask<TAB>output`, white in the
/// mask removed, the result written as PNG.
const BENCH_JOBS: &str = "OPAI_REMOVE_BENCH";

/// The environment variable naming what the bench runs: `small` (the small square alone, as before 2026-10-31),
/// `large` (the large square for large crops), and either with `+grain`.
const BENCH_FINISH: &str = "OPAI_REMOVE_FINISH";

/// The finish `name` names, as [`BENCH_FINISH`] spells it.
fn finish_named(name: &str) -> super::process::Finish {
    let mut parts = name.split('+');
    let large = match parts.next() {
        Some("small") => false,
        Some("large") => true,
        other => panic!("{BENCH_FINISH}={name}: {other:?} is neither small nor large"),
    };
    let mut finish = super::process::Finish { large, grain: false };

    for part in parts {
        match part {
            "grain" => finish.grain = true,
            other => panic!("{BENCH_FINISH}={name}: {other} is not a finish"),
        }
    }

    finish
}

/// The fork's Remove bench (`remove/README.md` in its notes): each job removed on the CPU, uncached, under the finish
/// [`BENCH_FINISH`] names, with the seconds the removal took printed as `bench<TAB>output<TAB>seconds`. The first job is
/// run once more before the rest and not reported, so every reported time is warm.
#[tokio::test]
#[ignore = "the fork's bench: needs OPAI_REMOVE_BENCH, OPAI_REMOVE_FINISH and Lima installed under OPAI_LIVE_DIR"]
async fn bench() {
    let (Some(jobs), Some(finish)) = (std::env::var_os(BENCH_JOBS), std::env::var(BENCH_FINISH).ok()) else {
        println!("skipped: {BENCH_JOBS} or {BENCH_FINISH} is not set");
        return;
    };
    let Some(opai) = application().await else { return };
    *super::process::BENCH_FINISH.lock().unwrap() = Some(finish_named(&finish));

    let jobs = std::fs::read_to_string(&jobs).expect("the bench's jobs are readable");
    let jobs: Vec<Vec<&str>> = jobs.lines().filter(|line| !line.is_empty()).map(|line| line.split('\t').collect()).collect();

    for (index, job) in std::iter::once(&jobs[0]).chain(&jobs).enumerate() {
        let [photograph, mask, output] = job[..] else { panic!("a job is three paths: {job:?}") };
        let source = crate::image::load(std::path::Path::new(photograph)).await.expect("the photograph decodes");
        let mask = crate::image::load(std::path::Path::new(mask)).await.expect("the mask decodes");
        let operation = Remove::lima(Arc::new(Mask::from_image(mask.pixels())));
        let options = ProcessOptions { provider: ExecutionProvider::Cpu, cache: false, ..Default::default() };

        let started = Instant::now();
        let run = opai.process(&source, std::slice::from_ref(&operation), Some(options)).await.unwrap();
        let seconds = started.elapsed().as_secs_f64();

        if index == 0 {
            continue;
        }

        let path = PathBuf::from(output);
        crate::image::save(run.picture.shared_pixels(), run.picture.shared_metadata(), &path, ImageFormat::Png, None)
            .await
            .expect("the result encodes");
        println!("bench\t{output}\t{seconds:.3}");
    }

    *super::process::BENCH_FINISH.lock().unwrap() = None;
}
