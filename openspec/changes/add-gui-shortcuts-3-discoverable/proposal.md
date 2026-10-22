> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

Slices 1 and 2 (`add-gui-shortcuts-1-defaults`, `add-gui-shortcuts-2-settings`) gave the window four keyboard actions
and let the user rebind them, but nothing in the window says they exist: the only trace is `aria-keyshortcuts` on two
buttons and a settings page a user has to find. And the four actions stop short of what a Topaz Photo AI or Lightroom
Classic user reaches for next: looking at the photograph's actual pixels (`Cmd/Ctrl 1` in Topaz, `Z` in Lightroom) and
moving to the next or previous photograph without the mouse.

## What Changes

Task estimate for the whole feature: **about 25**, a series of three slices (see design.md - Slices). This change is
the third and last slice, `add-gui-shortcuts-3-discoverable`, estimated at **11 tasks**.

**In `crates/gui/frontend` only, observable in the window:**

- **Actual pixels**, `Mod+1` by default: the current image is magnified so one of its pixels is one pixel of the
  display, about the middle of the pane, within the zoom range the preview already has.
- **Toggle zoom**, `Z` by default, as Lightroom's `Z`: from the fitted size to actual pixels, and from any other
  magnification back to the fitted size.
- **Previous image and next image**, `←` and `→` by default: the image before or after the current one in the order
  the drawer lists them, stopping at either end. The drawer scrolls to keep the current image in view.
- **Arrow keys stay with the control that uses them**: a focused slider, the comparison toggles, the split's divider,
  tabs, lists and menus keep their arrow keys; the shortcut acts only when nothing that moves by arrows has the focus.
- **Keyboard shortcuts list**, `?` by default: a dialog listing every action with the user's current bindings, as the
  user presses them on this platform, with a line pointing to Settings > Keyboard to change them.
- **Tooltips name the shortcuts**: the zoom-out and zoom-in buttons show a tooltip with their name and the user's
  first binding (`Zoom in (⌘ =)`), and the Keyboard settings page says how to open the list.
- **The new actions are ordinary actions**: they appear on the Keyboard settings page, can be rebound, cleared and
  reset like the first four, and are stored and repaired by slice 2's rules.
- **i18n:** five action names, the list's title, description and hint, and the tooltip format, in all 14 locales.
- **Not in this slice:** cycling the comparison mode from the keyboard (named in slice 1's plan, dropped from the
  series in slice 2's; see design.md - Non-Goals); a menu bar with accelerators.

## Capabilities

### New Capabilities

(none --- `gui-shortcuts` is introduced by slice 1 and extended by slice 2; neither is archived yet, so this change
adds requirements to it.)

### Modified Capabilities

- `gui-shortcuts`: three more kinds of keyboard action (actual pixels and its toggle, previous/next image, the
  shortcuts list), arrow keys left to the control that uses them, and the shortcuts named in tooltips and a list.

## Impact

- **Crates touched: `gui` only, and only its frontend.** No Rust, no Tauri command, no capability change. Zoom, the
  current image and the list are view state only the GUI has; `opai` knows nothing of any user interface and the CLI
  has no interactive view.
- **`rust-sak`:** checked; it has no keyboard, input or GUI module, and none is warranted. Not used, not changed.
- **No new npm dependency.** The list is the existing Radix dialog; the tooltips are the existing shadcn tooltip.
- **Persisted state:** none new. A user's stored bindings from slice 2 keep working; the five new actions read their
  defaults until changed.
- **Files:** `lib/shortcuts.ts`, `hooks/useShortcuts.ts`, `stores/transform.ts`, `stores/files.ts`, a new
  `stores/shortcutsList.ts`, `features/preview/PreviewImage.tsx`, `features/drawer/{DrawerHeader,DrawerBody}.tsx`,
  `features/settings/KeyboardPage.tsx`, a new `features/shortcuts/ShortcutsDialog.tsx`, `App.tsx`, the 14 locale
  files, and their tests.
