## 1. Bindings and actions (`crates/gui/frontend`)

- [x] 1.1 In `lib/shortcuts.ts` add the actions and defaults of design.md - D1 (`toggleAutopilot`, `addEnhancement`, `removeEnhancement`, `openOptions1`-`7`, `cropRotate`, `cyclePreview`, `toggleDrawer`, `revealImage`, four pans) and macOS's `⌫`/`⌦` in `formatBinding`; add `lib/shortcuts.test.ts` cases for each default on both platforms, no two defaults overlapping, none refused; verify `pnpm test lib/shortcuts` is green
- [x] 1.2 Add `stores/addEnhancementMenu.ts`, `stores/optionsPanel.ts` and `stores/cropDialog.ts` (D2), `nextPreviewMode` in `stores/preview.ts` and `registerPanner`/`panCurrent` in `stores/transform.ts` (D5), reset in `test/setup.ts`; add store tests; verify they are green
- [x] 1.3 Add `lib/editorActions.ts` (D2-D4) and wire it into `hooks/useShortcuts.ts`, with "nothing to do leaves the key alone" and once-per-press for toggles, Delete and digits; add `hooks/useShortcuts.test.tsx` cases (reveal on both platforms, repeat, pans, text field, open menu, a rebinding); verify `pnpm test hooks/useShortcuts` is green

## 2. Components (`crates/gui/frontend`)

- [x] 2.1 `AddEnhancement.tsx` reads the menu store; `EnhancementRow.tsx` reads the options panel store and carries `data-family`; `EnhancementList.tsx` shares `removeEnhancementOf`; add component tests for `E` (first entry focused, arrows move), `2` and Escape back to the row, Delete/Backspace with the focus moving, Delete elsewhere, repeat, analysing; verify `pnpm test features/enhancements` is green
- [x] 2.2 `SidebarMiniature.tsx` reads the crop store, `Sidebar.tsx`, `DrawerHeader.tsx` and `FileOptionsMenu.tsx` use the shared actions; tooltips on the crop button and fold toggle and `aria-keyshortcuts` where D7 says; add tests for `A`, `C`, `V`, Mod+`\`, the tooltips and focus to Add enhancement after the last row; verify `pnpm test features/sidebar features/drawer` is green
- [x] 2.3 Register the panner in `PreviewImage.tsx` (D5); add tests for a step, the edge, after a zoom, fitted, side by side, unmounted; verify `pnpm test features/preview` is green
- [x] 2.4 Let `ShortcutsDialog.tsx` scroll; add tests that the list shows the new actions on both platforms; verify `pnpm test features/shortcuts features/settings` is green
- [x] 2.5 Add the 18 action names to all 14 locale files under `crates/gui/frontend/i18n/locales/`; verify `pnpm test i18n` (parity) is green

- [x] 2.6 Add the keyboard hints (D7): `HintTooltip`, `useShortcutKeys`/`useAppShortcutHint`, hints on the Autopilot switch, Add enhancement, enhancement rows and remove buttons, the comparison toggles, Export, Settings, Add images and the file menu's three items, and `shortcuts.tooltipCycle` in all 14 locales; add component tests for both platforms, a rebinding and an unbound action; verify `pnpm test` is green

## 3. Verification

- [x] 3.1 In `crates/gui` run `pnpm check`, `pnpm typecheck` and `pnpm test`; verify all are green
- [ ] 3.2 Verify by hand on macOS (`~/dev/opai-notes/TESTING.md`, "Keyboard shortcuts 5: editor actions"): every key of D1's table, each with nothing open, in a text field and under an open dialog; Delete's focus moving; `E` then arrows and Enter; the tooltips; record that Windows and Linux are untested
- [x] 3.3 Add the by-hand cases to `~/dev/opai-notes/TESTING.md`; verify the checklist lists every row of D1's table

## Implementation notes and deviations

Implemented on bookmark `add-gui-shortcuts-5-editor-actions` (one commit on `add-gui-shortcuts-4-app-actions`).

- No conflict was found for any agreed key (design.md - D1), so the table is unchanged.
- 1.1: seven separate `openOptions` actions rather than one action with fixed digits, so each is rebindable like
  every other action; the list and the Keyboard page therefore have seven rows for them.
- 2.2: the comparison toggles first kept their tooltips as they were (the mode's name), with `V` named once, on the
  group, for assistive technology; 2.6 now adds "(V cycles)" to each toggle's tooltip.
- 2.2: every component test that now mounts a control with a shortcut hint mocks `@/ipc/os`, as the drawer's tests
  already did; the hint asks the platform to write its keys.
- 3.1: also `pnpm build`, green. The Rust checks were not run: no Rust changed. Counts at commit: 116 test files,
  1710 tests, all green; biome clean; typecheck clean. The `openspec` CLI is not installed here, so `openspec
  validate` was not run.
