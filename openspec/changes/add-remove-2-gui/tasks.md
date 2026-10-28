## 1. Rust

- [x] 1.1 Add `mask: Option<opai::Mask>` to `Requested`; in `resolve`, hand it to `ParameterValues::with_mask` when the row publishes a parameter of kind `ParameterKind::Mask`, and refuse it as `UnknownOperation::Parameter { name: "mask" }` for a row that publishes none; tests: the painted region is the library's own `Remove::lima`, an empty or absent region resolves to a removal of nothing, a region sent to an upscale is refused, every catalogue row taking a mask is handed it by kind (design.md - R4)

## 2. Frontend: the store and the wire

- [x] 2.1 Give `stores/mask.ts` a `Target` (`limit`, `removal`): a `removals` map beside `masks`, `editing` and the live stroke carrying their target, the region actions taking an optional `target` defaulting to the limiting mask, `forgetTarget`, and `forget`/`forgetAll` for both; tests: strokes go to the region being edited, a stroke stays in the region it began in, separate undo histories, take and restore by region, forgetting one region, closing files (design.md - R1)
- [x] 2.2 Add `"remove"` to `Family` and `{ kind: "mask" }` to `ParameterKind` (`ipc/catalogue.ts`); `mask?: Mask` on `Operation` (`ipc/enhance.ts`); `regionOf` and `withRemoval` in `lib/mask.ts`; update the catalogue fixture (`test/support.tsx`: Lima, the orders shifted); tests: an empty region is still sent, rounding, never inverted, only the Remove carries it, the wire keeps the region on the operation and the limiting mask apart (design.md - R4)

## 3. Frontend: the enhancement, the panel and the preview

- [x] 3.1 Add Remove first to `ENHANCEMENTS` with `painted: true` and the `Bandage` icon; `SETTABLE` and `familiesWhere` leave it out of the settings table and of Autopilot; `isPainted`; update the list, order and Autopilot tests (design.md - R2)
- [x] 3.2 Open the removal brush when Remove is added (`AddEnhancement.tsx`); give its row a toggle for the brush instead of a popover and an info line saying whether anything is painted (`EnhancementRow.tsx`); forget its strokes when it is taken off (`EnhancementList.tsx`); tests (design.md - R2)
- [x] 3.3 `MaskPanel` and `useMaskKeys` take a target: the removal's title, no Invert, its hint, undo/redo/clear on the region; `MaskSection` draws the removal's panel in its place, offers the limiting mask only for a stack with an enhancement that is not painted, and stops painting a region the stack has nothing left for; tests (design.md - R3)
- [x] 3.4 `--removal` theme token; `MaskOverlay` shows the region being edited in its own colour, live strokes included; a painting test in `PreviewImage.test.tsx` (design.md - R6)
- [x] 3.5 `useEnhancementRun` sends the settled region on the Remove (300 ms, keyed by photograph, at once when taken away or cleared) as a run dependency; tests: sent empty before painting, one run per pause, a run in flight stopped, a clear at once, never another photograph's region, independent of the limiting mask (design.md - R5)

## 4. Frontend: export, crop, strings

- [x] 4.1 The export batch puts each photograph's region on its Remove; test (design.md - R7)
- [x] 4.2 A crop change takes both regions with one toast and one Undo, and the crop dialog's notice names what will be cleared; tests (design.md - R8)
- [x] 4.3 The new keys in all 14 locales (design.md - R9)

## 5. Checks

- [x] 5.1 `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p gui -p opai`, and in `crates/gui` `pnpm test`, `pnpm typecheck`, `pnpm lint`; `just build gui` bundles
- [ ] 5.2 Joseph's hand checks (`~/dev/opai-notes/remove/README.md` - GUI): add Remove, paint a tag away, undo, erase, clear, crop, export, the limiting mask beside it; record the results and any fixes here
- [ ] 5.3 Decide open questions 1 to 3; note Windows and Linux as untested
