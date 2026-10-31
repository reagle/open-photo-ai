//! Where what an execution provider compiles from a model is kept, and the two events that discard it.
//!
//! Under the configuration directory, beside `models/`:
//!
//! ```text
//! <config>/engines/<artifact-id>/tensorrt/   the TensorRT engine compiled from that model
//! <config>/engines/<artifact-id>/coreml/     the CoreML model compiled from it
//! <config>/engines/.timing/                  the one TensorRT timing cache, shared by every model
//! <config>/engines/.version                  the runtime release and the layout those were written under
//! ```
//!
//! One folder per provider inside a model's own: each provider owns its directory, so one can be cleared or inspected
//! without touching the other, and two providers' file names can never collide. `rust-sak` creates a provider's folder
//! when a rung that attaches it is tried, so a model only ever run on the CPU leaves none behind.
//!
//! Not inside `models/<artifact-id>/`, which was the first idea and does not survive contact with the install
//! pipeline: it finishes by recording **every file under the directory** as something it downloaded, and a
//! replacement removes exactly the files the *previous* record named — so an engine compiled after that record was
//! written would survive the one replacement that must not leave it.
//!
//! `.timing` sits **inside** `engines/` rather than beside it, which is what gives it the right lifetime at both
//! ends: replacing one model's weights clears that model's subdirectory and the shared cache survives, while a
//! runtime change empties the tree wholesale and takes the timing cache with it — which is correct, because a timing
//! cache records tactics for one TensorRT version and TensorRT comes from the runtime.
//!
//! One function resolves a model's engine directory and every caller goes through it: the descriptor that records it
//! as derived from the weights, the installer that clears it on a replacement, and the session builder that points a
//! provider at it. They cannot disagree about which directory it is. That function lives beside `model_dir` in
//! `deps/model/descriptor.rs` rather than here, because `deps/` needs it too and sits below this layer — the layout
//! decisions above are still this module's.
//!
//! # CoreML keeps its weights twice, and one copy is made a hard link of the other
//!
//! Given its folder, `coreml/` above, ONNX Runtime's CoreML provider writes the MLProgram it translated from the graph
//! as a package, compiles it, and moves the compiled model in beside it — and deletes neither. Each holds a full copy
//! of the weights, byte for byte the same:
//!
//! ```text
//! <coreml>/<hash>/<n>_static_mlprogram/model/Data/com.microsoft.OnnxRuntime/weights/weight.bin   the package's
//! <coreml>/<hash>/<n>_static_mlprogram/model/compiled_model.mlmodelc/weights/weight.bin         the compiled one's
//! ```
//!
//! So the tree is twice the size it has to be — 13.4 GB rather than 6.7 GB for Osaka at FP16. As of 1.30.0 a cache hit
//! only asks that `model/` exist and then loads `compiled_model.mlmodelc`; the package is never read again. Deleting
//! it would therefore work, but it would also make a later runtime that *did* read it fail on a file that is not
//! there. [`share_coreml_weights`] instead replaces the package's copy with a hard link to the compiled one, once both
//! are proven identical: every file the runtime wrote stays where it wrote it, and the space is held once. Nothing
//! writes either file after the compile, so the two names sharing one inode is never observable.
//!
//! This is a workaround for the runtime and is meant to be removed once a pinned release stops keeping both.

use std::path::{Path, PathBuf};

use rust_sak::ai::CachePaths;

use crate::config;
use crate::deps::model::paths::{ENGINES_DIR, engine_dir};
use crate::error::InitError;
use crate::models::ArtifactId;

/// The installation-wide TensorRT timing cache, shared by every model.
///
/// Spelled out rather than composed from [`ENGINES_DIR`], because a `const` cannot format — the test below is what
/// keeps it inside the tree, which is the half of its placement that matters.
const TIMING_DIR: &str = "engines/.timing";

/// The record of which ONNX Runtime release, and which layout, the tree was written under.
///
/// A file rather than an inference from "the runtime was just replaced", because the latter is not durable: a process
/// that died between replacing the runtime and discarding the engines would find a matching runtime on its next
/// start, skip the reinstall, and never discard them.
const VERSION_STAMP: &str = ".version";

/// The layout this build writes the tree in, as the stamp records it after the runtime's version.
///
/// `2` is a folder per provider inside each model's. A stamp naming no layout is the flat `engines/<artifact-id>/` of
/// earlier releases, which nothing reads any more. A later layout change is a bump here.
const LAYOUT: &str = "layout=2";

/// The stamp a tree written under `runtime_version` in this build's layout carries.
fn stamp_for(runtime_version: &str) -> String {
    format!("{runtime_version};{LAYOUT}")
}

/// The folders the execution providers are pointed at for `id`: its TensorRT and CoreML folders inside its own, and
/// the timing cache every model shares. Computes the paths only; see the module documentation for who creates them.
///
/// # Errors
///
/// Returns [`InitError::InvalidName`] if `name` cannot be a directory, [`InitError::Fs`] if the tree could escape its
/// parent, and [`InitError::Io`] if the tree itself could not be created.
pub(crate) fn resolve(app_dir: &Path, name: &str, id: &ArtifactId) -> Result<CachePaths, InitError> {
    // The tree is validated and created through the one rule every configuration path goes through; what is inside it
    // is named by an artifact id, which cannot escape it.
    engines_root(app_dir, name)?;

    Ok(CachePaths { timing: app_dir.join(TIMING_DIR), ..CachePaths::under(&app_dir.join(engine_dir(id))) })
}

/// The engines tree itself, resolved and created.
///
/// # Errors
///
/// As [`resolve`].
fn engines_root(app_dir: &Path, name: &str) -> Result<PathBuf, InitError> {
    config::sub_dir(app_dir, name, ENGINES_DIR)
}

/// Empties the engines tree where it was written under a runtime release other than `runtime_version`, or in a layout
/// other than this build's, and records both.
///
/// The wholesale invalidation of the two: what a provider compiles is valid only for the runtime that built it, and a
/// stale artifact is at best a wasted rebuild and at worst loaded against a runtime that cannot use it. Artifacts in a
/// layout nothing reads any more would never be reused and never removed by anything else. It belongs to
/// initialization because that is where the runtime version in force is decided.
///
/// A launch whose stamp already names both discards nothing, which is the point — these artifacts cost minutes to
/// rebuild. A machine that has never compiled anything writes the stamp over an empty directory and completes. No
/// model's weights are under this tree, so none is touched.
///
/// # Errors
///
/// Returns [`InitError::Io`] if the tree cannot be emptied or the stamp cannot be written, and otherwise as
/// [`resolve`].
pub(crate) fn invalidate_for_runtime(app_dir: &Path, name: &str, runtime_version: &str) -> Result<(), InitError> {
    let engines = engines_root(app_dir, name)?;
    let stamp = engines.join(VERSION_STAMP);
    let expected = stamp_for(runtime_version);

    // Unreadable, absent and malformed all read the same way — as "written under something else" — which costs a
    // rebuild rather than trusting a stamp nothing wrote. A stamp naming no layout is an earlier release's.
    let recorded = std::fs::read_to_string(&stamp).ok();
    if recorded.as_deref() == Some(expected.as_str()) {
        return Ok(());
    }

    // Counted before the tree goes, and the stamp itself does not count: a launch that empties an empty tree — a
    // machine that has never compiled anything, or one whose stamp alone was stale — has discarded nothing and has
    // nothing to account for. The record exists for the minutes a rebuild costs, not for the bookkeeping.
    let discarded = std::fs::read_dir(&engines)
        .map(|entries| entries.flatten().filter(|entry| entry.file_name() != VERSION_STAMP).count())
        .unwrap_or_default();

    // The stamp lives in the tree it describes, so it goes with everything else and is written back afterwards. That
    // ordering is what makes an interruption in between read as "written under something else" on the next launch.
    empty_dir(&engines).map_err(InitError::io(&engines))?;

    if discarded > 0 {
        // `info`, naming what replaced what they were built under — which is the whole of the answer to "why did this
        // launch spend four minutes compiling engines it had already compiled".
        tracing::info!(
            discarded,
            stamp = %expected,
            previous = recorded.as_deref().unwrap_or("unrecorded"),
            "discarded every compiled engine: the inference runtime they were built under, or the layout they were \
             kept in, has changed"
        );
    }

    std::fs::write(&stamp, &expected).map_err(InitError::io(&stamp))
}

/// Deletes everything in `dir`, keeping `dir` itself. `file_type` comes from the entry and does not follow links, so a
/// link to a directory is unlinked rather than having its target's contents removed.
fn empty_dir(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }

    Ok(())
}

/// The package's copy of the weights, relative to an MLProgram's `model/` directory.
#[cfg(unix)]
const PACKAGE_WEIGHTS: &str = "Data/com.microsoft.OnnxRuntime/weights/weight.bin";

/// The compiled model's copy of the weights, relative to the same directory.
#[cfg(unix)]
const COMPILED_WEIGHTS: &str = "compiled_model.mlmodelc/weights/weight.bin";

/// Makes the package's copy of each CoreML MLProgram's weights under `engine`, a model's CoreML folder, a hard link to
/// the compiled model's, and returns the bytes that freed.
///
/// Called after every session build that attached CoreML, cache hit or not, so an engine compiled before this existed
/// is shared the next time its model is opened. A pair already sharing an inode costs two `stat` calls; the first time
/// a pair is seen, both files are read once to prove them identical, and a pair that differs in any byte is left
/// alone.
///
/// Never an error: the build it follows has already succeeded, and a copy left unshared costs disk space and nothing
/// else. A failure is logged at `warn` with the file it concerns.
#[cfg(unix)]
pub(crate) fn share_coreml_weights(engine: &Path) -> u64 {
    let mut freed = 0;

    for model in mlprogram_models(engine) {
        let (compiled, package) = (model.join(COMPILED_WEIGHTS), model.join(PACKAGE_WEIGHTS));

        match link_if_identical(&compiled, &package) {
            Ok(bytes) => freed += bytes,
            Err(error) => tracing::warn!(
                path = %package.display(),
                %error,
                "could not share CoreML's two copies of a model's weights; both are kept"
            ),
        }
    }

    if freed > 0 {
        tracing::info!(freed, "shared CoreML's two copies of a model's weights through a hard link");
    }

    freed
}

/// Every `<hash>/<subgraph>/model` directory under `engine`: the one layout the CoreML provider writes beneath a cache
/// directory. A file at the folder's top level yields nothing.
#[cfg(unix)]
fn mlprogram_models(engine: &Path) -> Vec<PathBuf> {
    let directories = |parent: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(parent)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect()
    };

    directories(engine)
        .iter()
        .flat_map(|hash| directories(hash))
        .map(|subgraph| subgraph.join("model"))
        .filter(|model| model.is_dir())
        .collect()
}

/// Replaces `package` with a hard link to `compiled` where both are there, are not already one file, and hold the same
/// bytes; returns the size freed, which is zero where nothing was replaced.
///
/// The link is made under a temporary name and renamed over `package`, so an interruption leaves either the copy or
/// the link, never a missing file.
#[cfg(unix)]
fn link_if_identical(compiled: &Path, package: &Path) -> std::io::Result<u64> {
    use std::os::unix::fs::MetadataExt;

    let (Ok(kept), Ok(copy)) = (std::fs::metadata(compiled), std::fs::metadata(package)) else {
        return Ok(0);
    };
    let shared = (kept.dev(), kept.ino()) == (copy.dev(), copy.ino());
    if shared || kept.len() != copy.len() || kept.len() == 0 || !same_bytes(compiled, package)? {
        return Ok(0);
    }

    let staged = package.with_file_name("weight.bin.linking");
    // A leftover from an interrupted run would make `hard_link` fail; it is ours, so it goes.
    if staged.exists() {
        std::fs::remove_file(&staged)?;
    }
    std::fs::hard_link(compiled, &staged)?;
    std::fs::rename(&staged, package).inspect_err(|_| {
        let _ = std::fs::remove_file(&staged);
    })?;

    Ok(kept.len())
}

/// Whether two files of the same length hold the same bytes, read a mebibyte at a time.
#[cfg(unix)]
fn same_bytes(first: &Path, second: &Path) -> std::io::Result<bool> {
    const CHUNK: usize = 1 << 20;

    let (mut first, mut second) = (std::fs::File::open(first)?, std::fs::File::open(second)?);
    let (mut left, mut right) = (vec![0; CHUNK], vec![0; CHUNK]);

    loop {
        let read = fill(&mut first, &mut left)?;
        if read != fill(&mut second, &mut right)? || left[..read] != right[..read] {
            return Ok(false);
        }
        if read < CHUNK {
            return Ok(true);
        }
    }
}

/// Reads into `buffer` until it is full or the file ends, and returns how much was read.
#[cfg(unix)]
fn fill(file: &mut std::fs::File, buffer: &mut [u8]) -> std::io::Result<usize> {
    use std::io::{ErrorKind, Read};

    let mut read = 0;
    while read < buffer.len() {
        match file.read(&mut buffer[read..]) {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }

    Ok(read)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging;
    use crate::models::test_support::{kyoto, tokyo};
    use tempfile::TempDir;

    /// The application name and directory every test here resolves against.
    const NAME: &str = "opai-test";

    /// The runtime version every test here is stamped under.
    const RUNTIME: &str = "1.30.0";

    /// An application directory to resolve beneath, and the temporary root keeping it alive.
    fn app() -> (TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let app_dir = root.path().join(NAME);

        (root, app_dir)
    }

    /// `paths` with something compiled into each provider's folder and into the shared timing cache, as a machine that
    /// ran the model on both would have them. Returns the files written.
    fn compile_into(paths: &CachePaths) -> [PathBuf; 3] {
        let files = [
            paths.tensorrt.join("engine.bin"),
            paths.coreml.join("model.mlmodelc"),
            paths.timing.join("t.cache"),
        ];
        for file in &files {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, b"compiled").unwrap();
        }
        files
    }

    /// What the stamp says now.
    fn stamp(app_dir: &Path) -> String {
        std::fs::read_to_string(app_dir.join(ENGINES_DIR).join(VERSION_STAMP)).unwrap()
    }

    #[test]
    fn each_provider_compiles_into_a_folder_of_its_own_inside_the_models() {
        let (_root, app_dir) = app();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let model = app_dir.join(ENGINES_DIR).join(kyoto().as_str());

        assert_eq!(paths.tensorrt, model.join("tensorrt"));
        assert_eq!(paths.coreml, model.join("coreml"));
        assert_ne!(paths.tensorrt, paths.coreml);
        assert!(paths.tensorrt.starts_with(&model) && paths.coreml.starts_with(&model));
    }

    #[test]
    fn two_artifacts_are_pointed_at_two_directories() {
        // What a provider compiled from one model's weights must never be attributed to or reused against another's,
        // which is a property of the directories rather than of anything a provider is asked to do.
        let (_root, app_dir) = app();

        let first = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let second = resolve(&app_dir, NAME, &tokyo()).unwrap();

        assert_ne!(first.tensorrt, second.tensorrt);
        assert_ne!(first.coreml, second.coreml);
    }

    #[test]
    fn both_models_are_pointed_at_one_timing_cache_inside_the_engines_tree_and_outside_either_model() {
        // Shared because measured kernel timings carry between graphs, and inside the tree because a runtime change
        // has to take it with everything else — a timing cache records tactics for one TensorRT version.
        let (_root, app_dir) = app();

        let first = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let second = resolve(&app_dir, NAME, &tokyo()).unwrap();

        assert_eq!(first.timing, second.timing);
        assert_eq!(first.timing, app_dir.join(TIMING_DIR));
        assert!(
            first.timing.starts_with(app_dir.join(ENGINES_DIR)),
            "{} escaped the tree",
            first.timing.display()
        );
        for model in [kyoto(), tokyo()] {
            let own = app_dir.join(ENGINES_DIR).join(model.as_str());
            assert!(!first.timing.starts_with(&own), "the timing cache is inside {}", own.display());
        }
        assert!(TIMING_DIR.starts_with(&format!("{ENGINES_DIR}/")), "{TIMING_DIR} is not inside {ENGINES_DIR}");
    }

    #[test]
    fn resolving_creates_no_provider_folder_and_disturbs_nothing_compiled() {
        // The provider folders are `rust-sak`'s to create when a rung that compiles is tried, so a model only run on
        // the CPU leaves nothing; and the session builder resolves on every build that misses the cache, so
        // resolution has to be something an engine compiled minutes ago survives.
        let (_root, app_dir) = app();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        assert!(!paths.tensorrt.exists() && !paths.coreml.exists() && !paths.timing.exists());

        let compiled = compile_into(&paths);
        assert_eq!(resolve(&app_dir, NAME, &kyoto()).unwrap(), paths);
        assert!(
            compiled.iter().all(|file| file.is_file()),
            "resolving a second time discarded what was compiled"
        );
    }

    #[test]
    fn a_bumped_runtime_release_empties_the_whole_tree() {
        let (_root, app_dir) = app();
        invalidate_for_runtime(&app_dir, NAME, "1.26.0").unwrap();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let compiled = compile_into(&paths);

        invalidate_for_runtime(&app_dir, NAME, "1.27.0").unwrap();

        assert!(compiled.iter().all(|file| !file.exists()), "something compiled survived a runtime bump");
        assert!(!app_dir.join(engine_dir(&kyoto())).exists(), "the model's directory was kept");
        assert_eq!(stamp(&app_dir), "1.27.0;layout=2");
    }

    #[test]
    fn a_stamp_naming_no_layout_discards_the_flat_engines_once() {
        // The stamp an earlier release wrote: the runtime's version alone, over engines kept flat in each model's
        // folder. Same runtime, so only the layout differs.
        let (_root, app_dir) = app();
        let flat = app_dir.join(engine_dir(&kyoto())).join("engine.bin");
        std::fs::create_dir_all(flat.parent().unwrap()).unwrap();
        std::fs::write(&flat, b"compiled in the flat layout").unwrap();
        let timing = app_dir.join(TIMING_DIR).join("t.cache");
        std::fs::create_dir_all(timing.parent().unwrap()).unwrap();
        std::fs::write(&timing, b"measured").unwrap();
        std::fs::write(app_dir.join(ENGINES_DIR).join(VERSION_STAMP), RUNTIME).unwrap();

        invalidate_for_runtime(&app_dir, NAME, RUNTIME).unwrap();
        assert!(!flat.exists(), "an engine in the flat layout survived");
        assert!(!timing.exists(), "the shared timing cache survived a layout change");
        assert_eq!(stamp(&app_dir), "1.30.0;layout=2");

        let compiled = compile_into(&resolve(&app_dir, NAME, &kyoto()).unwrap());
        invalidate_for_runtime(&app_dir, NAME, RUNTIME).unwrap();
        assert!(compiled.iter().all(|file| file.is_file()), "the second launch discarded again");
    }

    #[test]
    fn a_stamp_naming_another_layout_empties_the_whole_tree() {
        // Same runtime, a layout this build does not write: what a later layout bump, or a rollback, finds.
        let (_root, app_dir) = app();
        let compiled = compile_into(&resolve(&app_dir, NAME, &kyoto()).unwrap());
        std::fs::write(app_dir.join(ENGINES_DIR).join(VERSION_STAMP), format!("{RUNTIME};layout=1")).unwrap();

        invalidate_for_runtime(&app_dir, NAME, RUNTIME).unwrap();

        assert!(compiled.iter().all(|file| !file.exists()), "something compiled survived a layout change");
        assert_eq!(stamp(&app_dir), "1.30.0;layout=2");
    }

    #[test]
    fn a_launch_under_the_recorded_release_and_layout_discards_nothing() {
        // The common case, and the one that must cost nothing: these artifacts take minutes to rebuild.
        let (_root, app_dir) = app();
        invalidate_for_runtime(&app_dir, NAME, RUNTIME).unwrap();
        let compiled = compile_into(&resolve(&app_dir, NAME, &kyoto()).unwrap());

        invalidate_for_runtime(&app_dir, NAME, RUNTIME).unwrap();

        assert!(compiled.iter().all(|file| file.is_file()), "a launch with nothing changed discarded something");
    }

    #[test]
    fn a_machine_that_has_never_compiled_anything_records_the_release_and_layout_and_completes() {
        let (_root, app_dir) = app();

        invalidate_for_runtime(&app_dir, NAME, RUNTIME).unwrap();

        assert_eq!(stamp(&app_dir), "1.30.0;layout=2");
    }

    #[test]
    fn discarding_the_compiled_artifacts_leaves_every_installed_model_in_place() {
        // The weights are under `models/`, which is why they are a different tree: a runtime bump or a layout change
        // costs a rebuild, never a re-download.
        let (_root, app_dir) = app();
        let models = config::sub_dir(&app_dir, NAME, "models/up_kyoto_4x_fp32").unwrap();
        let weights = models.join("up_kyoto_4x_fp32.onnx");
        std::fs::write(&weights, b"the published graph").unwrap();
        compile_into(&resolve(&app_dir, NAME, &kyoto()).unwrap());

        std::fs::write(app_dir.join(ENGINES_DIR).join(VERSION_STAMP), RUNTIME).unwrap();
        invalidate_for_runtime(&app_dir, NAME, RUNTIME).unwrap();
        invalidate_for_runtime(&app_dir, NAME, "1.31.0").unwrap();

        assert!(weights.is_file(), "discarding the engines removed an installed model");
    }

    #[test]
    fn a_discard_says_what_it_discarded_and_what_replaced_it() {
        // Alongside the filesystem effect, not instead of it: the record exists because a user pays minutes for the
        // rebuild it causes, and nothing on screen says why.
        let (_root, app_dir) = app();
        compile_into(&resolve(&app_dir, NAME, &kyoto()).unwrap());
        std::fs::write(app_dir.join(ENGINES_DIR).join(VERSION_STAMP), RUNTIME).unwrap();

        let (log, ()) = logging::records_of_blocking("info", || {
            invalidate_for_runtime(&app_dir, NAME, RUNTIME).unwrap();
        });

        let discarded = log
            .lines()
            .find(|line| line.contains("discarded every compiled engine"))
            .unwrap_or_else(|| panic!("the discard was not recorded:\n{log}"));

        assert!(discarded.contains("level=INFO"), "{discarded}");
        assert!(discarded.contains("or the layout they were kept in, has changed"), "{discarded}");
        assert!(
            discarded.contains(r#"stamp="1.30.0;layout=2""#),
            "the record does not name what replaced it: {discarded}"
        );
        assert!(discarded.contains("previous=1.30.0"), "{discarded}");
    }

    #[test]
    fn a_launch_that_discarded_nothing_writes_no_record() {
        // Two ways to reach an empty tree, and neither is work about to be redone: a machine that has never compiled
        // anything, and a launch under the stamp already recorded.
        let (_root, first) = app();
        let (fresh, ()) = logging::records_of_blocking("info", || {
            invalidate_for_runtime(&first, NAME, RUNTIME).unwrap();
        });
        assert!(!fresh.contains("discarded every compiled engine"), "a first launch claimed a discard:\n{fresh}");

        let (_root, second) = app();
        invalidate_for_runtime(&second, NAME, RUNTIME).unwrap();
        compile_into(&resolve(&second, NAME, &kyoto()).unwrap());

        let (unchanged, ()) = logging::records_of_blocking("info", || {
            invalidate_for_runtime(&second, NAME, RUNTIME).unwrap();
        });
        assert!(!unchanged.contains("discarded every compiled engine"), "{unchanged}");
    }

    /// Lays out one MLProgram the way the CoreML provider leaves it under `engine`, with `package` and `compiled` as
    /// the two copies of its weights, and returns their paths in that order.
    #[cfg(unix)]
    fn mlprogram(engine: &Path, package: &[u8], compiled: &[u8]) -> (PathBuf, PathBuf) {
        let model = engine.join("13628133443189788236/0_static_mlprogram/model");
        let (package_path, compiled_path) = (model.join(PACKAGE_WEIGHTS), model.join(COMPILED_WEIGHTS));
        for (path, bytes) in [(&package_path, package), (&compiled_path, compiled)] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }

        (package_path, compiled_path)
    }

    #[cfg(unix)]
    fn inode(path: &Path) -> u64 {
        use std::os::unix::fs::MetadataExt;

        std::fs::metadata(path).unwrap().ino()
    }

    #[cfg(unix)]
    #[test]
    fn two_identical_copies_of_the_weights_become_one_file_under_both_names() {
        let (_root, app_dir) = app();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let (package, compiled) = mlprogram(&paths.coreml, b"the weights", b"the weights");

        let freed = share_coreml_weights(&paths.coreml);

        assert_eq!(freed, 11);
        assert_eq!(inode(&package), inode(&compiled), "the two names are still two files");
        assert_eq!(std::fs::read(&package).unwrap(), b"the weights", "the package's weights changed");
        assert!(!package.with_file_name("weight.bin.linking").exists(), "the staging name was left behind");
    }

    #[cfg(unix)]
    #[test]
    fn copies_that_differ_are_left_alone_even_at_the_same_length() {
        // Proven identical byte for byte, not assumed from the length: replacing a copy that differs would hand a
        // runtime that reads the package weights it never wrote.
        let (_root, app_dir) = app();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let (package, compiled) = mlprogram(&paths.coreml, b"the weights", b"the weighty");

        assert_eq!(share_coreml_weights(&paths.coreml), 0);
        assert_ne!(inode(&package), inode(&compiled));
        assert_eq!(std::fs::read(&package).unwrap(), b"the weights");
    }

    #[cfg(unix)]
    #[test]
    fn a_second_build_finds_the_copies_already_shared_and_frees_nothing() {
        // Every CoreML build runs this, cache hit or not, so the common case has to be a no-op.
        let (_root, app_dir) = app();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let (package, compiled) = mlprogram(&paths.coreml, b"the weights", b"the weights");
        share_coreml_weights(&paths.coreml);

        assert_eq!(share_coreml_weights(&paths.coreml), 0);
        assert_eq!(inode(&package), inode(&compiled));
    }

    #[cfg(unix)]
    #[test]
    fn an_engine_directory_without_an_mlprogram_is_untouched() {
        // A stray file at the folder's top level, and a compile that has not finished: only the one layout counts.
        let (_root, app_dir) = app();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        std::fs::create_dir_all(&paths.coreml).unwrap();
        let engine = paths.coreml.join("engine.bin");
        std::fs::write(&engine, b"what TensorRT compiled").unwrap();
        let (package, compiled) = mlprogram(&paths.coreml, b"the weights", b"the weights");
        std::fs::remove_file(&compiled).unwrap();

        assert_eq!(share_coreml_weights(&paths.coreml), 0);
        assert_eq!(std::fs::read(&engine).unwrap(), b"what TensorRT compiled");
        assert_eq!(std::fs::read(&package).unwrap(), b"the weights");
    }

    #[cfg(unix)]
    #[test]
    fn sharing_the_weights_says_how_much_it_freed() {
        let (_root, app_dir) = app();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        mlprogram(&paths.coreml, b"the weights", b"the weights");

        let (log, _) = logging::records_of_blocking("info", || share_coreml_weights(&paths.coreml));

        let shared = log
            .lines()
            .find(|line| line.contains("shared CoreML's two copies"))
            .unwrap_or_else(|| panic!("the sharing was not recorded:\n{log}"));
        assert!(shared.contains("freed=11"), "{shared}");
    }
}
