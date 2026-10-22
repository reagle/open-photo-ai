## 1. Bundle

- [x] 1.1 Add `bundle.fileAssociations` to `crates/gui/tauri.conf.json` per design.md - D1 (families, `ext`, UTIs, `role: "Editor"`, `rank: "Alternate"`, `mimeType`); verify `cargo tauri build --bundles app` produces an `Info.plist` with `CFBundleDocumentTypes` and `LSHandlerRank` Alternate
- [x] 1.2 Add a `lib.rs` drift test that the union of every association's `ext` equals `opai::image::input_extensions()`; verify it fails with one extension removed and passes as written

## 2. Native side

- [x] 2.1 Add `src/opened.rs` with the managed inbox and the `take_opened_files` command (D2), registered in `generate_handler!`; add unit tests for push, drain-once and an empty drain; verify `cargo test -p gui opened` is green
- [x] 2.2 Handle `RunEvent::Opened` on macOS in `run()`'s callback: file URLs to paths, push, emit `files-opened`, raise; add a test for the URL conversion (file, non-file, percent-encoded); verify `cargo test -p gui` is green
- [x] 2.3 On Windows and Linux push existing-file arguments from `std::env::args_os` in `setup`; and in the single-instance callback push the second launch's file arguments resolved against `cwd`, then emit and raise; add tests for relative and absolute paths and non-file arguments; verify `cargo test -p gui` is green
- [x] 2.4 Update the comments in `src/single_instance.rs` and `lib.rs` that say arguments are ignored; verify `cargo doc -p gui --no-deps` is warning-free

## 3. Frontend

- [x] 3.1 Move the drop's path handling into `crates/gui/frontend/lib/openPaths.ts` (`openPaths(paths, source)`) and use it from `hooks/useDroppedImages.ts`; verify the existing `useDroppedImages` tests pass unchanged
- [x] 3.2 Add the `takeOpenedFiles` binding in `ipc/images.ts` and `hooks/useOpenedFiles.ts` (drain when setup is `ready` and on each `files-opened`), mounted in `App.tsx`, with `files_added` source `"open_with"`; add tests for a drain before and after ready, an event while running, and a refused file; verify `pnpm test hooks/useOpenedFiles` is green

## 4. Verification

- [x] 4.1 Run `cargo fmt --check`, `cargo clippy -p gui --all-targets -- -D warnings` and `cargo test -p gui`; verify all are green
- [x] 4.2 In `crates/gui` run `pnpm check`, `pnpm typecheck`, `pnpm test` and `pnpm build`; verify all are green
- [ ] 4.3 Verify by hand on macOS with the app in /Applications: Open With lists it for JPEG, PNG, HEIC and an ARW without changing the default app; Open With while running adds the files and raises; Open With while not running opens the file after launch (including during first-run setup); dropping a photo on the Dock icon opens it; `open -a "Open Photo AI" a.jpg b.txt` opens one and names the other; record which of Windows and Linux were tried
- [ ] 4.4 Add the by-hand cases to `~/dev/opai-notes/TESTING.md`; verify the checklist lists them

## Implementation notes

Branch `gui-macos-open-with` (one commit on `main@upstream`, independent of `gui-macos-window-state`), pushed to
`origin`.

- 1.1: nine associations (JPEG, PNG, TIFF, WebP, AVIF, HEIF/HEIC, BMP, GIF, camera RAW with the 29 decodable RAW
  extensions). Linux gets one `mimeType` per association, so RAW declares `image/x-dcraw`, the shared-mime-info parent
  of the per-camera types. The built `Info.plist` has nine `CFBundleDocumentTypes`, each `LSHandlerRank` Alternate,
  `CFBundleTypeRole` Editor, with both `LSItemContentTypes` and `CFBundleTypeExtensions`, and no
  `UTExportedTypeDeclarations`.
- 1.2: `tauri_conf_json_declares_exactly_the_extensions_opai_opens` also asserts role and rank, and that no extension
  is declared twice; it failed with `gif` removed and passes as written.
- 2.2/2.3 deviation: `opened::deliver` raises the window only once `window.rs` has revealed it (a new
  `has_been_revealed()`), because a launch-time `Opened` arrives while the window is still hidden waiting for React;
  raising then would show the empty window. The reveal itself shows and focuses it. A second launch naming no file
  raises exactly as before.
- 2.3 deviation: the first launch's arguments are read on every platform, not only Windows and Linux, so running the
  macOS binary with a path also opens it. macOS's own routes (Finder, `open -a`) arrive as `RunEvent::Opened`, never
  on the command line, so nothing is delivered twice.
- 2.1: `take_opened_files` returns `Vec<String>`; a non-Unicode path is logged and skipped. `Vec<String>` was added to
  `command.rs`'s `finished!` list.
- 3.2: also `onFilesOpened` and `FILES_OPENED_EVENT` in `ipc/images.ts`, a name test in `ipc/images.test.ts`, and
  `"open_with"` added to `files_added`'s `source` in `lib/events.ts`. The first drain runs after the listener is
  registered, so a file cannot fall between the two.
- 4.1: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace` (2490
  passed, 41 ignored; `gui` 345) and `cargo doc -p gui --no-deps` clean.
- 4.2: `pnpm check` (no fixes), `pnpm typecheck`, `pnpm test` (108 files, 1392 tests) and `pnpm build` green.
- 4.3 partly done, by log only (no UI automation or screen capture permitted): with the built app (not in
  /Applications), `open -a <app> "a copy.jpg" notes.txt` while not running logged "the platform asked to open files"
  count=2 at launch and then Autopilot analysed a new identity, i.e. the JPEG was described and added after setup;
  `open -a <app> b.jpg` while running reached the same process (no second instance) and the same path. Not verified:
  Finder's Open With menu listing it for JPEG/PNG/HEIC/ARW without changing the default, the refusal toast for
  `notes.txt`, the raise, a drop on the Dock icon, first-run setup. Windows and Linux not tried.
- 4.4 not done: `~/dev/opai-notes/TESTING.md` was outside this implementation's allowed edits.

