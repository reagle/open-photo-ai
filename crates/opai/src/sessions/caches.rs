//! Where what an execution provider compiles from a model is kept, and the two events that discard it.
//!
//! Slice 2 took the two directories as arguments and decided nothing about them; this is the slice that first
//! compiles an engine, so it is where they are. Under the configuration directory, beside `models/` and `runtime/`:
//!
//! ```text
//! <config>/engines/<artifact-id>/     what a provider compiled from that model
//! <config>/engines/.timing/           the one TensorRT timing cache, shared by every model
//! <config>/engines/.version           the runtime release those artifacts were compiled under
//! ```
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
//! Given a cache directory, ONNX Runtime's CoreML provider writes the MLProgram it translated from the graph as a
//! package, compiles it, and moves the compiled model in beside it — and deletes neither. Each holds a full copy of the
//! weights, byte for byte the same:
//!
//! ```text
//! <engine>/<hash>/<n>_static_mlprogram/model/Data/com.microsoft.OnnxRuntime/weights/weight.bin   the package's
//! <engine>/<hash>/<n>_static_mlprogram/model/compiled_model.mlmodelc/weights/weight.bin         the compiled one's
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

use crate::config;
use crate::deps::manifest;
use crate::deps::model::descriptor::{ENGINES_DIR, engine_dir};
use crate::error::InitError;
use crate::models::ArtifactId;
use crate::providers::options::CachePaths;

/// The installation-wide TensorRT timing cache, shared by every model.
///
/// Spelled out rather than composed from [`ENGINES_DIR`], because a `const` cannot format — the test below is what
/// keeps it inside the tree, which is the half of its placement that matters.
const TIMING_DIR: &str = "engines/.timing";

/// The record of which ONNX Runtime release the tree was compiled under.
///
/// A file rather than an inference from "the runtime was just replaced", because the latter is not durable: a process
/// that died between replacing the runtime and discarding the engines would find a matching runtime on its next
/// start, skip the reinstall, and never discard them.
const VERSION_STAMP: &str = ".version";

/// Resolves and creates the two directories the execution providers are pointed at for `id`.
///
/// Both are created on demand through [`config::sub_dir`], so the first session built for a model is what brings its
/// directory into existence and a machine that has never run one carries nothing.
///
/// # Errors
///
/// Returns [`InitError::InvalidName`] if `name` cannot be a directory, [`InitError::Fs`] if either path could escape
/// its parent, and [`InitError::Io`] if either could not be created.
pub(crate) fn resolve(app_dir: &Path, name: &str, id: &ArtifactId) -> Result<CachePaths, InitError> {
    Ok(CachePaths {
        engine: config::sub_dir(app_dir, name, &engine_dir(id))?,
        timing: config::sub_dir(app_dir, name, TIMING_DIR)?,
    })
}

/// The engines tree itself, resolved and created.
///
/// # Errors
///
/// As [`resolve`].
fn engines_root(app_dir: &Path, name: &str) -> Result<PathBuf, InitError> {
    config::sub_dir(app_dir, name, ENGINES_DIR)
}

/// Empties the engines tree where it was compiled under a runtime release other than `tag`, and records `tag`.
///
/// The wholesale invalidation of the two: what a provider compiles is valid only for the runtime that built it, and a
/// stale artifact is at best a wasted rebuild and at worst loaded against a runtime that cannot use it. It belongs to
/// initialization because that is where the runtime version in force is decided.
///
/// A launch whose stamp already names `tag` discards nothing, which is the point — these artifacts cost minutes to
/// rebuild. A machine that has never compiled anything writes the stamp over an empty directory and completes. No
/// model's weights are under this tree, so none is touched.
///
/// # Errors
///
/// Returns [`InitError::Io`] if the tree cannot be emptied or the stamp cannot be written, and otherwise as
/// [`resolve`].
pub(crate) fn invalidate_for_runtime(app_dir: &Path, name: &str, tag: &str) -> Result<(), InitError> {
    let engines = engines_root(app_dir, name)?;
    let stamp = engines.join(VERSION_STAMP);

    // Unreadable, absent and malformed all read the same way — as "compiled under something else" — which costs a
    // rebuild rather than trusting a stamp nothing wrote.
    let recorded = std::fs::read_to_string(&stamp).ok();
    if recorded.as_deref() == Some(tag) {
        return Ok(());
    }

    // Counted before the tree goes, and the stamp itself does not count: a launch that empties an empty tree — a
    // machine that has never compiled anything, or one whose stamp alone was stale — has discarded nothing and has
    // nothing to account for. The record exists for the minutes a rebuild costs, not for the bookkeeping.
    let discarded = std::fs::read_dir(&engines)
        .map(|entries| entries.flatten().filter(|entry| entry.file_name() != VERSION_STAMP).count())
        .unwrap_or_default();

    // The stamp lives in the tree it describes, so it goes with everything else and is written back afterwards. That
    // ordering is what makes an interruption in between read as "compiled under something else" on the next launch.
    manifest::empty_dir(&engines).map_err(InitError::io(&engines))?;

    if discarded > 0 {
        // `info`, naming the tag that replaced the one they were built under — which is the whole of the answer to
        // "why did this launch spend four minutes compiling engines it had already compiled".
        tracing::info!(
            discarded,
            tag,
            previous = recorded.as_deref().unwrap_or("unrecorded"),
            "discarded every compiled engine: the inference runtime they were built under has been replaced"
        );
    }

    std::fs::write(&stamp, tag).map_err(InitError::io(&stamp))
}

/// The package's copy of the weights, relative to an MLProgram's `model/` directory.
#[cfg(unix)]
const PACKAGE_WEIGHTS: &str = "Data/com.microsoft.OnnxRuntime/weights/weight.bin";

/// The compiled model's copy of the weights, relative to the same directory.
#[cfg(unix)]
const COMPILED_WEIGHTS: &str = "compiled_model.mlmodelc/weights/weight.bin";

/// Makes the package's copy of each CoreML MLProgram's weights under `engine` a hard link to the compiled model's, and
/// returns the bytes that freed.
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
/// directory. A TensorRT engine directory holds files at its top level and yields nothing.
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

    /// An application directory to resolve beneath, and the temporary root keeping it alive.
    fn app() -> (TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let app_dir = root.path().join(NAME);

        (root, app_dir)
    }

    #[test]
    fn two_artifacts_are_pointed_at_two_directories() {
        // What a provider compiled from one model's weights must never be attributed to or reused against another's,
        // which is a property of the directories rather than of anything a provider is asked to do.
        let (_root, app_dir) = app();

        let first = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let second = resolve(&app_dir, NAME, &tokyo()).unwrap();

        assert_ne!(first.engine, second.engine);
        assert_eq!(first.engine, app_dir.join(ENGINES_DIR).join(kyoto().as_str()));
        assert!(first.engine.is_dir() && second.engine.is_dir(), "a resolved directory was not created");
    }

    #[test]
    fn both_models_are_pointed_at_one_timing_cache_inside_the_engines_tree() {
        // Shared because measured kernel timings carry between graphs, and inside the tree because a runtime change
        // has to take it with everything else — a timing cache records tactics for one TensorRT version.
        let (_root, app_dir) = app();

        let first = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let second = resolve(&app_dir, NAME, &tokyo()).unwrap();

        assert_eq!(first.timing, second.timing);
        assert!(
            first.timing.starts_with(app_dir.join(ENGINES_DIR)),
            "{} escaped the tree",
            first.timing.display()
        );
        assert!(first.timing.is_dir(), "the timing directory was not created");
        assert!(TIMING_DIR.starts_with(&format!("{ENGINES_DIR}/")), "{TIMING_DIR} is not inside {ENGINES_DIR}");
    }

    #[test]
    fn resolving_a_second_time_creates_nothing_and_disturbs_nothing() {
        // The session builder resolves these on every build that misses the cache, so resolution has to be something
        // an engine compiled minutes ago survives.
        let (_root, app_dir) = app();
        let id = kyoto();

        let paths = resolve(&app_dir, NAME, &id).unwrap();
        let compiled = paths.engine.join("engine.bin");
        std::fs::write(&compiled, b"what a provider compiled").unwrap();

        let again = resolve(&app_dir, NAME, &id).unwrap();

        assert_eq!(again, paths);
        assert!(compiled.is_file(), "resolving a second time discarded what was compiled");
    }

    #[test]
    fn a_bumped_runtime_release_empties_the_whole_tree() {
        let (_root, app_dir) = app();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let engine = paths.engine.join("engine.bin");
        let timing = paths.timing.join("timings.bin");
        std::fs::write(&engine, b"compiled under the old runtime").unwrap();
        std::fs::write(&timing, b"measured under the old runtime").unwrap();

        invalidate_for_runtime(&app_dir, NAME, "runtime/1.26.0").unwrap();
        invalidate_for_runtime(&app_dir, NAME, "runtime/1.27.0").unwrap();

        assert!(!engine.exists(), "a compiled engine survived a runtime bump");
        assert!(!timing.exists(), "the shared timing cache survived a runtime bump");
        assert!(!paths.engine.exists(), "the model's directory was kept rather than discarded with the tree");
        let stamp = app_dir.join(ENGINES_DIR).join(VERSION_STAMP);
        assert_eq!(std::fs::read_to_string(stamp).unwrap(), "runtime/1.27.0");
    }

    #[test]
    fn a_launch_under_the_recorded_release_discards_nothing() {
        // The common case, and the one that must cost nothing: these artifacts take minutes to rebuild.
        let (_root, app_dir) = app();

        // Stamped first, then compiled: the order a machine actually reaches this state in, since the launch that
        // records the release precedes every session built under it.
        invalidate_for_runtime(&app_dir, NAME, "runtime/1.26.0").unwrap();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let engine = paths.engine.join("engine.bin");
        std::fs::write(&engine, b"compiled under the recorded runtime").unwrap();

        invalidate_for_runtime(&app_dir, NAME, "runtime/1.26.0").unwrap();

        assert!(engine.is_file(), "a launch with the same runtime discarded a compiled engine");
    }

    #[test]
    fn a_machine_that_has_never_compiled_anything_records_the_release_and_completes() {
        // A first run: there is no tree, no stamp and nothing to discard. Emptying a directory that is already empty
        // is what "absent reads as compiled under something else" costs here.
        let (_root, app_dir) = app();

        invalidate_for_runtime(&app_dir, NAME, "runtime/1.26.0").unwrap();

        let stamp = app_dir.join(ENGINES_DIR).join(VERSION_STAMP);
        assert_eq!(std::fs::read_to_string(stamp).unwrap(), "runtime/1.26.0");
    }

    #[test]
    fn discarding_the_compiled_artifacts_leaves_every_installed_model_in_place() {
        // The weights are under `models/`, which is why they are a different tree: a runtime bump costs a rebuild,
        // never a re-download.
        let (_root, app_dir) = app();
        let models = config::sub_dir(&app_dir, NAME, "models/up_kyoto_4x_fp32").unwrap();
        let weights = models.join("up_kyoto_4x_fp32.onnx");
        std::fs::write(&weights, b"the published graph").unwrap();
        resolve(&app_dir, NAME, &kyoto()).unwrap();

        invalidate_for_runtime(&app_dir, NAME, "runtime/1.27.0").unwrap();

        assert!(weights.is_file(), "a runtime bump removed an installed model");
    }

    #[test]
    fn a_bumped_runtime_release_says_what_it_discarded_and_what_replaced_it() {
        // Alongside the filesystem effect, not instead of it: the record exists because a user pays minutes for the
        // rebuild it causes, and nothing on screen says why.
        let (_root, app_dir) = app();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        std::fs::write(paths.engine.join("engine.bin"), b"compiled under the old runtime").unwrap();
        invalidate_for_runtime(&app_dir, NAME, "runtime/1.26.0").unwrap();
        resolve(&app_dir, NAME, &kyoto()).unwrap();
        std::fs::write(paths.engine.join("engine.bin"), b"compiled under the old runtime").unwrap();

        let (log, ()) = logging::records_of_blocking("info", || {
            invalidate_for_runtime(&app_dir, NAME, "runtime/1.27.0").unwrap();
        });

        let discarded = log
            .lines()
            .find(|line| line.contains("the inference runtime they were built under has been replaced"))
            .unwrap_or_else(|| panic!("the discard was not recorded:\n{log}"));

        assert!(discarded.contains("level=INFO"), "{discarded}");
        assert!(
            discarded.contains("tag=runtime/1.27.0"),
            "the record does not name what replaced it: {discarded}"
        );
        assert!(discarded.contains("previous=runtime/1.26.0"), "{discarded}");
        assert!(!paths.engine.exists(), "a compiled engine survived a runtime bump");
    }

    #[test]
    fn a_launch_that_discarded_nothing_writes_no_record() {
        // Two ways to reach an empty tree, and neither is work about to be redone: a machine that has never compiled
        // anything, and a launch under the release already recorded.
        let (_root, first) = app();
        let (fresh, ()) = logging::records_of_blocking("info", || {
            invalidate_for_runtime(&first, NAME, "runtime/1.26.0").unwrap();
        });
        assert!(
            !fresh.contains("has been replaced"),
            "a first launch claimed to have discarded engines:\n{fresh}"
        );

        let (_root, second) = app();
        invalidate_for_runtime(&second, NAME, "runtime/1.26.0").unwrap();
        let paths = resolve(&second, NAME, &kyoto()).unwrap();
        std::fs::write(paths.engine.join("engine.bin"), b"compiled under the recorded runtime").unwrap();

        let (unchanged, ()) = logging::records_of_blocking("info", || {
            invalidate_for_runtime(&second, NAME, "runtime/1.26.0").unwrap();
        });
        assert!(!unchanged.contains("has been replaced"), "{unchanged}");
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
        let (package, compiled) = mlprogram(&paths.engine, b"the weights", b"the weights");

        let freed = share_coreml_weights(&paths.engine);

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
        let (package, compiled) = mlprogram(&paths.engine, b"the weights", b"the weighty");

        assert_eq!(share_coreml_weights(&paths.engine), 0);
        assert_ne!(inode(&package), inode(&compiled));
        assert_eq!(std::fs::read(&package).unwrap(), b"the weights");
    }

    #[cfg(unix)]
    #[test]
    fn a_second_build_finds_the_copies_already_shared_and_frees_nothing() {
        // Every CoreML build runs this, cache hit or not, so the common case has to be a no-op.
        let (_root, app_dir) = app();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let (package, compiled) = mlprogram(&paths.engine, b"the weights", b"the weights");
        share_coreml_weights(&paths.engine);

        assert_eq!(share_coreml_weights(&paths.engine), 0);
        assert_eq!(inode(&package), inode(&compiled));
    }

    #[cfg(unix)]
    #[test]
    fn an_engine_directory_without_an_mlprogram_is_untouched() {
        // A TensorRT engine directory, and a CoreML one whose compile has not finished: only the one layout counts.
        let (_root, app_dir) = app();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        let engine = paths.engine.join("engine.bin");
        std::fs::write(&engine, b"what TensorRT compiled").unwrap();
        let (package, compiled) = mlprogram(&paths.engine, b"the weights", b"the weights");
        std::fs::remove_file(&compiled).unwrap();

        assert_eq!(share_coreml_weights(&paths.engine), 0);
        assert_eq!(std::fs::read(&engine).unwrap(), b"what TensorRT compiled");
        assert_eq!(std::fs::read(&package).unwrap(), b"the weights");
    }

    #[cfg(unix)]
    #[test]
    fn sharing_the_weights_says_how_much_it_freed() {
        let (_root, app_dir) = app();
        let paths = resolve(&app_dir, NAME, &kyoto()).unwrap();
        mlprogram(&paths.engine, b"the weights", b"the weights");

        let (log, _) = logging::records_of_blocking("info", || share_coreml_weights(&paths.engine));

        let shared = log
            .lines()
            .find(|line| line.contains("shared CoreML's two copies"))
            .unwrap_or_else(|| panic!("the sharing was not recorded:\n{log}"));
        assert!(shared.contains("freed=11"), "{shared}");
    }
}
