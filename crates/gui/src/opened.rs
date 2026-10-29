//! Files the platform asks the application to open: Open With, a drop on the application's icon, a double-click in
//! the file manager, or paths named on the command line.
//!
//! Every route ends in [`deliver`], which puts the paths in the [`Inbox`], tells the window with [`FILES_OPENED`] and
//! raises it. The window drains the inbox with [`take_opened_files`] once it can use the files, and opens them exactly
//! as it opens a drop: the extension check, the refusal notice and `describe_images`, which is still the only way a
//! file is admitted to [`crate::images::Opened`].
//!
//! A batch `opai --gui --lens-blur` handed over also carries the lens blur it asked for, in the
//! [`opai::open_request`] the command line wrote beside the handover: [`deliver`] claims it as the paths arrive, and
//! the window takes it with [`take_open_request`] in the same drain, before it opens them.

// The inbox rather than the event's payload is what carries the paths, because the event alone would lose the first
// files: a launch to open a photograph hands it over (`RunEvent::Opened` on macOS, the command line elsewhere) before
// the page exists, and an event emitted then has no listener. The window drains once its setup is ready and again on
// every event, so files that arrive at any moment of the application's life are opened once and only once.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use opai::open_request::{self, OpenRequest};
use serde::Serialize;

use crate::command::{Traceparent, command_span, traced_sync};
use crate::sync::lock;

/// The event that tells the window the inbox has something in it. It carries no payload; the window drains.
///
/// Written once more, in `frontend/ipc/images.ts`.
pub(crate) const FILES_OPENED: &str = "files-opened";

/// The paths the platform has handed this process and the window has not taken yet.
#[derive(Debug, Default)]
pub(crate) struct Inbox(Mutex<Vec<PathBuf>>);

impl Inbox {
    /// Adds `paths` to what the window will take next.
    pub(crate) fn push(&self, paths: impl IntoIterator<Item = PathBuf>) {
        lock(&self.0).extend(paths);
    }

    /// Everything waiting, in the order it arrived, leaving the inbox empty.
    pub(crate) fn take(&self) -> Vec<PathBuf> {
        std::mem::take(&mut *lock(&self.0))
    }
}

/// The lens blur a batch handed over by `opai --gui` asked for, claimed and not taken by the window yet.
#[derive(Debug, Default)]
pub(crate) struct Requested(Mutex<Option<Request>>);

/// What a batch asked for, as the window reads it: `OpenRequest` with the paths the window knows the photographs by.
///
/// Written once more, as `OpenRequest`, in `frontend/ipc/images.ts`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Request {
    paths: Vec<String>,
    lens_blur: LensBlur,
}

/// A lens blur as the command line asked for it; what is absent is the window's own default.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LensBlur {
    #[serde(skip_serializing_if = "Option::is_none")]
    f_number: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    focus_x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    focus_y: Option<f64>,
}

impl From<OpenRequest> for Request {
    fn from(request: OpenRequest) -> Self {
        let blur = request.lens_blur;
        Self {
            // A path that is not Unicode cannot cross to the window, and `take_opened_files` leaves it out too.
            paths: request.paths.into_iter().filter_map(|path| path.into_os_string().into_string().ok()).collect(),
            lens_blur: LensBlur {
                f_number: blur.f_number,
                amount: blur.amount,
                focus_x: blur.focus.map(|(x, _)| x),
                focus_y: blur.focus.map(|(_, y)| y),
            },
        }
    }
}

// The command's name is written once more, in `frontend/ipc/images.ts`.
/// What the batch the window last drained asked for besides opening it, once, or nothing.
#[tauri::command]
pub(crate) fn take_open_request(requested: State<'_, Requested>, traceparent: Traceparent) -> Option<Request> {
    traced_sync(command_span!("take_open_request", traceparent), || lock(&requested.0).take())
}

// The command's name is written once more, in `frontend/ipc/images.ts`.
/// The files the platform asked the application to open since the window last asked, each given out once.
///
/// Only paths the platform handed this process are here; nothing is admitted to [`crate::images::Opened`] until the
/// window describes them, as it would a drop. A path that is not valid Unicode cannot cross to the window and is
/// recorded and left out.
#[tauri::command]
pub(crate) fn take_opened_files(inbox: State<'_, Inbox>, traceparent: Traceparent) -> Vec<String> {
    traced_sync(command_span!("take_opened_files", traceparent), || {
        inbox
            .take()
            .into_iter()
            .filter_map(|path| match path.into_os_string().into_string() {
                Ok(path) => Some(path),
                Err(path) => {
                    tracing::warn!(path = %Path::new(&path).display(), "a file to open has a path that is not Unicode; skipped");
                    None
                }
            })
            .collect()
    })
}

/// Hands `paths` to the window: into the inbox, then the event, then the raise.
///
/// Nothing at all for no paths, so a second launch that named no file only raises the window, as before.
///
/// The window is raised only once it has been revealed. Until then it is still waiting for React to paint, and
/// `src/window.rs` shows it when that happens; showing it here would put the empty window on screen first.
pub(crate) fn deliver<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }

    tracing::info!(count = paths.len(), "the platform asked to open files");

    // Claimed before the paths are pushed, so a drain that takes them finds what they asked for beside them.
    if let Some(request) = claim_request(&paths) {
        tracing::info!(count = request.paths.len(), "the command line asked for a lens blur on the files");
        *lock(&app.state::<Requested>().0) = Some(request.into());
    }

    app.state::<Inbox>().push(paths);

    // An event with no listener - the page has not loaded - is not an error; the window drains when it is ready.
    if let Err(error) = app.emit(FILES_OPENED, ()) {
        tracing::warn!(%error, "could not tell the window that files are waiting to be opened");
    }

    if crate::window::has_been_revealed() {
        crate::single_instance::raise_main_window(app);
    }
}

/// The request `opai --gui` wrote for `paths`, where it wrote one for them.
fn claim_request(paths: &[PathBuf]) -> Option<OpenRequest> {
    let file = open_request::path(opai::APP_NAME)
        .inspect_err(|error| tracing::warn!(%error, "the open request's location could not be determined"))
        .ok()?;

    open_request::claim_at(&file, paths, std::time::SystemTime::now())
}

/// The local files among `urls`, which is how macOS names what it asks the application to open.
///
/// Anything that is not a `file:` URL, or that names no local path, is left out.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn paths_from_urls(urls: &[tauri::Url]) -> Vec<PathBuf> {
    urls.iter()
        .filter(|url| url.scheme() == "file")
        .filter_map(|url| url.to_file_path().ok())
        .collect()
}

/// The files named in `args`, a launch's arguments without the program itself, resolved against `cwd`.
///
/// Only what exists as a file is kept, so a flag, a folder or a mistyped name changes nothing: the window then never
/// hears of it, rather than naming it as a format the application does not open.
pub(crate) fn paths_from_args(args: impl IntoIterator<Item = impl Into<OsString>>, cwd: &Path) -> Vec<PathBuf> {
    args.into_iter()
        .map(|arg| cwd.join(PathBuf::from(arg.into())))
        .filter(|path| path.is_file())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn the_inbox_gives_out_what_was_pushed_once_and_in_order() {
        let inbox = Inbox::default();

        inbox.push([PathBuf::from("/a.jpg"), PathBuf::from("/b.png")]);
        inbox.push([PathBuf::from("/c.nef")]);

        assert_eq!(inbox.take(), ["/a.jpg", "/b.png", "/c.nef"].map(PathBuf::from));
        assert!(inbox.take().is_empty(), "a second drain gives out nothing");
    }

    #[test]
    fn an_empty_inbox_drains_to_nothing() {
        assert!(Inbox::default().take().is_empty());
    }

    #[test]
    fn delivering_fills_the_inbox_even_with_no_window_to_tell() {
        use tauri::test::{mock_builder, mock_context, noop_assets};

        // The launch-time case: the files arrive before any window exists, and must be waiting when it drains.
        let app = mock_builder()
            .manage(Inbox::default())
            .manage(Requested::default())
            .build(mock_context(noop_assets()))
            .expect("a mock app");

        deliver(app.handle(), vec![PathBuf::from("/a.jpg")]);
        deliver(app.handle(), Vec::new());

        assert_eq!(app.state::<Inbox>().take(), [PathBuf::from("/a.jpg")]);
    }

    #[test]
    fn a_request_reaches_the_window_with_its_focus_as_two_fractions_and_nothing_it_left_out() {
        let blur = open_request::LensBlurRequest { f_number: Some(1.4), amount: None, focus: Some((0.3, 0.6)) };
        let request = Request::from(OpenRequest::new(vec![PathBuf::from("/photos/a.jpg")], blur));

        let json = serde_json::to_string(&request).expect("serializes");

        assert_eq!(json, r#"{"paths":["/photos/a.jpg"],"lensBlur":{"fNumber":1.4,"focusX":0.3,"focusY":0.6}}"#);
    }

    // Unix only because the URLs are Unix paths, which is what macOS - the one platform that sends URLs - hands over.
    #[cfg(unix)]
    #[test]
    fn only_local_file_urls_become_paths() {
        let urls = [
            "file:///Users/me/Pictures/a.jpg",
            "https://example.com/b.jpg",
            "file:///Users/me/My%20Photos/c%C3%A9.png",
        ]
        .map(|url| tauri::Url::parse(url).expect("a valid URL"));

        let paths = paths_from_urls(&urls);

        // Percent-encoding is decoded: Finder escapes a space and every non-ASCII byte.
        assert_eq!(paths, [PathBuf::from("/Users/me/Pictures/a.jpg"), PathBuf::from("/Users/me/My Photos/cé.png")]);
    }

    #[test]
    fn arguments_are_resolved_against_the_launch_directory_and_kept_only_if_they_are_files() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        fs::write(dir.path().join("a.jpg"), b"").expect("writable");
        fs::create_dir(dir.path().join("folder")).expect("writable");
        let absolute = dir.path().join("b.png");
        fs::write(&absolute, b"").expect("writable");

        let args = [
            OsString::from("a.jpg"),
            absolute.clone().into_os_string(),
            OsString::from("--flag"),
            OsString::from("folder"),
            OsString::from("missing.jpg"),
        ];

        assert_eq!(paths_from_args(args, dir.path()), [dir.path().join("a.jpg"), absolute]);
    }

    #[test]
    fn no_arguments_name_no_files() {
        let dir = tempfile::tempdir().expect("a temporary directory");

        assert!(paths_from_args(Vec::<OsString>::new(), dir.path()).is_empty());
    }
}
