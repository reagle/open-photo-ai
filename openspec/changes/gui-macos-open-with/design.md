## Context

Root cause, with evidence:

- `crates/gui/tauri.conf.json:55-66` (`bundle`) has no `fileAssociations`, so the bundled `Info.plist` has no
  `CFBundleDocumentTypes` and Launch Services never lists the app under Open With.
- `crates/gui/src/lib.rs:178-189`: the run callback handles only `RunEvent::Exit`; `RunEvent::Opened { urls }`
  (`tauri-2.11.5/src/app.rs:263`, macOS/iOS only) is ignored, so a file sent by Finder - at launch or while running -
  is dropped.
- `crates/gui/src/lib.rs:88-94`: the single-instance callback ignores `_args` and `_cwd` by design
  (`specs/single-instance` - "Arguments on the second launch change nothing"). On Windows and Linux that is the route
  a file opened from the file manager takes when the app is already running.
- No code reads `std::env::args` on first launch.
- Opening paths already exists for drag-and-drop: `hooks/useDroppedImages.ts:91-120` filters by `input_extensions`,
  warns about refused files, calls `describe_images` (`src/images/files.rs:290-306`, which admits them to `Opened`)
  and `addFiles`.

## Goals / Non-Goals

**Goals:** Open With and double-click-to-open on all three platforms; files arriving at any moment of the app's life
are added; one code path for "paths from outside" shared with the drop.

**Non-Goals:** becoming the default handler; folders (Open With offers files); URL schemes; opening images into a new
window.

## How the work divides

`opai` already owns the list of input extensions. `gui` Rust collects paths from the platform and hands them to the
window; the frontend filters, describes and adds them through the drop's code. `cli` is untouched.

## `rust-sak` coverage

Nothing applies; this is platform plumbing in the GUI.

## Decisions

### D1. File associations generated from `input_extensions`

One `fileAssociations` entry per format family (JPEG, PNG, TIFF, WebP, AVIF, HEIF/HEIC, BMP, GIF, camera RAW incl.
DNG), each with its `ext` list, `contentTypes` UTIs (`public.jpeg`, `public.png`, `public.tiff`,
`org.webmproject.webp`, `public.avif`, `public.heic`/`public.heif`, `com.microsoft.bmp`, `com.compuserve.gif`,
`public.camera-raw-image`, `com.adobe.raw-image`), `role: "Editor"`, `rank: "Alternate"` and `mimeType` for Linux.
Written by hand in `tauri.conf.json`, with a `lib.rs` drift test asserting that the union of `ext` equals
`opai::image::input_extensions()` - the pattern the file already uses for the CSP and `backgroundColor`.

Alternative: generate the configuration in `build.rs`. Rejected: Tauri reads the JSON before `build.rs` could write it
cleanly, and a test catches drift as well.

### D2. One inbox in Rust: `src/opened.rs`

A managed `Inbox(Mutex<Vec<PathBuf>>)` with `push(paths)` and a `take_opened_files` command that drains it. Every
route pushes and then emits `files-opened` (no payload) and raises the window (`single_instance::raise_main_window`,
which also un-hides a window hidden by slice 4 of the shortcuts series):

- macOS `RunEvent::Opened { urls }`: `file://` URLs converted with `Url::to_file_path`; others ignored.
- First launch on Windows and Linux: `std::env::args_os().skip(1)` in `setup`, kept only if they are existing files.
- Second launch (single-instance callback): `args` after the first, resolved against `cwd` when relative.

The inbox, not an event payload, is the source of truth: a launch-time `Opened` fires before the page exists and an
event emitted then is lost.

### D3. The frontend drains when it can use the files

`hooks/useOpenedFiles.ts`, mounted in `App.tsx`, drains with `take_opened_files` once the setup store is `ready`, and
again on every `files-opened`. The drop's `open(paths)` moves into `lib/openPaths.ts` (`openPaths(paths, source)`),
used by both, so refusal notices, `describe_images` and `files_added` (`source: "open_with"`) are identical.

Alternative: describe in Rust and emit records. Rejected: the refusal notice and analytics are the frontend's, as in
the drop.

### D4. Security

`take_opened_files` returns only paths the platform handed this process; admission to `Opened` still happens only
through `describe_images`, as for a drop. No new capability is granted.

## Risks / Trade-offs

- [Launch Services caches bundle declarations] -> by-hand check after copying the app to /Applications;
  `lsregister -f` if needed.
- [A RAW extension with no system UTI] -> listed by `ext` too, so it still matches.
- [Windows installers register associations for every listed extension] -> accepted; rank cannot be expressed there,
  but the user still chooses.
- [Validated against] macOS by reasoning and Tauri's source; Windows and Linux untested.

## Difficulty and likely paths

Moderate (a day). The macOS path is mostly configuration plus a small event handler; the subtle part is the
launch-time race, solved by the inbox. Windows/Linux argv handling is a few lines but untested by the author.
