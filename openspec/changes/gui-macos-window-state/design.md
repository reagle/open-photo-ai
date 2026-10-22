## Context

Root cause, with evidence:

- `crates/gui/tauri.conf.json:21` declares the main window `"maximized": true` (with `width` 1280 / `height` 720 and
  the same values as `minWidth` / `minHeight`). Every launch therefore maximises - on macOS this is the "zoomed" frame
  filling the screen, which reads as full screen.
- `crates/gui/src/lib.rs:59-190` (`run`) registers no window-state plugin and no `on_window_event`; nothing saves the
  frame. `crates/gui/Cargo.toml` has no such dependency.
- `crates/gui/src/single_instance.rs:196-199` deliberately never calls `unmaximize`, reasoning that "the window ships
  `maximized: true`". `crates/gui/src/window.rs:28-32` documents that on Windows `maximized: true` defeats
  `visible: false` (tao's `ShowWindow(SW_MAXIMIZE)` shows the window early) - dropping the flag also fixes that.
- The window ships hidden (`visible: false`) and is shown by `window::window_ready` / `reveal_when_late`; the `lib.rs`
  test `the_window_ships_hidden_in_the_colour_the_stylesheet_paints` pins that.

The same file and lines are identical on `main@upstream`.

## Goals / Non-Goals

**Goals:** remember size, position and maximised state per user; restore safely across monitor changes; keep the
hidden-until-painted launch.

**Non-Goals:** remembering native full-screen (a macOS Space) - restoring into a full-screen Space at launch is
disorienting and Tauri's restore of it is unreliable; multiple windows; remembering panel or drawer state.

## How the work divides

All in `gui`: the plugin registration in `src/lib.rs`, the configuration change in `tauri.conf.json`, and comment and
test updates. `opai` and `cli` are untouched.

## `rust-sak` coverage

`rust-sak` has no window or geometry module, and this is not general-purpose infrastructure for it; nothing to extend.

## Decisions

### D1. `tauri-plugin-window-state` rather than a local implementation

The official plugin (2.5.0 at the time of writing; pin `"2"` like the other plugins) saves on window close and on
`RunEvent::Exit`, tracks moves and resizes, stores per-window-label state in `.window-state.json`, and on restore only
applies a saved position when a connected monitor intersects the saved frame
(`tauri-plugin-window-state-2.5.0/src/lib.rs:206-226`), otherwise leaving placement to the platform. It also keeps the
pre-maximise frame (`prev_x`/`prev_y`), so un-maximising after a restored maximised launch returns to the user's own
size.

Alternative: a ~100-line local module (save `outer_position`/`outer_size`/`is_maximized` on `CloseRequested` and
`Exit` into the GUI's settings file, restore in `setup`). Rejected: it would re-derive the monitor-intersection and
scale-factor handling the plugin already gets right, for no gain except the file's location.

### D2. Flags: size, position, maximised - not visible, not full screen, not decorations

`Builder::default().with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)`. Excluding
`VISIBLE` is essential: with it the plugin shows the window at restore time, before React has painted, undoing the
handshake in `window.rs`. Excluding `FULLSCREEN` per the Non-Goals; `DECORATIONS` is fixed by configuration.

The file lives in Tauri's `app_config_dir` (`~/Library/Application Support/io.vinicius.opai/` on macOS), not in
`opai`'s own configuration directory. Accepted: it is GUI-only state; `with_filename` keeps the plugin's default name.

### D3. First launch: centred default size, not maximised

Remove `"maximized": true`; add `"center": true` and a default `width`/`height` of 1440 x 900, keeping `minWidth` 1280
/ `minHeight` 720. The plugin then overrides size and position from the second launch on. Alternative: keep maximised
for the first launch only (`skip_initial_state` + maximise when no state exists) - possible, but a maximised first
launch is exactly what the user reported as wrong.

### D4. Registration order

Registered after the single-instance plugin (which must stay first, `lib.rs:83-94`) and before `.setup`, so the
restore runs as the window is created, while it is still hidden. Comments in `single_instance.rs` and `window.rs` that
cite `maximized: true` are rewritten; `raise_main_window` still never un-maximises.

## Risks / Trade-offs

- [The saved frame is smaller than the minimum after a config change] -> the window's `minWidth`/`minHeight` still
  apply; the platform clamps.
- [A monitor arrangement changes between sessions] -> the plugin skips a position no monitor intersects; size is still
  restored. A frame larger than the new monitor is clamped by macOS; on Windows and Linux it may overhang - accepted.
- [Validated against] macOS by reasoning and the plugin's source; Windows and Linux untested. Execution providers are
  not involved.

## Difficulty and likely paths

Easy (about half a day). The likely path is exactly D1-D4. The only subtle part is the `VISIBLE` flag and confirming
by hand that the window still appears only once painted.
