//! What the command line asks the GUI to do with the photographs `opai --gui` hands it, beyond opening them: today, a
//! lens blur on each, with Autopilot off.
//!
//! Why a file and not arguments: on macOS a running application receives a handover as an "open documents" Apple Event,
//! which carries file paths only; arguments reach it only on a fresh launch. A custom URL scheme could carry them, but
//! would need the deep-link plugin and would let any web page drive the application. So the command line writes the
//! rest to [`FILE`] with [`write_at`] just before the handover, and the GUI claims it with [`claim_at`] when the paths
//! arrive: only for a batch it names and only while fresh ([`FRESH_FOR`]), so a lost request cannot attach itself to a
//! photograph opened later from the Finder. A claimed or stale request is deleted.
//!
//! The format is versioned ([`VERSION`]); a file of another version, or one that cannot be read, is ignored and
//! deleted. Claiming never fails a caller.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The file's name inside the configuration directory, beside `gui-settings.json`.
pub const FILE: &str = "open-request.json";

/// The version of the format [`write_at`] writes and [`claim_at`] accepts.
pub const VERSION: u64 = 1;

/// How long after it was written a request may still be claimed: far longer than a handover takes, even one that has
/// to launch the application first, and short enough that a forgotten one does not linger.
pub const FRESH_FOR: Duration = Duration::from_secs(120);

/// What to do with the photographs handed over, besides opening them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenRequest {
    /// Always [`VERSION`] when written by this build.
    pub version: u64,
    /// When it was written, in seconds since the Unix epoch.
    pub written_at: u64,
    /// The photographs it is for, as the command line resolved them.
    pub paths: Vec<PathBuf>,
    /// The lens blur to add to each.
    pub lens_blur: LensBlurRequest,
}

/// A lens blur as `--lens-blur` asks for it. What is absent is the GUI's own default for a freshly added one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LensBlurRequest {
    /// The f-number, `--lb-f`.
    pub f_number: Option<f64>,
    /// The blur amount from 0 to 100, `--lb-amount`; read against each photograph's own lens, as the slider reads it.
    pub amount: Option<f64>,
    /// The focus point as fractions of the width and height, `--lb-focus`.
    pub focus: Option<(f64, f64)>,
}

impl OpenRequest {
    /// A request for `paths`, written now.
    pub fn new(paths: Vec<PathBuf>, lens_blur: LensBlurRequest) -> Self {
        Self { version: VERSION, written_at: seconds_since_epoch(SystemTime::now()), paths, lens_blur }
    }
}

/// Where the file is, under the configuration directory of the application `name`. Creates nothing.
///
/// # Errors
///
/// Where the platform has no configuration directory, or `name` is not a usable directory name.
pub fn path(name: &str) -> Result<PathBuf, rust_sak::fs::FsError> {
    rust_sak::fs::user_config_dir(name, FILE)
}

/// Writes `request` to `path`, creating the directory if missing.
///
/// # Errors
///
/// Where the directory or the file could not be written.
pub fn write_at(path: &Path, request: &OpenRequest) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(path, serde_json::to_vec_pretty(request)?)
}

/// The request at `path` for the batch `opened`, as of `now`, or `None`.
///
/// Claimed where it is fresh and names at least one of `opened`; its paths are then the ones of `opened` it names, in
/// the spelling the handover delivered them in, which is the one the GUI knows the photographs by. The two are compared
/// as files, so a path the command line wrote through a symbolic link still matches. A claimed request is deleted, and
/// so is one that is stale, unreadable or of another version; a fresh one naming none of `opened` is left for its own
/// batch.
pub fn claim_at(path: &Path, opened: &[PathBuf], now: SystemTime) -> Option<OpenRequest> {
    let raw = match std::fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "the open request could not be read");
            return None;
        }
    };

    let discard = |reason: &str| {
        tracing::warn!(path = %path.display(), "ignoring the open request: {reason}");
        remove(path);
        None
    };

    let mut request = match serde_json::from_slice::<OpenRequest>(&raw) {
        Ok(request) if request.version == VERSION => request,
        Ok(request) => return discard(&format!("it is version {}, and this build reads {VERSION}", request.version)),
        Err(error) => return discard(&format!("it is not a request: {error}")),
    };

    let age = seconds_since_epoch(now).saturating_sub(request.written_at);
    if age > FRESH_FOR.as_secs() {
        return discard(&format!("it was written {age} s ago"));
    }

    let asked: Vec<PathBuf> = request.paths.iter().map(|path| identity(path)).collect();
    let named: Vec<PathBuf> = opened.iter().filter(|path| asked.contains(&identity(path))).cloned().collect();
    if named.is_empty() {
        return None;
    }

    remove(path);
    request.paths = named;
    Some(request)
}

/// The file `path` names, for comparing two spellings of it: its canonical form, or itself where that cannot be found.
fn identity(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Deletes the request, which is only ever claimed once; a failure is recorded and otherwise harmless, since a stale
/// request is never claimed.
fn remove(path: &Path) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(path = %path.display(), %error, "the open request could not be deleted");
    }
}

fn seconds_since_epoch(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let photo = dir.path().join("a.jpg");
        std::fs::write(&photo, b"").expect("written");
        let file = dir.path().join("config").join(FILE);
        (dir, file, photo)
    }

    fn blur() -> LensBlurRequest {
        LensBlurRequest { f_number: Some(1.4), amount: None, focus: Some((0.3, 0.6)) }
    }

    #[test]
    fn a_fresh_request_is_claimed_once_by_the_batch_it_names() {
        let (_dir, file, photo) = fixture();
        write_at(&file, &OpenRequest::new(vec![photo.clone()], blur())).expect("written");

        let claimed = claim_at(&file, std::slice::from_ref(&photo), SystemTime::now()).expect("claimed");

        assert_eq!((claimed.paths, claimed.lens_blur), (vec![photo.clone()], blur()));
        assert!(!file.exists(), "a claimed request is deleted");
        assert_eq!(claim_at(&file, &[photo], SystemTime::now()), None);
    }

    #[test]
    fn a_batch_it_does_not_name_leaves_it_for_its_own() {
        let (dir, file, photo) = fixture();
        write_at(&file, &OpenRequest::new(vec![photo.clone()], blur())).expect("written");

        assert_eq!(claim_at(&file, &[dir.path().join("other.jpg")], SystemTime::now()), None);
        assert!(file.exists());
        assert!(claim_at(&file, &[photo], SystemTime::now()).is_some());
    }

    #[test]
    fn only_the_named_photographs_of_a_batch_are_its_paths_in_the_spelling_delivered() {
        let (dir, file, photo) = fixture();
        let other = dir.path().join("other.jpg");
        let spelled = dir.path().join(".").join("a.jpg");
        write_at(&file, &OpenRequest::new(vec![spelled], blur())).expect("written");

        let claimed = claim_at(&file, &[other, photo.clone()], SystemTime::now()).expect("claimed");

        assert_eq!(claimed.paths, [photo]);
    }

    #[test]
    fn a_stale_or_foreign_request_is_deleted_unclaimed() {
        let (_dir, file, photo) = fixture();
        let later = SystemTime::now() + FRESH_FOR + Duration::from_secs(5);

        write_at(&file, &OpenRequest::new(vec![photo.clone()], blur())).expect("written");
        assert_eq!(claim_at(&file, std::slice::from_ref(&photo), later), None);
        assert!(!file.exists());

        let future = OpenRequest { version: VERSION + 1, ..OpenRequest::new(vec![photo.clone()], blur()) };
        write_at(&file, &future).expect("written");
        assert_eq!(claim_at(&file, std::slice::from_ref(&photo), SystemTime::now()), None);
        assert!(!file.exists());

        std::fs::write(&file, b"not json").expect("written");
        assert_eq!(claim_at(&file, &[photo], SystemTime::now()), None);
        assert!(!file.exists());
    }

    #[test]
    fn no_file_is_no_request() {
        let (_dir, file, photo) = fixture();

        assert_eq!(claim_at(&file, &[photo], SystemTime::now()), None);
    }

    #[test]
    fn what_a_lens_blur_leaves_out_is_absent() {
        let request: LensBlurRequest = serde_json::from_str("{}").expect("parses");

        assert_eq!(request, LensBlurRequest::default());
    }
}
