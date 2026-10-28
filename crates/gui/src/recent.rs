//! The photographs opened most recently, for File > Open Recent.
//!
//! **Rust owns the list, the file and the menu; the window owns what opening means.** The window tells [`note_recent`]
//! which photographs entered its gallery - whatever opened them, so every route counts alike - and opens the one chosen
//! through the route a drop takes. Here the list is kept as application state, written to [`FILE`] on every change and
//! shown in the macOS menu bar by `crate::menu::show_recent`; every change is also published to the window as
//! [`CHANGED`], for the Open Recent button Windows and Linux have in place of a menu bar.
//!
//! The file is the application's state file, not only the list's: a later slice keeps the last session in it too, so it
//! is versioned, read leniently and named for what it will hold.

// Read once, at launch, rather than at each use: the menu bar is built from it before the page exists, and nothing else
// writes the file while the application runs.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::command::{Traceparent, command_span, traced_sync};
use crate::sync::lock;

/// The state file's name, beside `gui-settings.json` in the application's configuration directory.
pub(crate) const FILE: &str = "session.json";

/// The one version of [`FILE`] this build reads and writes.
const VERSION: u32 = 1;

/// How many photographs the list holds.
pub(crate) const LIMIT: usize = 10;

/// The event every change of the list is published as, with the paths, newest first, as its payload.
///
/// Written once more in `frontend/ipc/recent.ts`.
pub(crate) const CHANGED: &str = "recent-changed";

/// One photograph the list remembers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Recent {
    /// Where it was opened from, as the window spells it.
    pub(crate) path: String,
    /// Its content identity when it was last opened - the window's `ImageRecord.identity` - or absent for one whose
    /// bytes could not be read. Kept for the session slice, which checks a saved stack against it; nothing here reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) identity: Option<String>,
}

/// The photographs opened most recently: newest first, one entry per path, at most [`LIMIT`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RecentList(Vec<Recent>);

impl RecentList {
    /// Puts `batch` at the front in its own order - its first is the one the window made current - removing any earlier
    /// entry at the same path and keeping the first [`LIMIT`]. Answers whether anything changed.
    fn note(&mut self, batch: Vec<Recent>) -> bool {
        // De-duplicated within the batch too, the first mention winning, as the window's own `addFiles` does.
        let mut front: Vec<Recent> = Vec::with_capacity(batch.len());
        for entry in batch {
            if !front.iter().any(|kept| kept.path == entry.path) {
                front.push(entry);
            }
        }

        let rest = self.0.iter().filter(|old| !front.iter().any(|new| new.path == old.path)).cloned();
        let next: Vec<Recent> = front.iter().cloned().chain(rest).take(LIMIT).collect();

        let changed = next != self.0;
        self.0 = next;
        changed
    }

    /// Drops the entry at `path`. Answers whether there was one.
    fn remove(&mut self, path: &str) -> bool {
        let before = self.0.len();
        self.0.retain(|entry| entry.path != path);
        self.0.len() != before
    }

    /// Empties the list. Answers whether it held anything.
    fn clear(&mut self) -> bool {
        !std::mem::take(&mut self.0).is_empty()
    }

    /// The paths, newest first.
    pub(crate) fn paths(&self) -> Vec<String> {
        self.0.iter().map(|entry| entry.path.clone()).collect()
    }
}

/// [`FILE`]'s contents. Members it does not name - a later slice's - are ignored on read.
#[derive(Debug, Serialize, Deserialize)]
struct Stored {
    version: u32,
    #[serde(default)]
    recent: Vec<Recent>,
}

/// The list saved at `path`. Never fails: a file that is absent is an empty list, and one that cannot be read, is not
/// the JSON this writes or is of another version is an empty list too, with a warning naming why. It is replaced at
/// the next change - an unreadable list is worth less than a launch.
pub(crate) fn read_at(path: &Path) -> RecentList {
    let raw = match std::fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return RecentList::default(),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "the state file could not be read; starting with no recent images");
            return RecentList::default();
        }
    };

    match serde_json::from_slice::<Stored>(&raw) {
        Ok(stored) if stored.version == VERSION => {
            let mut list = RecentList::default();
            // Through `note`, so a hand-edited file with repeats or too many entries is read as one this would write.
            list.note(stored.recent);
            list
        }
        Ok(stored) => {
            tracing::warn!(path = %path.display(), version = stored.version, expected = VERSION, "the state file is of another version; starting with no recent images");
            RecentList::default()
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "the state file is not valid; starting with no recent images");
            RecentList::default()
        }
    }
}

/// Writes `list` to `path`, answering whether the file changed.
///
/// Through a temporary file beside it renamed over it, as `gui-settings.json` is written, so a reader sees the old list
/// or the new one and never a torn file. A write that would change nothing is skipped; the directory is created if
/// missing.
fn write_at(path: &Path, list: &RecentList) -> Result<bool, std::io::Error> {
    let bytes = serde_json::to_vec_pretty(&Stored { version: VERSION, recent: list.0.clone() })?;

    if std::fs::read(path).is_ok_and(|existing| existing == bytes) {
        return Ok(false);
    }

    let directory = path.parent().ok_or_else(|| std::io::Error::other("the state file's path has no directory"))?;
    std::fs::create_dir_all(directory)?;

    let mut temporary = rust_sak::fs::mk_temp_file_in(directory, ".session-").map_err(std::io::Error::other)?;
    temporary.write_all(&bytes)?;
    temporary.flush()?;
    temporary.sync_all()?;
    temporary.into_inner().persist(path).map_err(|error| error.error)?;

    Ok(true)
}

/// The list, and where it is kept. Registered once, at launch, by [`Recents::load`].
#[derive(Debug, Default)]
pub(crate) struct Recents {
    /// `None` where the platform has no configuration directory: the list then lives for the session only.
    file: Option<PathBuf>,
    list: Mutex<RecentList>,
}

impl Recents {
    /// The list saved in the application's configuration directory, or an empty one kept in memory where there is no
    /// such directory.
    pub(crate) fn load() -> Self {
        match rust_sak::fs::user_config_dir(opai::APP_NAME, FILE) {
            Ok(file) => Self { list: Mutex::new(read_at(&file)), file: Some(file) },
            Err(error) => {
                tracing::warn!(%error, "the state file's location could not be determined; recent images are not kept");
                Self::default()
            }
        }
    }

    /// The paths, newest first.
    pub(crate) fn paths(&self) -> Vec<String> {
        lock(&self.list).paths()
    }

    /// The path `index` entries from the top, if the list is that long.
    pub(crate) fn path_at(&self, index: usize) -> Option<String> {
        lock(&self.list).0.get(index).map(|entry| entry.path.clone())
    }

    /// Applies `change` to the list and, where it changed anything, answers the paths to show; the file is written
    /// before the lock is let go, so two changes never write out of order.
    fn update(&self, change: impl FnOnce(&mut RecentList) -> bool) -> Option<Vec<String>> {
        let mut list = lock(&self.list);
        if !change(&mut list) {
            return None;
        }

        if let Some(file) = &self.file
            && let Err(error) = write_at(file, &list)
        {
            // The list in memory is still right, and the menu shows it; only the next launch will not.
            tracing::warn!(path = %file.display(), %error, "the recent images could not be saved");
        }

        Some(list.paths())
    }
}

/// Applies `change` and, where it changed the list, shows the new one in the menu bar and tells the window.
fn changed<R: Runtime>(app: &AppHandle<R>, change: impl FnOnce(&mut RecentList) -> bool) {
    let Some(paths) = app.state::<Recents>().update(change) else {
        return;
    };

    crate::menu::show_recent(app, &paths);

    if let Err(error) = app.emit(CHANGED, &paths) {
        tracing::warn!(%error, "could not tell the window the recent images changed");
    }
}

/// Empties the list: Clear Menu, from the macOS menu bar or the window's button.
pub(crate) fn clear<R: Runtime>(app: &AppHandle<R>) {
    changed(app, RecentList::clear);
}

// The commands' names are written once more, in `frontend/ipc/recent.ts`.
/// The photographs opened most recently, newest first: what the window's Open Recent button lists at mount.
#[tauri::command]
pub(crate) fn recent_images(recents: State<'_, Recents>, traceparent: Traceparent) -> Vec<String> {
    traced_sync(command_span!("recent_images", traceparent), || recents.paths())
}

/// Puts `images`, which have just entered the window's gallery, at the top of the list, in their order.
#[tauri::command]
pub(crate) fn note_recent<R: Runtime>(app: AppHandle<R>, images: Vec<Recent>, traceparent: Traceparent) {
    traced_sync(command_span!("note_recent", traceparent), || changed(&app, |list| list.note(images)));
}

/// Whether the photograph at `path` is still there to open. One that is not leaves the list, which is shown again.
#[tauri::command]
pub(crate) fn check_recent<R: Runtime>(app: AppHandle<R>, path: String, traceparent: Traceparent) -> bool {
    traced_sync(command_span!("check_recent", traceparent), || {
        let present = Path::new(&path).is_file();
        if !present {
            tracing::info!(path, "a recent image is no longer there; dropped from the list");
            changed(&app, |list| list.remove(&path));
        }
        present
    })
}

/// Empties the list, for the window's Open Recent button; the macOS menu's Clear Menu is acted on here directly.
#[tauri::command]
pub(crate) fn clear_recent<R: Runtime>(app: AppHandle<R>, traceparent: Traceparent) {
    traced_sync(command_span!("clear_recent", traceparent), || clear(&app));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recent(path: &str) -> Recent {
        Recent { path: path.to_owned(), identity: Some(format!("id-{path}")) }
    }

    fn list(paths: &[&str]) -> RecentList {
        let mut list = RecentList::default();
        list.note(paths.iter().map(|path| recent(path)).collect());
        list
    }

    fn file() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("opai").join(FILE);
        (dir, path)
    }

    #[test]
    fn a_batch_goes_to_the_top_in_its_own_order_and_an_earlier_entry_at_its_path_leaves() {
        let mut recent_list = list(&["/c.jpg", "/b.jpg", "/a.jpg"]);

        assert!(recent_list.note(vec![recent("/x.jpg"), recent("/a.jpg")]));

        assert_eq!(recent_list.paths(), ["/x.jpg", "/a.jpg", "/c.jpg", "/b.jpg"]);
    }

    #[test]
    fn the_list_keeps_the_ten_newest_the_last_opened_first() {
        let mut recent_list = RecentList::default();
        for n in 1..=12 {
            recent_list.note(vec![recent(&format!("/{n}.jpg"))]);
        }

        let expected: Vec<String> = (3..=12).rev().map(|n| format!("/{n}.jpg")).collect();
        assert_eq!(recent_list.paths(), expected);
    }

    #[test]
    fn opening_the_top_entry_again_changes_nothing_and_a_repeat_within_a_batch_is_listed_once() {
        let mut recent_list = list(&["/a.jpg", "/b.jpg"]);

        assert!(!recent_list.note(vec![recent("/a.jpg")]), "already first");
        assert!(!recent_list.note(Vec::new()), "an empty batch");

        recent_list.note(vec![recent("/c.jpg"), recent("/c.jpg")]);
        assert_eq!(recent_list.paths(), ["/c.jpg", "/a.jpg", "/b.jpg"]);
    }

    #[test]
    fn a_newer_identity_at_the_same_path_replaces_the_older() {
        let mut recent_list = list(&["/a.jpg"]);

        assert!(recent_list.note(vec![Recent { path: "/a.jpg".to_owned(), identity: Some("other".to_owned()) }]));

        assert_eq!(recent_list.0[0].identity.as_deref(), Some("other"));
    }

    #[test]
    fn removing_and_clearing_answer_whether_anything_went() {
        let mut recent_list = list(&["/a.jpg", "/b.jpg"]);

        assert!(recent_list.remove("/a.jpg"));
        assert!(!recent_list.remove("/a.jpg"));
        assert_eq!(recent_list.paths(), ["/b.jpg"]);

        assert!(recent_list.clear());
        assert!(!recent_list.clear());
    }

    #[test]
    fn the_list_written_is_the_list_read_back_identity_included() {
        let (_dir, path) = file();
        let written = list(&["/a.jpg", "/b.jpg"]);

        assert!(write_at(&path, &written).expect("writable"));
        assert!(!write_at(&path, &written).expect("writable"), "an unchanged list is not written again");

        assert_eq!(read_at(&path), written);
        let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).expect("readable")).expect("JSON");
        assert_eq!(raw["version"], 1);
        assert_eq!(raw["recent"][0], serde_json::json!({ "path": "/a.jpg", "identity": "id-/a.jpg" }));
    }

    #[test]
    fn a_missing_file_is_an_empty_list() {
        let (_dir, path) = file();

        assert_eq!(read_at(&path), RecentList::default());
    }

    #[test]
    fn a_corrupt_file_is_an_empty_list_and_is_replaced_at_the_next_change() {
        let (_dir, path) = file();
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("creatable");
        std::fs::write(&path, b"{ \"version\": 1, \"recent\": [ { \"pa").expect("writable");

        assert_eq!(read_at(&path), RecentList::default());

        write_at(&path, &list(&["/a.jpg"])).expect("writable");
        assert_eq!(read_at(&path).paths(), ["/a.jpg"]);
    }

    #[test]
    fn a_file_of_another_version_is_an_empty_list() {
        let (_dir, path) = file();
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("creatable");
        std::fs::write(&path, br#"{ "version": 2, "recent": [{ "path": "/a.jpg" }] }"#).expect("writable");

        assert_eq!(read_at(&path), RecentList::default());
    }

    #[test]
    fn a_later_slices_members_are_ignored_and_a_hand_edited_list_is_read_as_one_this_writes() {
        let (_dir, path) = file();
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("creatable");
        let entries: Vec<serde_json::Value> =
            (1..=12).chain([1]).map(|n| serde_json::json!({ "path": format!("/{n}.jpg") })).collect();
        let stored = serde_json::json!({ "version": 1, "session": { "files": [] }, "recent": entries });
        std::fs::write(&path, serde_json::to_vec(&stored).expect("JSON")).expect("writable");

        let read = read_at(&path).paths();

        assert_eq!(read.len(), LIMIT);
        assert_eq!(read.first().map(String::as_str), Some("/1.jpg"));
        assert_eq!(read.iter().filter(|path| *path == "/1.jpg").count(), 1);
        assert_eq!(read_at(&path).0[0].identity, None, "an entry without an identity reads as one");
    }

    #[test]
    fn a_change_is_saved_and_an_unchanged_list_writes_nothing() {
        let (_dir, path) = file();
        let recents = Recents { file: Some(path.clone()), list: Mutex::new(RecentList::default()) };

        assert_eq!(recents.update(|list| list.note(vec![recent("/a.jpg")])), Some(vec!["/a.jpg".to_owned()]));
        assert_eq!(recents.update(|list| list.note(vec![recent("/a.jpg")])), None);
        assert_eq!(read_at(&path).paths(), ["/a.jpg"]);
        assert_eq!(recents.path_at(0).as_deref(), Some("/a.jpg"));
        assert_eq!(recents.path_at(1), None);
    }

    /// A photograph that has gone is answered as gone and leaves the list; one that is there stays.
    #[test]
    fn checking_drops_a_photograph_that_is_no_longer_there() {
        use tauri::test::{mock_builder, mock_context, noop_assets};

        let (dir, path) = file();
        let present = dir.path().join("present.jpg");
        std::fs::write(&present, b"pixels").expect("writable");
        let present = present.to_string_lossy().into_owned();
        let gone = dir.path().join("gone.jpg").to_string_lossy().into_owned();

        let recents = Recents { file: Some(path.clone()), list: Mutex::new(list(&[&gone, &present])) };
        let app = mock_builder().manage(recents).build(mock_context(noop_assets())).expect("a mock app");

        assert!(check_recent(app.handle().clone(), present.clone(), Traceparent::default()));
        assert!(!check_recent(app.handle().clone(), gone, Traceparent::default()));

        assert_eq!(app.state::<Recents>().paths(), std::slice::from_ref(&present));
        assert_eq!(read_at(&path).paths(), [present]);
    }
}
