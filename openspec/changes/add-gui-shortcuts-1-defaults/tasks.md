## 1. Bindings

- [x] 1.1 Add `crates/gui/frontend/lib/shortcuts.ts` with `ShortcutAction` (`zoomIn`, `zoomOut`, `fit`, `showOriginal`), `DEFAULT_SHORTCUTS` (design.md - D1), `parseBinding` and `matches(event, binding, mac)` (`Mod` = `metaKey` on macOS, `ctrlKey` elsewhere; the other platform modifier must be up; `=`/`+` and `-`/`_` equivalent; compared on `event.key`) (design.md - D2); add `lib/shortcuts.test.ts` covering each default on both platforms, Shift and keypad variants, the wrong platform modifier, an extra Alt, and a malformed string; verify `pnpm test lib/shortcuts` is green
  - 2026-10-24: `Space` removed from `showOriginal`'s defaults, leaving `\` (design.md - D1: a closed Radix menu
    returns the focus to its trigger button, which takes `Space`, so the next presses reopened Add enhancement and
    added its first item). Tests now check that `Space` is unbound by default; the guard and its tests are unchanged.
- [x] 1.2 Add `isWindowShortcut(event)` to `lib/shortcuts.ts`, the guard of design.md - D4 (editable targets, an open `dialog`/`alertdialog`/`menu`, `Space` on a button/checkbox/slider/tab/menu item); add tests for each case and for `\` on a focused button passing; verify `pnpm test lib/shortcuts` is green

## 2. Actions

- [x] 2.1 Add `stepZoom(direction)` and `fitCurrent()` to `crates/gui/frontend/stores/transform.ts` (design.md - D5), and make `DrawerHeader.tsx`'s minus and plus buttons call `stepZoom`; add tests in `stores/transform.test.ts` for a step, both ends of the range, fit from a panned transform, and no current file; verify `pnpm test stores/transform features/drawer` is green with no existing expectation changed
- [x] 2.2 Add `crates/gui/frontend/stores/peek.ts` (`showingOriginal`, `setShowingOriginal`), not persisted (design.md - D3), with `stores/peek.test.ts`; verify `pnpm test stores/peek` is green

## 3. The hook

- [x] 3.1 Add `crates/gui/frontend/hooks/useShortcuts.ts`: one `keydown`/`keyup` listener pair on `window`, dispatching through `matches` and `isWindowShortcut`, `preventDefault()` on handled events, `showOriginal` set on keydown (ignoring `repeat`) and cleared on keyup, `blur` and `visibilitychange`; add `hooks/useShortcuts.test.tsx` with `isMacOs` mocked both ways, covering each action, repeat, blur release and a key in a text field; verify `pnpm test hooks/useShortcuts` is green
- [x] 3.2 Mount `useShortcuts()` once in `crates/gui/frontend/App.tsx`; add an `App.test.tsx` case that `Mod+=` with an image open raises the drawer slider's value by one step; verify `pnpm test App` is green

## 4. The canvas and the controls

- [x] 4.1 In `crates/gui/frontend/features/preview/PreviewImage.tsx`, read `showingOriginal` and, while set, draw `source` in the enhanced pane and label it Original (reusing `preview.pane.original`, so no new i18n key); add `PreviewImage.test.tsx` cases for full, side and split with a result present, and that the transform and divider are unchanged; verify `pnpm test features/preview` is green
- [x] 4.2 In `crates/gui/frontend/features/drawer/DrawerHeader.tsx`, set `aria-keyshortcuts` on the zoom-out and zoom-in buttons from `DEFAULT_SHORTCUTS` (`Meta+=`/`Meta+-` on macOS, `Control+=`/`Control+-` elsewhere, via one formatter in `lib/shortcuts.ts`); add tests for both platforms; verify `pnpm test features/drawer` is green

## 5. Verification

- [x] 5.1 In `crates/gui`, run `pnpm lint`, `pnpm typecheck` and `pnpm test`; verify all three are green (no Rust changed, so `cargo` checks are not required by this change)
- [x] 5.2 Verify by hand on macOS with `fixtures/test.dat` enhanced: `Cmd =`/`Cmd -` step the zoom and move the slider, `Cmd 0` fits, holding `Space` and `\` show the original in full, side and split and release restores, `Cmd-Tab` while holding restores on return, `Space` on a focused button activates it, nothing fires in the settings dialog's number field or with the crop dialog open, and the webview itself never zooms; record in this file which of Windows and Linux were tried and which remain untested (checked by hand by Joseph on macOS, 2026-10-22)
  - Not yet done (2026-10-22): the implementing agent could not drive a real window. Automated coverage only (jsdom).
    Windows and Linux: untested.
  - 2026-10-24: the hand check above held `Space` as a default; hand testing then found that `Space` after picking an
    enhancement reopened the menu and added one, so `Space` is no longer a default (see 1.1). Recheck with `\` only,
    and that `Space` after picking from Add enhancement does nothing unless the user has bound it.
