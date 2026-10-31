> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code, Opus 5.5). Joseph reviews and is responsible for it.

## Why

The keyboard shortcut series (slices 1 to 5) gave the window its keys, but the surfaces drawn over the photograph each
answered the keyboard and the pointer in their own way. An enhancement's options panel is a Radix popover, whose content
is a dialog, so while it was open every window key was off: no zoom, no fit, no pan, no glance at the original ---
exactly when the user wants to see what a slider did. A press on the preview, or on the drawer's zoom, dismissed the
panel. Every wheel event zoomed by a fixed step, the reference's parity, so a two-finger scroll on a trackpad magnified
and nothing panned while a panel was open. Space panned only inside a brush. Escape was Cancel in the draft dialogs and
Done on the live surfaces, which is right, but Return did nothing anywhere.

Photos, Preview and Lightroom agree on the rules this change adopts, and writing them down as a requirement is what
keeps the next surface from inventing a third set.

## What Changes

**In `crates/gui/frontend` only.** Estimate: **about 12 tasks**, one slice.

- **Two kinds of surface.** A *draft dialog* (Settings, Export, Crop/Rotate, Select faces) holds changes until its
  primary button: Escape is Cancel, as before, and **Return is the primary button** (Save, Apply), unless the focus is
  on a control Return operates itself (a button, a link, a text area, a select, a menu or listbox, a switch), an input
  method is composing, or the Keyboard page is recording a binding. Return in a single-line field commits the field and
  then accepts. A *live surface* (an enhancement's options panel, the Remove, Refine and limiting-mask brushes, Set
  focus) changes the photograph as it is used: **Escape and Return both mean Done**, keeping what was done. One shared
  helper (`lib/layers.ts`) decides both.
- **The photograph stays operable under a live surface.** While only live surfaces are open, the view's keys --- zoom in
  and out, fit, actual pixels, toggle zoom, the four pans and show original --- still act; every other window key waits,
  and a dialog, menu or listbox still takes every key. A press on the preview or on the drawer's zoom leaves an options
  panel open; a press anywhere else, or Escape, closes it.
- **Trackpad and mouse as in Photos, Preview and Lightroom.** A pinch (which the webview delivers as a wheel event with
  Control) zooms by an amount proportional to the pinch, clamped per event, about the point under the pointer; Command
  or Control with a mouse's wheel zooms the same way. A plain wheel event or two-finger scroll **pans**, in both
  directions, in pixels whatever unit the event counts in. A new setting on the General page, **Scroll wheel zooms**
  (off by default), puts back the reference's plain wheel, every event zooming by a fixed step. Side by side and split
  keep sharing one transform.
- **Space held is the pan modifier everywhere on the preview**, not only in a brush: while it is down a press pans
  (never a stroke, never a focus pick) and the cursor is a hand. It is left to a focused button, switch, radio, link or
  text field, and to the action a user bound a bare Space to, except while a brush is up. It is not a default *binding*,
  for the reason slice 1 gives (design.md - D1 of `add-gui-shortcuts-1-defaults`).
- **The shortcuts list documents the rules**, in a fixed last section: Return, Escape, pinch and the modifier with the
  wheel, scroll, drag and Space with a drag, as the setting has them now. The setting's label is in all 14 catalogues.

## Capabilities

### Modified Capabilities

- `gui-shortcuts`: adds the input rules every surface over the photograph follows, as a requirement; changes when a
  window key is left to a surface (a live surface keeps the view's keys for the window).
- `gui-settings`: adds the Scroll wheel zooms preference on the General page.

## Impact

- **Crates touched: `gui` only, and only its frontend.** The preference is persisted with the others in the settings
  store's own storage and is not one the command line follows, so nothing round-trips through Rust and no Rust file
  changes.
- **Files:** `lib/layers.ts` (the two kinds of surface, the Return helper), `lib/shortcuts.ts` (`windowShortcutScope`,
  `VIEW_ACTIONS`), `hooks/useShortcuts.ts`, `components/ui/dialog.tsx`, `features/enhancements/OptionsPanel.tsx`,
  `features/preview/PreviewImage.tsx`, new `features/preview/useSpacePan.ts` and `stores/panKey.ts` (the held Space,
  moved out of the mask store), `features/mask/useMaskKeys.ts`, the four dialogs' primary buttons,
  `features/settings/GeneralPage.tsx`, `stores/settings.ts`, `features/shortcuts/`, the 14 catalogues, and their tests.
- **Upstream:** a change of the reference's wheel behaviour, which is why the reference's behaviour is one switch away;
  held for an issue first, as the rest of the series is.
