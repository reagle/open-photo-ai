> **AI disclosure:** this change was planned and implemented with Claude Code (Opus 5.5, medium effort) at Joseph
> Reagle's behest; Joseph reviews and is responsible for it, and has not yet tested it in practice.

## Why

Slices 1-4 gave the canvas and the application keys, but the editor itself still needs the pointer: turning Autopilot
on or off, adding an enhancement, opening or removing one, Crop/Rotate, the comparison mode, the drawer, revealing the
file and panning a magnified photograph. Topaz Photo AI and Lightroom Classic users expect single keys for most of
these, and a keyboard user cannot reach some of them comfortably at all (the remove button is drawn only on hover or
focus).

## What Changes

Task estimate: **12 tasks**. This is the fifth slice of the keyboard series, `add-gui-shortcuts-5-editor-actions`,
stacked on `add-gui-shortcuts-4-app-actions`. Dialog-local keys are a possible sixth slice (design.md - Slices).

- New rebindable actions with these defaults (agreed with Joseph on 2026-10-24):

  | Key | Action |
  |---|---|
  | `A` | Turn Autopilot on or off |
  | `E` | Open the Add enhancement menu, focus on its first entry |
  | `Delete` / `Backspace` | Remove the enhancement whose row has the focus |
  | `1`-`7` | Open the options of the enhancement in that place in the list |
  | `C` | Open Crop/Rotate |
  | `V` | Next comparison: full, side by side, split, full |
  | Mod+`\` | Show or hide the drawer's images |
  | Mod+Shift+`R` | Show the current image in Finder, Explorer or the file manager |
  | Mod+arrow | Pan a magnified photograph the way the arrow points |

- Each acts as its control does and is unavailable whenever that control is (Add enhancement and Crop/Rotate with no
  image, the comparison and the fold with nothing open). A key with nothing to act on is left alone rather than
  prevented. Toggles, Delete and the digits act once per press, not on key repeat.
- All of them are slice 2's ordinary actions: listed on the Keyboard settings page and in the `?` list, rebindable,
  stored, repaired and reset; slice 1's guard applies (no action in a text field, under an open dialog or menu, or on a
  focused control that uses the key).
- The crop button and the drawer's fold toggle name their key in a tooltip; the Autopilot switch, Add enhancement, the
  remove buttons and the comparison group carry `aria-keyshortcuts`. The shortcuts list scrolls.
- Keyboard hints: every on-screen control that has a key reveals it on hover and focus - the controls of this slice, the
  Export, Settings and Add images buttons, and the file menu's items - following the user's rebinding and the platform.
- Cycling the comparison mode, which slice 3 left out, is added here with `V`.

## Capabilities

### New Capabilities

### Modified Capabilities

- `gui-shortcuts` (added by slice 1, not yet archived): adds the editor actions, their defaults and when each does
  nothing.

## Impact

- Crates: `gui` frontend only (`lib/shortcuts.ts`, a new `lib/editorActions.ts`, three small stores, the sidebar, the
  enhancement list and rows, the drawer header, the canvas, the file options menu, the shortcuts list, 14 locale
  files). No Rust: none of the keys is a macOS menu item. Nothing a CLI needs, so nothing in `opai`.
- `rust-sak`: nothing relevant (no UI module); nothing to extend.
- No new crates.
