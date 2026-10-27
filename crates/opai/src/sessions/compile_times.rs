//! How long opening each model took the last time it was slow, kept so the next slow open can say how long it will be.
//!
//! Opening a model on CoreML or TensorRT compiles it the first time — Kazan's took three minutes on CoreML — and ONNX
//! Runtime reports nothing while it does. What *can* be said is how long the same compile took before, because the
//! compile is repeated exactly when the engine directory it wrote is discarded: a model's weights were replaced, or
//! the runtime that built it was. Same graph, same provider, same machine — so the last one is the best estimate going.
//!
//! ```text
//! <config>/compile-times.json      { "schema": 1, "opens": { "<artifact-id>": { "<provider>": <milliseconds> } } }
//! ```
//!
//! **Beside `engines/`, not inside it**, and that is the whole of the placement: both of the engine tree's
//! invalidations — one model's directory at install, the whole tree at a runtime change — are precisely the events
//! after which the number is wanted, and a record kept inside what they delete would be gone each time it mattered.
//!
//! Keyed by artifact and provider because those are what a compiled engine is a function of: the artifact names the
//! graph, its native scale and its precision, and the provider is what it was compiled for. A CPU open is recorded
//! like any other where it is slow, since it is the same wait to estimate.
//!
//! The **last** time rather than an average: what changes the cost of a compile is the machine — a new OS, a new
//! runtime — and the newest measurement is the one taken under those. Only a slow open is recorded, which is what
//! keeps a cache hit from replacing the compile it was a hit on; see [`RECORDED_FROM`].
//!
//! Nothing here is ever an error to a caller. A file that is missing, unreadable, malformed or written under another
//! schema is no estimate, and one that cannot be written is a `debug` record and the same next time: the estimate is
//! an embellishment on a wait that happens regardless.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::models::ArtifactId;
use crate::providers::ExecutionProvider;
use crate::task::lock;

/// The record's name, under the application directory.
const FILE_NAME: &str = "compile-times.json";

/// The layout this module writes. A record under any other is read as no record, and replaced by the next write.
const SCHEMA: u32 = 1;

// The same three seconds the window waits before it says a model is being optimized
// (`PREPARING_NOTICE_DELAY_MS`), and for the same reason seen from the other side. Below it an open is a cache hit —
// Kazan's compiled CoreML model opens in 0.7 s — and recording one would replace the minutes the compile took with a
// second, so the next compile would promise "about 1 minute" and take three. At or above it the window has said a
// compile is happening, and that is the wait the number is for.
/// How long an open has to have taken to be remembered.
pub(crate) const RECORDED_FROM: Duration = Duration::from_secs(3);

/// The record on disk.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Record {
    /// [`SCHEMA`], when this module wrote it.
    schema: u32,
    /// Per artifact, per provider (as [`ExecutionProvider::as_str`] spells it), the milliseconds the last slow open
    /// took. Sorted maps so the file reads the same however the entries were added.
    opens: BTreeMap<String, BTreeMap<String, u64>>,
}

// One process can be finishing two compiles at once — a diffusion upscaler opens several graphs, and two windows can
// each be opening a model — and each write reads the record, adds its entry and puts the whole back. Serialized here,
// so the second does not put back a record without the first's entry. Two *processes* (the GUI and the CLI) can still
// race, and the loser's entry is lost: one estimate, recorded again the next time that model compiles.
/// Held across each read-modify-write of the record.
static WRITING: Mutex<()> = Mutex::new(());

/// Where the record of the application at `app_dir` lives.
fn path(app_dir: &Path) -> PathBuf {
    app_dir.join(FILE_NAME)
}

/// The record as it stands, or an empty one where there is none this module can read.
fn read(app_dir: &Path) -> Record {
    std::fs::read(path(app_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Record>(&bytes).ok())
        .filter(|record| record.schema == SCHEMA)
        .unwrap_or_default()
}

/// How long opening `artifact` on `provider` took the last time it was slow, or `None` where it has not been.
pub(crate) fn expected(app_dir: &Path, artifact: &ArtifactId, provider: ExecutionProvider) -> Option<Duration> {
    read(app_dir)
        .opens
        .get(artifact.as_str())?
        .get(provider.as_str())
        .copied()
        .map(Duration::from_millis)
}

/// Remembers that opening `artifact` on `provider` took `took`, where that is long enough to be a compile.
///
/// Best effort, and silent apart from a `debug` record: see the module's documentation.
pub(crate) fn record(app_dir: &Path, artifact: &ArtifactId, provider: ExecutionProvider, took: Duration) {
    if took < RECORDED_FROM {
        return;
    }

    if let Err(error) = write(app_dir, artifact, provider, took) {
        tracing::debug!(%artifact, %provider, %error, "could not remember how long opening this model took");
    }
}

/// [`record`]'s read-modify-write, atomically replacing the file.
fn write(app_dir: &Path, artifact: &ArtifactId, provider: ExecutionProvider, took: Duration) -> std::io::Result<()> {
    let _writing = lock(&WRITING);

    let mut record = read(app_dir);
    record.schema = SCHEMA;
    let millis = u64::try_from(took.as_millis()).unwrap_or(u64::MAX);
    record
        .opens
        .entry(artifact.as_str().to_string())
        .or_default()
        .insert(provider.as_str().to_string(), millis);

    let json = serde_json::to_vec_pretty(&record).map_err(std::io::Error::other)?;
    // Beside the target, so the rename stays on one filesystem and is atomic: a reader sees the old record or the new
    // one, never half of either. Named for the process, so the GUI and the CLI writing at once do not write into one
    // temporary file.
    let temporary = app_dir.join(format!("{FILE_NAME}.{}.tmp", std::process::id()));
    std::fs::write(&temporary, json)?;
    std::fs::rename(&temporary, path(app_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::test_support::{kyoto, tokyo};

    /// Three minutes, as a first CoreML compile of Kazan took.
    const COMPILE: Duration = Duration::from_secs(186);

    #[test]
    fn a_slow_open_is_read_back_as_the_expected_time_of_the_next() {
        let dir = tempfile::tempdir().unwrap();

        record(dir.path(), &kyoto(), ExecutionProvider::CoreMl, COMPILE);

        assert_eq!(expected(dir.path(), &kyoto(), ExecutionProvider::CoreMl), Some(COMPILE));
    }

    #[test]
    fn the_last_slow_open_replaces_the_one_before_rather_than_being_averaged_with_it() {
        let dir = tempfile::tempdir().unwrap();

        record(dir.path(), &kyoto(), ExecutionProvider::CoreMl, COMPILE);
        record(dir.path(), &kyoto(), ExecutionProvider::CoreMl, Duration::from_secs(240));

        assert_eq!(expected(dir.path(), &kyoto(), ExecutionProvider::CoreMl), Some(Duration::from_secs(240)));
    }

    #[test]
    fn a_fast_open_is_a_cache_hit_and_leaves_the_compile_it_was_a_hit_on_alone() {
        let dir = tempfile::tempdir().unwrap();

        record(dir.path(), &kyoto(), ExecutionProvider::CoreMl, COMPILE);
        record(dir.path(), &kyoto(), ExecutionProvider::CoreMl, Duration::from_millis(700));
        record(dir.path(), &tokyo(), ExecutionProvider::CoreMl, RECORDED_FROM - Duration::from_millis(1));

        assert_eq!(expected(dir.path(), &kyoto(), ExecutionProvider::CoreMl), Some(COMPILE));
        assert_eq!(expected(dir.path(), &tokyo(), ExecutionProvider::CoreMl), None, "a fast open was recorded");
    }

    #[test]
    fn an_open_of_exactly_the_threshold_is_recorded() {
        let dir = tempfile::tempdir().unwrap();

        record(dir.path(), &kyoto(), ExecutionProvider::TensorRt, RECORDED_FROM);

        assert_eq!(expected(dir.path(), &kyoto(), ExecutionProvider::TensorRt), Some(RECORDED_FROM));
    }

    #[test]
    fn each_model_and_each_provider_is_remembered_apart() {
        // An engine is compiled from one graph for one provider, so neither half of the key can stand in for the other.
        let dir = tempfile::tempdir().unwrap();

        record(dir.path(), &kyoto(), ExecutionProvider::CoreMl, COMPILE);
        record(dir.path(), &kyoto(), ExecutionProvider::Cpu, Duration::from_secs(4));
        record(dir.path(), &tokyo(), ExecutionProvider::CoreMl, Duration::from_secs(60));

        assert_eq!(expected(dir.path(), &kyoto(), ExecutionProvider::CoreMl), Some(COMPILE));
        assert_eq!(expected(dir.path(), &kyoto(), ExecutionProvider::Cpu), Some(Duration::from_secs(4)));
        assert_eq!(expected(dir.path(), &tokyo(), ExecutionProvider::CoreMl), Some(Duration::from_secs(60)));
        assert_eq!(expected(dir.path(), &tokyo(), ExecutionProvider::TensorRt), None);
    }

    #[test]
    fn no_record_is_no_estimate() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(expected(dir.path(), &kyoto(), ExecutionProvider::CoreMl), None);
    }

    #[test]
    fn a_corrupt_record_is_no_estimate_and_is_replaced_by_the_next_slow_open() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(path(dir.path()), b"{ \"schema\": 1, \"opens\": { trunc").unwrap();

        assert_eq!(expected(dir.path(), &kyoto(), ExecutionProvider::CoreMl), None);

        record(dir.path(), &tokyo(), ExecutionProvider::CoreMl, COMPILE);
        assert_eq!(expected(dir.path(), &tokyo(), ExecutionProvider::CoreMl), Some(COMPILE));
    }

    #[test]
    fn a_record_written_under_another_schema_is_no_estimate() {
        let dir = tempfile::tempdir().unwrap();
        let other = format!(r#"{{ "schema": 99, "opens": {{ "{}": {{ "CoreML": 186000 }} }} }}"#, kyoto().as_str());
        std::fs::write(path(dir.path()), other).unwrap();

        assert_eq!(expected(dir.path(), &kyoto(), ExecutionProvider::CoreMl), None);
    }

    #[test]
    fn the_record_is_kept_outside_the_engines_tree_that_a_recompile_empties() {
        // Both invalidations delete under `engines/`, and both are exactly when a compile is about to be repeated.
        let dir = tempfile::tempdir().unwrap();

        record(dir.path(), &kyoto(), ExecutionProvider::CoreMl, COMPILE);

        let written = path(dir.path());
        assert!(written.is_file());
        assert!(!written.starts_with(dir.path().join(crate::deps::model::descriptor::ENGINES_DIR)));
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "a temporary file was left behind");
    }
}
