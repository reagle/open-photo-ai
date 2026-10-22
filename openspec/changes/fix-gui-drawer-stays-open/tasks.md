## 1. Fix

- [x] 1.1 In `crates/gui/frontend/features/drawer/Drawer.tsx` fold only for a `pointerdown` inside `[data-slot="preview-canvas"]` and outside the drawer (design.md - D1), and rewrite the comment block above the listener and the component's doc comment to match; verify `pnpm typecheck` is green
- [x] 1.2 In `features/drawer/Drawer.test.tsx` replace "folds when the canvas outside it is clicked" (press on `document.body`) with a press inside a `data-slot="preview-canvas"` element, and add cases that a press on a sidebar button, a navbar button and an open dialog leaves it unfolded; verify `pnpm test features/drawer` is green and the menu cases still pass

## 2. Verification

- [x] 2.1 In `crates/gui` run `pnpm check`, `pnpm typecheck`, `pnpm test` and `pnpm build`; verify all are green
- [ ] 2.2 Verify by hand: with three images open and the drawer unfolded, add an enhancement, change its options, open and run Export, open Settings - the drawer stays unfolded throughout; a press on the canvas folds it; record the platform tried (not done by the agent: left for Joseph to try by hand)
- [ ] 2.3 Draft the upstream PR description (behaviour change, the workflow it fixes, the spec delta, the AI disclosure line and Claude Code footer) in `~/dev/opai-notes/` for Joseph to review; verify it names `main@upstream` as base and does not get pushed to `vegidio` without his go-ahead
