> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

On macOS, Command-W quits the whole application, Command-O does nothing, and there is no keyboard route to Settings.
The window has no menu of its own: Tauri installs its generic default macOS menu, whose File > Close Window
(Command-W) closes the only window, after which the application exits. A user who closes an image the way every macOS
document application closes one loses the session, the models loaded and every open photograph.

## What Changes

Task estimate for the series: slices 1-3 (about 25 tasks) are done; this change is the fourth slice,
`add-gui-shortcuts-4-app-actions`, estimated at **14 tasks** (13 after D5 was settled, plus one for Export and
Command-slash, added on 2026-10-23; see design.md - Slices). It is stacked on `add-gui-shortcuts-3-discoverable`: it
extends that stack's refusal rules, Keyboard settings page and shortcuts list. Window size memory
(`gui-macos-window-state`) and Finder Open With (`gui-macos-open-with`) are separate changes.

- **macOS: an application menu of its own** replacing Tauri's default: Open Photo AI (About, Settings...
  Command-comma, Services, Hide, Hide Others, Show All, Quit Command-Q), File (Open... Command-O, Close Image
  Command-W, Close All Images Shift-Command-W), Edit (the standard items, which text fields need), View (Enter Full
  Screen), Window (Minimize, Zoom, Bring All to Front), Help. Labels follow the interface language. File also holds
  Export... (Command-S), between Open and the close items.
- **Command-W closes the current image, Shift-Command-W closes all images, and the application stays open.** Command-Q
  quits. Command-O opens the Browse images picker (today it does nothing). Command-comma opens Settings.
- **Export... opens the Export dialog**, as the sidebar's Export button does, and is unavailable whenever that button
  is (no image picked): Command-S (Topaz Photo AI's) and Shift-Command-E (Lightroom Classic's) on macOS, Ctrl+S and
  Ctrl+Shift+E elsewhere.
- **Command-slash (Ctrl+slash elsewhere) also opens the shortcuts list**, Topaz's key for its shortcuts panel, as a
  second default binding of slice 3's rebindable action beside `?`.
- **Windows and Linux: the same actions as window-level keys** (Ctrl+O, Ctrl+S, Ctrl+Shift+E, Ctrl+W, Ctrl+Shift+W,
  Ctrl+comma; Ctrl+Q on Linux; Alt+F4 stays the system's on Windows), handled by the interface; no menu bar is added
  there.
- These application keys are **fixed**: the Keyboard settings page and the shortcuts list show them read-only, and
  recording one of them for another action is refused. The existing zoom, fit, compare and navigation actions stay
  rebindable.
- **Closing the window still quits the application, on every platform** (Joseph's decision - design.md D5). What
  changes on macOS is that Command-W no longer closes the window; its close button and Command-Q still end the
  session.

## Capabilities

### New Capabilities

### Modified Capabilities

- `gui-shell`: adds the application-level actions (open, export, settings, close image, close all, quit), how each
  platform delivers them, the macOS application menu, what closing the window does, and their listing as fixed
  shortcuts; and a second default key for the shortcuts list.

## Impact

- Crates: `gui` only - a new `src/menu.rs`, `src/lib.rs` (menu and menu events), and the frontend (`lib/shortcuts.ts`,
  a new `hooks/useAppActions.ts`, settings-dialog and export-dialog stores, the sidebar, the Keyboard page and the
  shortcuts list, 14 locale files). Nothing a CLI needs, so nothing in `opai`.
- `rust-sak`: nothing relevant (no menu or windowing module); nothing to extend.
- No new crates: `tauri::menu` (muda) is part of Tauri core.
- Base: the shortcuts stack (`add-gui-shortcuts-3-discoverable`), not `main@upstream`. Upstream could take it after
  the shortcuts PR.
