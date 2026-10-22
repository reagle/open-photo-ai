> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

Open Photo AI cannot be chosen from Finder's Open With menu for a photograph, nor from the equivalent menus on Windows
and Linux: the bundle declares no document types, so the platform does not know the application opens images, and even
if it did, nothing in the application receives a file it is asked to open.

## What Changes

Task estimate: **12 tasks**, one change. Not part of a series. Independent of `gui-macos-window-state` and of
`add-gui-shortcuts-4-app-actions` (both edit `run()` in `src/lib.rs`, a trivial merge).

- The bundle declares the image formats the application opens (`bundle.fileAssociations`: `CFBundleDocumentTypes` on
  macOS with rank Alternate, so it is offered but never takes over the default; ProgIDs on Windows; `MimeType=` on
  Linux), generated from and tested against `opai::image::input_extensions`.
- Files the platform asks the application to open are added to the open images, by every route: macOS
  `RunEvent::Opened` at launch and while running; on Windows and Linux the first launch's command-line arguments, and
  a second launch's arguments through the single-instance plugin.
- A file arriving before the window is ready (at launch, during first-run setup) is held and added once it is.
- **Behaviour change** to `single-instance`: a second launch that names files now opens them in the running instance
  (it still raises the window and still ends successfully).
- Unsupported files are refused with the same notice as a drop; supported ones are described exactly as the picker and
  the drop describe them.

## Capabilities

### New Capabilities

### Modified Capabilities

- `gui-images`: adds a requirement that files the platform asks the application to open are opened, by every route.
- `single-instance`: a second launch naming files opens them rather than ignoring its arguments.

## Impact

- Crates: `gui` (`tauri.conf.json`, a new `src/opened.rs`, `src/lib.rs`, `src/single_instance.rs`; frontend
  `hooks/useDroppedImages.ts` refactored into a shared path-opening function, a new `hooks/useOpenedFiles.ts`, 14
  locales untouched unless a new string is needed). The format list is `opai`'s (`opai::image::input_extensions`),
  reused, not duplicated.
- `rust-sak`: nothing relevant; `image` already supplies the decoders behind `input_extensions`.
- No new crates.
- Base: `main@upstream`; upstream-able on its own.
