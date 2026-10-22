> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

Slice 1 (`add-gui-shortcuts-1-defaults`) gave the window four keyboard actions with fixed bindings. Fixed bindings
fail exactly the users the defaults were not chosen for: `\` is an AltGr chord on German and Nordic layouts, a
Lightroom user wants Lightroom's keys and a Topaz user Topaz's, and Topaz Photo AI itself does not allow remapping
(design.md of slice 1, D1), which is a gap this application can close cheaply. The settings dialog already has the
machinery a rebinding needs --- a draft, Save and Cancel, per-page Reset, and persistence with a repair per field ---
so the bindings belong there.

## What Changes

Task estimate for the whole feature: **about 25**, a series of three slices (see design.md - Slices). This change is
the second slice, `add-gui-shortcuts-2-settings`, estimated at **10 tasks**.

**In `crates/gui/frontend` only, observable in the window:**

- **A fifth settings page, Keyboard**, after Export, listing every keyboard action with the keys it is bound to. Each
  binding is shown as the keys the user presses (`⌘ =` on macOS, `Ctrl+=` elsewhere) and can be removed; Add shortcut
  records the next key combination pressed. An action may have several bindings, or none.
- **A binding already held by another action is refused** with a message naming that action, and one button, *Use here
  instead*, that moves the binding over. Nothing is moved without that click.
- **Keys that would trap the user are refused** with a message saying why: `Escape` and `Tab` (with any modifiers),
  `Enter` with no modifier other than Shift, and the platform's own window keys (`⌘Q`, `⌘W`, `⌘H`, `⌘M` on macOS;
  `Alt+F4` on Windows and Linux). While recording, `Escape` cancels the recording without closing the dialog, and
  `Tab` cancels it and moves the focus on as usual.
- **The bindings are a draft like every other setting**: nothing changes until Save, Cancel discards, and the page's
  Reset to defaults puts every action back to its default keys. Saved bindings take effect at once and survive a
  restart.
- **A remembered binding the application cannot honour is dropped** on start (an unknown action, a malformed or
  refused key), and the rest are kept.
- **The zoom buttons' `aria-keyshortcuts` report the user's bindings**, not the defaults.
- **i18n:** the page's title and description, the four action names, the page's controls and messages, and the name of
  the `Space` key, in all 14 locales.
- **Not in this slice:** tooltips, a cheat sheet and further actions (slice 3); exporting or importing the bindings as
  a file (design.md - D8).

## Capabilities

### New Capabilities

(none --- `gui-shortcuts` is introduced by slice 1, which is implemented but not yet archived; this change adds to
it.)

### Modified Capabilities

- `gui-shortcuts`: the bindings become the user's --- changed on a settings page, refused when held by another action
  or when they would trap the user, remembered across restarts and repaired when they cannot be honoured.
- `gui-settings`: the settings surface has five pages rather than four, and a reset of the Keyboard page puts every
  action back to its default keys.

## Impact

- **Crates touched: `gui` only, and only its frontend.** No Rust, no Tauri command, no capability change. Settings
  live in the webview's `localStorage` through the `settings-storage` zustand store, as every preference does today;
  `opai` has no settings store and the CLI has no interactive view, so nothing here is logic both front ends would
  need.
- **`rust-sak`:** checked; it has no keyboard, input or settings module, and none is warranted. Not used, not changed.
- **No new npm dependency.**
- **Persisted state:** `settings-storage` gains a `shortcuts` field holding only the actions the user changed. A file
  written before this change has none and reads as every action at its defaults; the Wails application never wrote the
  field.
- **Telemetry:** `settings_saved` already names every changed preference except analytics, so it will name `shortcuts`
  when bindings change --- the field name only, never a key.
- **Files:** `lib/shortcuts.ts`, `stores/settings.ts`, `hooks/useShortcuts.ts`, `features/drawer/DrawerHeader.tsx`,
  `features/settings/{pages.ts,SettingsDialog.tsx}`, a new `features/settings/KeyboardPage.tsx`, the 14 locale files,
  and their tests.
