> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

With the image drawer unfolded, adding an enhancement or exporting folds it away, so a user working through a set of
photographs has to unfold the strip again after every action. The drawer folds on *any* press outside itself - the
sidebar's Add enhancement and the navbar's Export included - which is what the reference's `ClickAwayListener` did and
what `gui-shell` currently specifies, but it punishes the commonest workflow: pick an image in the strip, change its
enhancements, export, pick the next.

## What Changes

Task estimate: **5 tasks**, one change, not part of a series.

- Only a press on the preview canvas folds the unfolded drawer. Presses on the sidebar, the navbar, and any dialog,
  menu or toast leave it as it is. **Behaviour change** against upstream and the current `gui-shell` spec.
- The fold control, closing the last image and the self-unfolding on new images are unchanged.

## Capabilities

### New Capabilities

### Modified Capabilities

- `gui-shell`: "Clicking away folds the drawer" narrows from any press outside the drawer to a press on the preview
  canvas.

## Impact

- Crates: `gui` frontend only (`features/drawer/Drawer.tsx` and its test). No Rust, no `opai`, no `cli`.
- `rust-sak`: not applicable (frontend behaviour).
- No new dependencies.
- Base: `main@upstream`. The code is identical there
  (`jj file show -r main@upstream crates/gui/frontend/features/drawer/Drawer.tsx` matches the integration build byte
  for byte), so this is a small, self-contained upstream PR.
