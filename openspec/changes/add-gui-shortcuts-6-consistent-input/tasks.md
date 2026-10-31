## 1. Live surfaces and the window's keys

- [x] 1.1 Add `LIVE_SURFACE`, `BLOCKING_LAYER` and `KEEPS_LIVE_SURFACE` to `lib/layers.ts`, with the two kinds of surface written at the top of it; add `windowShortcutScope` and `VIEW_ACTIONS` to `lib/shortcuts.ts`, keeping `isWindowShortcut` as its `"all"` case for the application's actions; make `hooks/useShortcuts.ts` let only the view's actions through while a live surface is open; tests in `hooks/useShortcuts.test.tsx` (panel open: `Mod =`, `Mod 0` from a slider and `\` act, `z` in a field and `C` do not; a dialog over the panel blocks)
- [x] 1.2 Mark the options panel a live surface, and keep it open for a press or a focus on the preview's canvas or the drawer's zoom (`onInteractOutside`); tests in `EnhancementList.test.tsx` that a press inside a marked region keeps it open and one elsewhere closes it

## 2. Return

- [x] 2.1 Add `returnAccepts`, `acceptOnReturn` and `pressPrimaryOnReturn` to `lib/layers.ts`; call the last from `components/ui/dialog.tsx`'s content, and mark Save in Settings and Export, Apply in Crop/Rotate and Select faces with `DIALOG_PRIMARY`; tests in `lib/layers.test.ts`, `SettingsDialog.test.tsx` (Return saves; not on a focused Cancel, not while recording, not mid-composition), `CropRotate.test.tsx` (Return presses Apply, from the size field after committing it) and `ExportDialog.test.tsx` (Save while idle only)
- [x] 2.2 Make Return Done on the live surfaces: the options panel (on the window, since the focus is often outside it), the brushes (`useMaskKeys`) and Set focus; tests for each

## 3. The wheel

- [x] 3.1 Pan on a plain wheel event (coalesced into the drag's per-frame write, in pixels whatever `deltaMode`, Shift for sideways), zoom proportionally on a pinch or the modifier with the wheel (`ZOOM_PINCH_RATE`, `ZOOM_PINCH_MAX_DELTA`); tests in `PreviewImage.test.tsx`
- [x] 3.2 Add the `wheelZooms` preference (store, defaults, repair, the General page's row, 14 catalogues) that puts back the reference's fixed-step zoom on a plain wheel; the existing wheel tests now run under it

## 4. Space

- [x] 4.1 Move the held Space out of the mask store into `stores/panKey.ts`, heard by `features/preview/useSpacePan.ts` for as long as the preview is drawn; a press pans and never picks while it is held, and the cursor is a hand; tests in `useSpacePan.test.tsx` and `PreviewImage.test.tsx`

## 5. Documentation and checks

- [x] 5.1 Add the input section to the shortcuts list (`features/shortcuts/rows.ts`, `ShortcutsDialog.tsx`), following the setting; tests in `ShortcutsDialog.test.tsx`
- [x] 5.2 In `crates/gui`, run `pnpm typecheck`, `pnpm lint` and `pnpm test`; verify all are green (no Rust changed)
- [ ] 5.3 Verify by hand on macOS: with an options panel open, `Cmd =`/`Cmd -`/`Cmd 0`/`Cmd 1`/`Z`/`Cmd` arrows/`\` act and the panel stays open; dragging the preview and the drawer's zoom buttons keep it open; a click on the sidebar or Escape or Return closes it; a two-finger scroll pans, a pinch zooms smoothly (judge the sensitivity), `Cmd`+mouse wheel zooms; Scroll wheel zooms on restores the old wheel; Space-drag pans with Set focus armed and in each brush, and Space on a focused button presses it; Return saves Settings and Export, applies Crop/Rotate and Select faces, and does nothing while recording a binding
