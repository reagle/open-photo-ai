> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

The GUI can only be zoomed with the mouse (wheel, pinch, the drawer's slider and its two step buttons), and the only
way to compare a result with its source is to change the comparison mode. Every photo editor this application is
measured against --- Topaz Photo AI and Adobe Lightroom Classic --- lets the user zoom, fit and glance at the original
from the keyboard, and the glance (press and hold to see the original, release to see the result) is the fastest way
to judge whether an enhancement helped.

## What Changes

Task estimate for the whole feature: **about 25**, so it is a series of three slices (see design.md - Slices). This
change is the first slice, `add-gui-shortcuts-1-defaults`, estimated at **10 tasks**.

**In `crates/gui/frontend` only, observable in the window:**

- **Four keyboard actions with fixed default bindings.** `Mod` is Command on macOS and Control on Windows and Linux:
  - Zoom in: `Mod+=` (the same physical key with Shift, `Mod++`, and the numeric keypad's `+` also count).
  - Zoom out: `Mod+-` (and the numeric keypad's `-`).
  - Fit to window: `Mod+0`.
  - Show original while held: `\` (Lightroom Classic's Before key), which works even when a button has focus. Not
    `Space`: see design.md - D1. The user can bind `Space` in slice 2.
- **Zoom in and out step exactly as the drawer's minus and plus buttons do** (the same step, about the middle of the
  pane, clamped to the same range), so the keyboard is a second route to the same control rather than a second zoom
  model. Fit puts the current image back to the size that fits, centred.
- **Holding the show-original key makes every pane that draws the enhanced image draw the original instead, and its
  label reads Original**, in all three comparison modes. Releasing the key, or the window losing focus while it is
  held, puts the result back. Key repeat while held does nothing further.
- **Shortcuts never fire while the user is typing or inside a dialog.** A key pressed in a text field, a number field,
  a select or an editable element, or while the settings, crop, export, face-selection or setup dialog is open, is
  left to that control. `Space`, if the user binds it, keeps its native meaning on a focused button, checkbox, slider,
  tab or menu item.
- **The zoom buttons announce their shortcuts** through `aria-keyshortcuts`, so a screen reader reports them. No
  visible text changes in this slice, so no i18n keys are added.
- **Not in this slice:** changing the bindings (slice 2), showing them in tooltips or a cheat sheet, and further
  actions such as 100% view or next/previous image (slice 3).

## Capabilities

### New Capabilities

- `gui-shortcuts`: keyboard shortcuts in the GUI --- which actions have shortcuts, their default bindings per
  platform, when a shortcut is ignored, and what the hold-to-show-original action draws.

### Modified Capabilities

(none --- the zoom control, the wheel and the comparison modes keep their requirements unchanged; the shortcuts are a
new route onto them, specified in `gui-shortcuts`.)

## Impact

- **Crates touched: `gui` only, and only its frontend.** No Rust, no Tauri command, no capability change. Keyboard
  handling is presentation: the CLI has no interactive view to bind keys in, and `opai` has no settings store and
  knows nothing of any user interface (project context - Architecture), so nothing here is logic both front ends would
  need.
- **`rust-sak`:** checked; it has no keyboard, input or GUI module, and none is warranted --- this is browser-side
  TypeScript. Nothing from it is used and it is not changed.
- **No new npm dependency.** A small in-house matcher and one hook (design.md - D2) instead of `react-hotkeys-hook` or
  `tinykeys`.
- **Tauri:** no native menu exists today and none is added; the webview's own zoom hotkeys are already off
  (`zoomHotkeysEnabled` is not set, and Tauri defaults it to false), so `Mod+=`/`Mod+-`/`Mod+0` reach the page. This
  must be confirmed by hand on WebView2 and WebKitGTK (design.md - Risks).
- **Files:** new `lib/shortcuts.ts`, `hooks/useShortcuts.ts`, `stores/peek.ts` and their tests; edits to `App.tsx`,
  `features/preview/PreviewImage.tsx`, `features/drawer/DrawerHeader.tsx`.
- **Upstream:** CONTRIBUTING.md asks for an issue before a new UI feature; see design.md - Difficulty and likely
  paths.
