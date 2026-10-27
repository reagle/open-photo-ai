## 1. Backend

- [ ] 1.1 Add `mask: Option<opai::Mask>` to the `enhance` and `export` commands and their requests, set `ProcessOptions::mask` (absent for an empty mask); tests in `enhance/run.rs` and `export/run.rs` with the fake backend: a masked request reaches `process` with the mask, an empty one without (design.md - G4)
- [ ] 1.2 Add `src/mask.rs`: the `mask_preview(mask, width, height)` command (rasterise, 8-bit grey PNG, binary response, size capped at 2048), registered in `lib.rs`; tests: the PNG's size and a painted pixel's value (design.md - G4)

## 2. Frontend

- [ ] 2.1 Add `ipc/mask.ts` (`Mask`, `Stroke` types in the wire's camelCase, `maskPreview`) and the `mask` argument in `ipc/enhance.ts` and `ipc/export.ts`; tests with the IPC mocked
- [ ] 2.2 Add `stores/mask.ts` (per path: strokes, inverted, undo and redo stacks capped at 100, version counter; global tool settings; `editing`; `registerFileOwner`) and `lib/mask.ts` (point decimation at 0.1 r, fraction conversion, wire conversion, emptiness) (design.md - G2, G7); tests: undo and redo, Clear and Invert undoable, a new stroke clears redo, the cap, decimation
- [ ] 2.3 Add `features/mask/MaskSection.tsx` and `MaskPanel.tsx` in `Sidebar.tsx` under the enhancement list (tools, sliders, hint, Done), hidden with an empty stack (design.md - G1); tests for the section's two states and the panel's controls
- [ ] 2.4 Add paint mode to `PreviewImage.tsx`: an unmodified primary press inside the photograph starts a stroke, moves append, release commits; Option/Alt erases; Space held pans; pointer capture and clamping; both panes paint; paint mode and the focus pick (where present) exclusive (design.md - G2); tests: a drag paints a stroke at the right fractions, a Space drag pans, a press outside the box paints nothing, Escape leaves
- [ ] 2.5 Add `MaskOverlay.tsx` (tint through a CSS `mask-image` of `mask_preview`'s PNG at the drawn box's size, one request in flight, the live stroke on a canvas) and `BrushCursor.tsx` (radius and core circles at the current zoom, erase sign) (design.md - G3); tests: the overlay requests a preview after a stroke and drops a superseded one; the cursor's radius follows zoom
- [ ] 2.6 Pass the mask in `hooks/useEnhancementRun.ts` (version-counter comparison, committed strokes only) and in `features/export/batch.ts` (design.md - G5, G6); tests: a committed stroke starts a run with the mask, a stroke in progress does not, export requests carry each photograph's mask
- [ ] 2.7 Clear the mask on a crop change with the crop panel's notice and the toast whose Undo restores crop and mask together (design.md - G7); tests for the notice, the clearing and the joint undo
- [ ] 2.8 Add the editing keys (`B`, `E`, `X`, brackets with and without Shift, undo and redo, `O`, Escape, Space held), ignored while a text field, dialog or menu has focus (design.md - G8); tests for each key and for the focus rule
- [ ] 2.9 Add the English keys of design.md - G9 to `en.json` and translate them into the other 13 locales; verify `pnpm test i18n` (parity) is green

## 3. Checks

- [ ] 3.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and in `crates/gui` `pnpm check`, `pnpm typecheck` and `pnpm test`
- [ ] 3.2 Build the app (a temporary merge with the fork's `integration` and `just build-fork gui`, or `pnpm tauri build` here), measure a stroke's re-run time on a 24 MP photograph with sharpen and a 2x upscale in the stack, and write the hand checks into `opai-notes/TESTING.md` (Masking section)
- [ ] 3.3 After Joseph's checks, record the results and any fixes in design.md; note Windows and Linux as untested
