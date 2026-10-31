> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I have not yet tested its application in
> practice.

## Why

A photograph has one limiting mask (masking slices 1 to 3), and it limits every enhancement at once. The commonest
retouch asks for something else: **sharpen the subject and denoise the background**, brighten a face but not the sky,
restore a weathered wall but not the faces in front of it. Issue #29 asked for "the subject to apply enhancements to,
specifically sharpening"; the series answered with the stack-wide mask and set the per-enhancement case aside as slice 6
(`add-masking-1-core/design.md` - Slices, D1). This is that slice.

Two other regions already belong to one enhancement each: Remove's region (`Remove { mask }`, `rm-…-m<signature>`) and
the lens blur's Refine strokes (`LensBlurRefine`, `-r<signature>`). What is missing is a region for the faithful
enhancements in between, which only says **where that enhancement applies**.

Fork work: whether to offer it upstream is Joseph's call later (HOLD: no upstream issue or PR now).

## What Changes

Task estimate: **24** over three slices (see design.md - Slices).

- **A step mask per enhancement**: Denoise, Colorization, Light Adjustment, Color Balance, Sharpen, Restore at 1x and
  Grain may each carry an `opai::Mask`. Such a step's result is blended with its own input, `out = m x step(prev) + (1 - m) x
  prev`, before the next step reads it. Remove (its region is its mask), Face Recovery (its face choice is its region),
  Upscale and an enlarging Restore (a step that resizes takes none) and Lens Blur (its Refine brush and the limiting
  mask's "keep in focus" already say where it applies) take none.
- **The photo-wide limiting mask is unchanged**: it still blends the chain's result with the photograph after its
  removals and before its lens blurs, so it reads as "all enhancements", and where both are painted the two meet as an
  intersection.
- **The cache**: a masked step is stored twice, unmasked under today's key and blended under a key that folds the mask's
  signature in, so a mask edit never runs that step's model again; every later step runs again, since its input changed.
- **Masks in the photograph's own coordinates**: step masks are painted over the photograph as loaded (before the crop,
  turn and flips) and reframed by the library for each run, so a crop change keeps them; they are rasterised at the
  chain input's size and read bilinearly at a step's size, so a step after an upscale gets the same region.
- **CLI**: `--dn-mask`, `--cl-mask`, `--la-mask`, `--cb-mask`, `--sh-mask`, `--rs-mask`, `--gr-mask <FILE>`, each requiring its
  family's flag, beside `--rs-str` and `--lb-refine`.
- **GUI**: a **Paint mask** action on each maskable row of the enhancement list, a badge on the row once a mask is
  painted, the brush and overlay of the row being edited, and the masks sent on their operations.
- **Export record**: an enhancement that had a mask is recorded as such in `opai:Enhancements` and the history event.

## Capabilities

### New Capabilities

None. The behaviour extends `masking`.

### Modified Capabilities

- `masking`: step masks, how they combine with the limiting mask, the cache and the coordinates.
- `cli-enhance`: the per-family mask flags.
- `gui-enhance`: Paint mask on a row, its badge and overlay, and the masks in preview, export and batch.

## Impact

- `opai`: `mask.rs` (`Mask::reframed`), `inference/process/{mod.rs,options.rs,identity.rs}` (`ProcessOptions::step_masks`,
  the per-step blend in `run`, `identity_after_masked`), `models/artifact.rs` (`Family::takes_step_mask`),
  `export/metadata/record.rs` (the record), tests in `inference/process/tests/step_mask.rs`.
- `cli`: `args.rs` (six flags), `run.rs` (read once per command, aspect check per photograph), `docs/CLI_REFERENCE.md`.
- `gui`: `src/enhance/{operation.rs,mod.rs}` (`Requested.scope`, the reframe), frontend `stores/mask.ts` (a fourth
  region kind keyed by family), `features/enhancements/EnhancementRow.tsx`, `features/mask/*`, `lib/mask.ts`,
  `lib/batch.ts`, `features/export/batch.ts`, 14 locales.
- No model, no catalogue row and no storage-format change. Run-cache entries of unmasked runs keep their keys.
