> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5, high effort) at my behest; I tested its application in
> practice.

## Why

Slice 1 makes Remove an enhancement the library and the CLI can run, with a mask file. In the GUI a removal is painted:
the user brushes over the tag or the passer-by and sees it gone. The masking series already built the brush, the
overlay and the stroke store; this slice gives Remove a brush of its own on top of them.

## What Changes

Task estimate: about **12**. Slice 2 of the `add-remove` series (see `add-remove-1-core/design.md` - Slices); its own
decisions are design.md R1 to R9.

- **Remove in the enhancements list**, first, as the catalogue orders it (order 0), with one model, Lima. Adding it
  opens its brush; it does nothing until something is painted (slice 1 drops an empty removal from the chain).
- **Its own region**: a second region in the masking store (`stores/mask.ts`, a `Target` of `limit` or `removal`) keyed
  by photograph, painted with the same panel, cursor and keys (paint, erase with Option, size, softness, undo and redo,
  clear), titled "Paint area to remove", with no Invert and a magenta tint so it is not mistaken for the limiting mask.
  Both can be edited, one at a time; taking Remove off the stack forgets its strokes.
- **On the wire**: the requested operation carries the strokes (`Requested::mask`, the library's `Mask` wire shape); the
  Rust side sets them with `ParameterValues::with_mask` before `VariantEntry::build`, as faces are injected. The catalogue
  says which parameter is a mask (`ParameterKind::Mask`), so no family name is spelled in the window.
- **Preview**: strokes are committed on pointer-up and the preview re-runs 300 ms after the last one; the removal is the
  first step, so everything after it recomputes, and a first run opens the model on the CPU (about 1 to 2 s per
  region).
- **Export**: each photograph's removal strokes go with its export.
- **A crop change clears the removal strokes** as it clears the limiting mask's, with the same notice and Undo.
- **i18n**: the new keys in all 14 locales.

Crates touched: `gui` (Rust: the `mask` field on `Requested`; frontend: the store, the panel's mode, the row, the tint),
nothing in `opai`.

## Capabilities

### Modified Capabilities

- `gui-enhance`: Remove is painted with a brush and applied to the preview and the export.

## Impact

- `gui`: `src/enhance/operation.rs` (`Requested::mask`), frontend `features/mask/` (a mode for the panel and overlay),
  `stores/` (the removal strokes per photograph), the enhancement row, the export batch, `i18n/locales/*.json`.
