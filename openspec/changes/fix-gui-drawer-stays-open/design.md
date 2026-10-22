## Context

Root cause, with evidence:

- `crates/gui/frontend/features/drawer/Drawer.tsx:65-94`: while the drawer is open, a `pointerdown` listener on the
  whole `document` calls `setOpen(false)` for any target not inside the drawer's element (line 88:
  `if (!element?.contains(event.target as Node)) setOpen(false);`).
- Add enhancement lives in the sidebar and Export in the navbar; both are outside the drawer, so pressing either folds
  it. Nothing else folds it: the only other writers of `useDrawerStore.open` are the count effect (`Drawer.tsx:44-63`,
  folds only at zero images) and the header's toggle (`DrawerHeader.tsx:97`). No store reset or remount is involved -
  the store is a module-level zustand store (`stores/drawer.ts:27-33`), and adding an enhancement does not change the
  image count.
- This is specified, not accidental: `openspec/specs/gui-shell/spec.md:582-609`, "Clicking away folds the drawer",
  scenario "A click on the canvas": "the user clicks the preview canvas **or the sidebar** - THEN the drawer folds".
  The test `Drawer.test.tsx:371-379` pins it with a press on `document.body`.
- Upstream: `Drawer.tsx` on `main@upstream` is identical to the integration build, so the behaviour is upstream's. It
  ports the Wails reference's `ClickAwayListener` (comment at `Drawer.tsx:68-72`).

## Goals / Non-Goals

**Goals:** the drawer stays unfolded while the user works in the sidebar, navbar and dialogs; a press on the canvas
still folds it, keeping the reason the rule exists (the drawer covers part of the canvas).

**Non-Goals:** remembering the fold across launches; a pin control.

## How the work divides

Frontend only, in `gui`. No logic shared with the CLI.

## `rust-sak` coverage

Not applicable.

## Decisions

### D1. Fold only on a press inside the preview canvas

Change the listener's test from "outside the drawer" to "inside the canvas and outside the drawer":
`target.closest('[data-slot="preview-canvas"]') && !element.contains(target)`. The canvas already carries that
`data-slot` (`features/preview/Preview.tsx:67`). Dialogs, menus and toasts portal to `document.body`, so they are
outside the canvas and no longer fold the drawer either - the existing special case for menus portalled into the
drawer keeps working because they are still inside it.

Alternatives:

- Drop click-away entirely (fold only by the toggle). Simplest, but a user who unfolds the strip and then wants the
  hidden bottom of the photograph back must find the toggle; the canvas press is the natural gesture.
- Exempt specific controls (Add enhancement, Export) with a marker. Fragile: every future sidebar or navbar control
  would need it.

### D2. The rest of the drawer's rules stay as they are

Self-unfolding on a count above one and folding at zero are untouched; the count effect is independent of the
listener.

## Risks / Trade-offs

- [Upstream may prefer the reference's behaviour] -> the PR explains the workflow cost; it is a one-condition change
  easy to decline.
- [A press on the navbar's empty area no longer folds] -> intended; the navbar is not covered by the drawer.
- [Validated against] jsdom tests and reasoning; behaviour is platform-independent (DOM only).

## Difficulty and likely paths

Easy (an hour or two): one condition, three test cases, one spec delta.
