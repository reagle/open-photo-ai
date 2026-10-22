> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

The main window opens maximised on every launch, whatever size and place the user last gave it: `tauri.conf.json`
declares `"maximized": true` (`crates/gui/tauri.conf.json:21`) and nothing records the window's frame between
sessions. On a large display, or with a second monitor, the user has to shrink and move the window every time.

## What Changes

Task estimate: **8 tasks**, one change. Not part of a series; it is independent of `add-gui-shortcuts-4-app-actions`
and `gui-macos-open-with` and can be proposed upstream on its own.

- The window remembers its size, position and maximised state when it closes or the application quits, and restores
  them at the next launch, on every platform.
- A position that no connected monitor shows any part of (a disconnected second display) is not restored: the window
  is placed by the platform instead, at its remembered size clamped to the minimum.
- A first launch, with nothing remembered, opens at a default size centred on the primary monitor rather than
  maximised. **Behaviour change** against upstream, which always maximises.
- The hidden-until-ready handshake (`window.rs`) is kept: visibility is never restored from the saved state.

## Capabilities

### New Capabilities

### Modified Capabilities

- `gui-shell`: adds a requirement that the window's frame is remembered and restored, and how a first launch and an
  off-screen frame are handled.

## Impact

- Crates: `gui` only (`Cargo.toml`, `tauri.conf.json`, `src/lib.rs`, comments in `src/window.rs` and
  `src/single_instance.rs`). No logic a CLI could want, so nothing goes in `opai`.
- `rust-sak`: checked `fs` (config directories); it has nothing for window geometry, which is a GUI concern. The
  plugin stores its file in Tauri's `app_config_dir`, not through `rust_sak::fs::user_config_dir` (see design.md -
  D2).
- New dependency: `tauri-plugin-window-state` 2.x (Apache-2.0 OR MIT), from the same `tauri-apps/plugins-workspace`
  release train as the single-instance plugin already used. Rust API only; no npm package and no capability grant.
- Base: `main@upstream` (no dependency on the fork's other branches); a good small upstream PR.
