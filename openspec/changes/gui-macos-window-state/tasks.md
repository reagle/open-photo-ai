## 1. Plugin

- [x] 1.1 Add `tauri-plugin-window-state = { version = "2", features = [] }` to `crates/gui/Cargo.toml` with a comment in the file's style (why the plugin, Rust API only, no permission granted); verify `cargo build -p gui` succeeds and `Cargo.lock` gains one plugin entry
- [x] 1.2 Register it in `crates/gui/src/lib.rs` `run()` after the single-instance plugin with `StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED` (design.md - D2, D4); verify `cargo test -p gui` is green and the capability test still passes unchanged

## 2. Configuration

- [x] 2.1 In `crates/gui/tauri.conf.json` remove `"maximized": true`, add `"center": true` and a default 1440 x 900 size keeping the 1280 x 720 minimum (D3); verify `cargo test -p gui` (the conf drift guards) is green
- [x] 2.2 Add a `lib.rs` test that reads `tauri.conf.json` and asserts the main window is neither `maximized` nor `fullscreen` and still `visible: false`; verify it fails against the old configuration and passes against the new
- [x] 2.3 Rewrite the comments in `src/window.rs` (the Windows `maximized` paragraph) and `src/single_instance.rs` (the `unmaximize` paragraph) to match; verify `cargo doc -p gui --no-deps` builds without warnings

## 3. Verification

- [x] 3.1 Run `cargo fmt --check`, `cargo clippy -p gui --all-targets -- -D warnings` and `cargo test -p gui`; verify all are green
- [ ] 3.2 Verify by hand on macOS: first launch centred and not maximised; resize, move, quit, relaunch restores; maximise, quit, relaunch restores maximised; move to a second display, quit, disconnect it, relaunch opens on the remaining display; the window still appears only once painted (no blank frame); a second launch still raises without resizing; record which of Windows and Linux were tried
- [ ] 3.3 Add the scenario to `~/dev/opai-notes/TESTING.md` for the integration build; verify the checklist names the four by-hand cases

## Implementation notes

Branch `gui-macos-window-state` (one commit on `main@upstream`), pushed to `origin`.

- 1.1 deviation: `Cargo.toml` pins `"2"` as planned, but `Cargo.lock` resolves the plugin to **2.4.1**, not 2.5.0.
  2.5.0 requires `tauri ^2.12` and would have moved Tauri 2.11.5 -> 2.12.5 with 37 other lock entries (tao, wry, muda,
  ...); 2.4.1 (`tauri ^2.8.2`) adds the one plugin entry. Its restore logic (monitor intersection, `prev_x`/`prev_y`)
  is the same as 2.5.0's cited in D1. `cargo update -p tauri-plugin-window-state` takes 2.5.0 whenever Tauri moves to
  2.12.
- 1.2: the flags live in a `window_state_flags()` helper so a test can pin that `VISIBLE` (and `FULLSCREEN`) are out.
- 2.2: `the_window_opens_at_its_own_size_and_still_hidden` failed against the old configuration (`maximized`) and
  passes against the new.
- 2.3: the `window.rs` paragraph now also says that on Windows a launch restoring a *maximised* window still shows the
  empty window early (the plugin calls `maximize()` on the hidden window, which is tao's `ShowWindow(SW_MAXIMIZE)`).
- 3.1: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace` (2485
  passed, 41 ignored; `gui` 340) and `cargo doc -p gui --no-deps` all clean.
- 3.2 partly done on macOS (built app, 5K display at 2560 x 1440): first launch centred at 1440 x 900 and not
  maximised; moved/resized to (100, 120) 1500 x 950, quit, relaunch restored exactly. Not verified: maximise (the Zoom
  menu item and the zoom button's AX actions did nothing under automation, and a hand-edited `maximized: true` state
  produced an odd 1388 x 1107 frame - check by hand), second display, no blank frame, second-launch raise. Windows and
  Linux not tried.
- 3.3 not done: `~/dev/opai-notes/TESTING.md` was outside this implementation's allowed edits.

