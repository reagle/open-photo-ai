## 1. Bindings

- [x] 1.1 In `crates/gui/frontend/lib/shortcuts.ts` add `actualPixels` (`Mod+1`), `toggleZoom` (`z`), `previousImage` (`ArrowLeft`), `nextImage` (`ArrowRight`) and `showShortcuts` (`?`) to `SHORTCUT_ACTIONS` and `DEFAULT_SHORTCUTS` (design.md - D1 to D5), and the arrow-key rule of D4 to `isWindowShortcut`; add `lib/shortcuts.test.ts` cases for each new default on both platforms, `?` typed with Shift, and arrows on a slider, a radio, a separator and a `[data-arrow-keys]` element versus a plain button; verify `pnpm test lib/shortcuts` is green

## 2. Actions

- [x] 2.1 In `crates/gui/frontend/stores/transform.ts` add the published `fit` ratio with its setter, `actualPixels()` and `toggleZoom()` (D1, D2); add `stores/transform.test.ts` cases for a large photograph at device pixel ratios 1 and 2, the clamp at both ends, a ratio published for another photograph, the toggle both ways, and no current file; verify `pnpm test stores/transform` is green
- [x] 2.2 In `crates/gui/frontend/features/preview/PreviewImage.tsx` publish the fit ratio from the pane that publishes the viewport; add a `PreviewImage.test.tsx` case with a stubbed pane size that the published ratio is the fitted width over the photograph's width; verify `pnpm test features/preview` is green
- [x] 2.3 Add `stepImage(direction)` to `crates/gui/frontend/stores/files.ts` (D3); add `stores/files.test.ts` cases for each direction, both ends and nothing open; verify `pnpm test stores/files` is green
- [x] 2.4 Make `crates/gui/frontend/features/drawer/DrawerBody.tsx` scroll the current item into view when `currentIndex` changes (D3); add a `Drawer.test.tsx` case that a change of the current image asks the virtualizer to scroll to it; verify `pnpm test features/drawer` is green

## 3. The hook

- [x] 3.1 Dispatch the five new actions in `crates/gui/frontend/hooks/useShortcuts.ts`, with `showShortcuts` opening a new non-persisted `stores/shortcutsList.ts`; add `hooks/useShortcuts.test.tsx` cases for each, including `→` ignored on a focused slider and `?` with Shift; verify `pnpm test hooks/useShortcuts` is green

## 4. Showing the shortcuts

- [x] 4.1 Add `crates/gui/frontend/features/shortcuts/ShortcutsDialog.tsx` (D5) and mount it once in `App.tsx`; add `ShortcutsDialog.test.tsx` for the listing on both platforms, a rebinding shown, an action with none, and Escape closing it, and an `App.test.tsx` case that `?` opens it; verify `pnpm test features/shortcuts App` is green
- [x] 4.2 Give the zoom-out and zoom-in buttons in `features/drawer/DrawerHeader.tsx` tooltips naming their first binding in force (D6), and add the "Press {{key}} anywhere" line to `features/settings/KeyboardPage.tsx`; add tests for the tooltip text on both platforms and with no binding, and for the line present and absent; verify `pnpm test features/drawer features/settings` is green
- [x] 4.3 Add the i18n keys (the five action names, the list's title, description and hint, the tooltip format, and the Keyboard page's line) to all 14 locale files under `crates/gui/frontend/i18n/locales/`; verify `pnpm test i18n` (parity) is green

## 5. Verification

- [x] 5.1 In `crates/gui`, run `pnpm install --frozen-lockfile`, `pnpm check`, `pnpm typecheck`, `pnpm test` and `pnpm build`, then `cargo clippy -p gui --all-targets -- -D warnings` and `cargo test -p gui` (no Rust changes, run to prove the bundle still builds against the crate); verify all are green
- [x] 5.2 Verify by hand on macOS with a photograph larger than the pane: `Cmd 1` shows actual pixels (compare against Preview.app at 100%), `Z` toggles fit and actual pixels, `←`/`→` move through the drawer and the strip follows, `→` on the focused zoom slider moves the slider only, `?` opens the list with any rebinding shown, and the zoom buttons' tooltips name their keys; record in this file which of Windows and Linux were tried (checked by hand by Joseph on macOS, 2026-10-22)
  - Not yet done (2026-10-22): the implementing agent could not drive a real window. Automated coverage only (jsdom).
    Windows and Linux: untested.
