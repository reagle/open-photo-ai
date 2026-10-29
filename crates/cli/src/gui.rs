//! `--gui`: the photographs handed to Open Photo AI's window instead of enhanced here.
//!
//! ```text
//! ignored options  --> ⚠️ naming each option given besides the inputs and -r; they are the window's to set
//! inputs           --> expanded and checked as a run's are; nothing to open refuses it
//! lens blur        --> with --lens-blur, `opai::open_request` written for the application to claim when the paths
//!                      arrive: a Lens Blur on each and Autopilot off. One that cannot be written is a warning, and the
//!                      photographs open without it
//! hand over        --> macOS: `open -b io.vinicius.opai <paths>`, which delivers them as an "open documents" Apple
//!                      Event whether or not the application is running
//!                      Windows, Linux: the application's binary, beside this one or on PATH, started detached with
//!                      the paths; a running instance receives them through its single-instance plugin
//! confirm          --> 🖼️ Opening N images in Open Photo AI
//! ```
//!
//! All of it happens before the library is initialized and before telemetry starts: the application holds the
//! single-instance claim a run would be refused by, and a handover is not a run.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use opai::open_request::{self, LensBlurRequest, OpenRequest};

use crate::args::Args;
use crate::error::CliError;
use crate::inputs;
use crate::look::{self, Look};

/// The application's name, as the lines name it.
const APP: &str = "Open Photo AI";

/// The application's bundle identifier on macOS, `identifier` in `crates/gui/tauri.conf.json`.
#[cfg(any(target_os = "macos", test))]
const BUNDLE_ID: &str = "io.vinicius.opai";

/// The application's binary on Windows and Linux, `mainBinaryName` in `crates/gui/tauri.conf.json`.
#[cfg(any(not(target_os = "macos"), test))]
const BINARY: &str = if cfg!(windows) { "OpenPhotoAI.exe" } else { "OpenPhotoAI" };

/// A program and its arguments: what the handover runs, kept apart from running it so it can be tested.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Launch {
    program: PathBuf,
    args: Vec<OsString>,
}

/// Warns about the options `--gui` ignores, expands the inputs and hands every image to the application, then says how
/// many it opened.
///
/// # Errors
///
/// [`CliError::Usage`] where an input is refused or nothing is left to open, as a run's inputs are refused, and
/// [`CliError::Launch`] where the application cannot be found or started.
pub fn open(args: &Args, look: Look) -> Result<(), CliError> {
    if let Some(warning) = ignored_warning(&args.ignored) {
        eprintln!("{}", look::warning(warning, look.color));
    }

    let sources = inputs::expand(&args.inputs, args.recursive, None)?;
    let paths = sources
        .iter()
        .map(|source| {
            std::path::absolute(&source.path)
                .map_err(|error| CliError::Usage(format!("cannot resolve {}: {error}", source.path.display())))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if let Some(warning) = request_lens_blur(args, &paths) {
        eprintln!("{}", look::warning(warning, look.color));
    }

    hand_over(&paths)?;
    tracing::info!("handed {} images to the GUI", paths.len());

    eprintln!("{}", look::success(confirmation(paths.len()), look.color));
    Ok(())
}

/// The warning naming the options `--gui` ignores, or `None` when there are none.
fn ignored_warning(ignored: &[String]) -> Option<String> {
    let (last, rest) = ignored.split_last()?;
    let (names, verb, them) = if rest.is_empty() {
        (last.clone(), "is", "it")
    } else {
        (format!("{} and {last}", rest.join(", ")), "are", "them")
    };

    Some(format!("{names} {verb} ignored with --gui; set {them} in {APP} instead"))
}

/// Writes the request for the application to add the lens blur `args` asks for to each of `paths`, where it asks for
/// one; answers the warning to give where the request could not be written.
fn request_lens_blur(args: &Args, paths: &[PathBuf]) -> Option<String> {
    let selection = &args.selection;
    selection.lens_blur?;

    let lens_blur = LensBlurRequest {
        f_number: selection.lens_blur_f,
        amount: selection.lens_blur_amount,
        focus: selection.lens_blur_focus,
    };
    let request = OpenRequest::new(paths.to_vec(), lens_blur);

    let written = open_request::path(opai::APP_NAME)
        .map_err(|error| error.to_string())
        .and_then(|path| open_request::write_at(&path, &request).map_err(|error| error.to_string()));

    written.err().map(|error| {
        tracing::warn!(%error, "the lens blur for the GUI could not be requested");
        format!("could not ask {APP} for the lens blur ({error}); the photographs open without it")
    })
}

/// `🖼️ Opening N images in Open Photo AI`, or `1 image`.
fn confirmation(count: usize) -> String {
    let noun = if count == 1 { "image" } else { "images" };
    format!("🖼️ Opening {count} {noun} in {APP}")
}

/// `open -b <bundle id> <paths>`: Launch Services starts the application if it is not running, and delivers the paths
/// as an "open documents" Apple Event either way. No `--args`, which only a fresh launch would see.
#[cfg(any(target_os = "macos", test))]
fn macos_launch(paths: &[PathBuf]) -> Launch {
    let mut args = vec![OsString::from("-b"), OsString::from(BUNDLE_ID)];
    args.extend(paths.iter().map(|path| path.as_os_str().to_owned()));

    Launch { program: PathBuf::from("open"), args }
}

/// Runs `open` and waits for it, which returns once the event is sent; its own message explains a failure.
#[cfg(target_os = "macos")]
fn hand_over(paths: &[PathBuf]) -> Result<(), CliError> {
    let launch = macos_launch(paths);
    let output = Command::new(&launch.program)
        .args(&launch.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| CliError::Launch(format!("could not run open to start {APP}: {error}")))?;

    if output.status.success() {
        return Ok(());
    }

    let reason = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let reason = if reason.is_empty() { output.status.to_string() } else { reason };
    Err(CliError::Launch(format!("could not open {APP}; is it installed? {reason}")))
}

/// Starts the application's binary detached, with the paths as its arguments. A running instance's single-instance
/// plugin receives them and the new process exits; otherwise the new process opens them itself.
#[cfg(not(target_os = "macos"))]
fn hand_over(paths: &[PathBuf]) -> Result<(), CliError> {
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
    let program = find_binary(beside.as_deref(), std::env::var_os("PATH").as_deref()).ok_or_else(|| {
        CliError::Launch(format!("could not find {APP}: {BINARY} is neither beside opai nor on PATH"))
    })?;

    let mut command = Command::new(&program);
    command.args(paths).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    detach(&mut command);

    command
        .spawn()
        .map(drop)
        .map_err(|error| CliError::Launch(format!("could not start {}: {error}", program.display())))
}

/// The application's binary: beside this one first, as an installation that ships both puts it, then on `path`.
#[cfg(any(not(target_os = "macos"), test))]
fn find_binary(beside: Option<&std::path::Path>, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    beside
        .into_iter()
        .map(std::path::Path::to_path_buf)
        .chain(path.into_iter().flat_map(std::env::split_paths))
        .map(|dir| dir.join(BINARY))
        .find(|candidate| candidate.is_file())
}

/// Lets the application outlive this process and its terminal: no console, its own process group.
#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

/// Lets the application outlive this process and its terminal: its own process group, so a Ctrl-C in the terminal
/// does not reach it.
#[cfg(all(unix, not(target_os = "macos")))]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn nothing_ignored_is_no_warning() {
        assert_eq!(ignored_warning(&[]), None);
    }

    #[test]
    fn the_warning_names_every_ignored_option() {
        assert_eq!(
            ignored_warning(&names(&["--upscale"])).as_deref(),
            Some("--upscale is ignored with --gui; set it in Open Photo AI instead")
        );
        assert_eq!(
            ignored_warning(&names(&["--upscale", "--quality", "--output"])).as_deref(),
            Some("--upscale, --quality and --output are ignored with --gui; set them in Open Photo AI instead")
        );
    }

    #[test]
    fn the_confirmation_counts_the_images() {
        assert_eq!(confirmation(1), "🖼️ Opening 1 image in Open Photo AI");
        assert_eq!(confirmation(3), "🖼️ Opening 3 images in Open Photo AI");
    }

    #[test]
    fn macos_opens_the_bundle_with_the_paths_and_nothing_else() {
        let paths = [PathBuf::from("/photos/a.jpg"), PathBuf::from("/photos/with space/b.png")];

        let launch = macos_launch(&paths);

        assert_eq!(launch.program, PathBuf::from("open"));
        assert_eq!(
            launch.args,
            ["-b", "io.vinicius.opai", "/photos/a.jpg", "/photos/with space/b.png"].map(OsString::from)
        );
    }

    #[test]
    fn the_bundle_id_and_binary_are_the_applications() {
        let config = include_str!("../../gui/tauri.conf.json");

        assert!(config.contains(&format!("\"identifier\": \"{BUNDLE_ID}\"")), "{config}");
        let binary = BINARY.trim_end_matches(".exe");
        assert!(config.contains(&format!("\"mainBinaryName\": \"{binary}\"")), "{config}");
    }

    #[test]
    fn the_binary_is_found_beside_this_one_before_path() {
        let beside = tempfile::tempdir().expect("a scratch directory");
        let on_path = tempfile::tempdir().expect("a scratch directory");
        let empty = tempfile::tempdir().expect("a scratch directory");
        std::fs::write(on_path.path().join(BINARY), b"").expect("written");

        let path = std::env::join_paths([empty.path(), on_path.path()]).expect("joins");
        assert_eq!(find_binary(Some(beside.path()), Some(&path)), Some(on_path.path().join(BINARY)));

        std::fs::write(beside.path().join(BINARY), b"").expect("written");
        assert_eq!(find_binary(Some(beside.path()), Some(&path)), Some(beside.path().join(BINARY)));

        assert_eq!(find_binary(Some(empty.path()), None), None);
    }
}
