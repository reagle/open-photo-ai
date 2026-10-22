## 1. Bindings

- [x] 1.1 In `crates/gui/frontend/lib/shortcuts.ts` add `refusal(binding, mac)` (design.md - D5), `overlaps(a, b, mac)` built on `matches` over the 16 modifier combinations (D4), `bindingFromEvent(event, mac)` (D6, `undefined` for a lone modifier, `Dead`, `Unidentified`, `Process`) and `formatBinding(binding, mac, spaceName)` (D6); add `lib/shortcuts.test.ts` cases for each refusal on both platforms, `Mod+=`/`Meta+=` and `?`/`Shift+?` and `\`/`Alt+\` overlapping, recording Command/Control/AltGr/Shift on both platforms, and both display forms; verify `pnpm test lib/shortcuts` is green
- [x] 1.2 Add `repairShortcuts(value)` (platform-independent, D2) and `resolveShortcuts(stored, mac)` (D3) to `lib/shortcuts.ts`; add tests for junk, unknown actions, malformed and refused strings, duplicates, the cap of four, `[]` as a choice, a stored binding beating another action's default, and two stored actions overlapping; verify `pnpm test lib/shortcuts` is green

## 2. Persistence

- [x] 2.1 Add `shortcuts: Partial<Record<ShortcutAction, string[]>>` to `SettingsData` in `crates/gui/frontend/stores/settings.ts` (`{}` in `settingsDefaults`, carried by `settingsData`, repaired by `repairShortcuts` in `merge`) and a fifth `keyboard` page with `keys: ["shortcuts"]` in `features/settings/pages.ts`; add `stores/settings.test.ts` cases that a saved binding survives `persist.rehydrate()` and that a stored `Escape` or unknown action is dropped while the rest is kept; verify `pnpm test stores/settings features/settings/pages` and `pnpm typecheck` are green

## 3. The bindings in force

- [x] 3.1 Make `crates/gui/frontend/hooks/useShortcuts.ts` read `resolveShortcuts(useSettingsStore.getState().shortcuts, mac)` on each `keydown`, add a `useShortcutBindings()` hook, and make `features/drawer/DrawerHeader.tsx` derive `aria-keyshortcuts` from it; add tests that a saved rebinding acts at once without remounting, that the old default no longer acts, that an action bound to `[]` does nothing, and that the zoom-in button reports a rebinding; verify `pnpm test hooks/useShortcuts features/drawer` is green

## 4. The Keyboard page

- [x] 4.1 Add `crates/gui/frontend/features/settings/KeyboardPage.tsx`: one row per action in `SHORTCUT_ACTIONS` order with its bindings as chips (`formatBinding`), a remove button per chip named with the binding and the action, "No shortcut" for an empty list, and Add shortcut; edits go through the draft's `update`; add `KeyboardPage.test.tsx` for the listing on both platforms, removal and the empty state; verify `pnpm test features/settings/KeyboardPage` is green
- [x] 4.2 Make Add shortcut a recorder (D6): focus kept, `aria-live` prompt, a lone modifier ignored, `Escape` ending it without closing the dialog through `onEscapeKeyDown` in `SettingsDialog.tsx` and `data-shortcut-recorder`, `Tab` and blur ending it, a refused key shown with its reason (D5), and a conflict shown with *Use here instead* (D7); add tests for each, including Escape inside the mounted dialog leaving it open; verify `pnpm test features/settings` is green
- [x] 4.3 Mount the page in `SettingsDialog.tsx`'s `PAGE_BODIES` and update the "four pages" comments in `SettingsDialog.tsx`, `SettingsNav.tsx` and `pages.ts`; add `SettingsDialog.test.tsx` cases that the nav lists five pages, that Reset to defaults on Keyboard restores every default and leaves other pages' drafts alone, that Save applies a rebinding and that Cancel discards it; verify `pnpm test features/settings` is green
- [x] 4.4 Add the i18n keys (page title and description, the four action names, Add shortcut, the recording prompt, No shortcut, the remove label, both refusal messages, the conflict message, Use here instead, and the name of the `Space` key) to all 14 locale files under `crates/gui/frontend/i18n/locales/`; verify `pnpm test i18n` (parity) is green

## 5. Verification

- [x] 5.1 In `crates/gui`, run `pnpm install --frozen-lockfile`, `pnpm check`, `pnpm typecheck`, `pnpm test` and `pnpm build`, then `cargo clippy -p gui --all-targets -- -D warnings` and `cargo test -p gui` (no Rust changes, run to prove the bundle still builds against the crate); verify all are green
- [x] 5.2 Verify by hand on macOS: rebind zoom in to `Shift Z`, save, and it zooms while `Cmd =` does not; restart the application and the binding is still there; `Escape` while recording keeps the dialog open; `Cmd Q` and `Tab` are refused as specified; Reset to defaults on Keyboard and Save restores `Cmd =`; record in this file which of Windows and Linux were tried (checked by hand by Joseph on macOS, 2026-10-22)
  - Not yet done (2026-10-22): the implementing agent could not drive a real window. Automated coverage only (jsdom),
    including Save followed by a rehydrate from `localStorage`. Windows and Linux: untested.
