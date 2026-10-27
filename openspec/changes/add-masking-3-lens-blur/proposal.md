> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I tested its application in practice.

## Why

The fork's lens blur takes its depth from a model, and where the depth is wrong the subject goes soft: side hair read 5
to 10 cm behind the face, a hand, the far edge of a hood. Its render already has a seam for this (`keep_sharp`, a weight
that holds an area in focus), left unused until masking existed. With slices 1 and 2 the user can paint the subject;
this slice lets the lens blur keep what is painted sharp.

Without it, a mask and a lens blur also work against each other: slice 1 blends after the whole chain, so the masked-out
part would show the unblurred photograph, the opposite of what a lens blur is for.

## What Changes

Task estimate: **9**. Slice 3 of the `add-masking` series (see `add-masking-1-core/design.md` - Slices, D13).

- **The blend moves before a trailing lens blur**: the chain without the lens blur runs and is blended with the
  photograph through the mask, then the lens blur runs over the blended picture.
- **Lens Blur keeps the mask sharp**: the mask, rasterised, becomes the lens blur's `keep_sharp` weight, and its
  signature joins the lens blur's cache tag. The masked part is the subject: enhanced, and in focus.
- **What the mask is for** can be narrowed: `Mask` gains `limits_enhancements` and `keeps_sharp` (both on by default).
  With only `keeps_sharp`, a painted subject is held in focus while every other enhancement still applies to the whole
  photograph.
- **CLI**: `--mask-use <both|enhancements|lens-blur>` (default `both`).
- **GUI**: two switches in the mask panel, shown when the stack holds a lens blur: "Limit the enhancements to the mask"
  and "Keep the mask in focus (Lens Blur)"; i18n in all 14 locales.

Crates touched: `opai` (the blend's placement, the injection, the cache tag), `cli` (the flag), `gui` (two switches).
The choice of use is part of the mask, so both front ends hand it on and neither decides anything.

`rust-sak` coverage found: nothing needed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `masking`: a mask's two uses, and the blend placed before a lens blur.
- `cli-enhance`: `--mask-use`.
- `gui-enhance`: the mask panel's two switches.

## Impact

- Base: a merge of `add-masking-2-brush` and `add-lens-blur-4-gui` (`jj new add-masking-2-brush add-lens-blur-4-gui`),
  then this slice's commits on top. Expected merge conflicts, all mechanical: `imaging/src/lib.rs` (module docs;
  `plane.rs` and the `bilinear.rs` hunk are identical on both sides), `features/preview/PreviewImage.tsx` (paint mode
  beside the focus pick), `ipc/enhance.ts`, `test/support.tsx`, the 14 locale files, `inference/process/mod.rs` (the
  depth injection beside the blend).
- `opai`: `mask.rs` (the two fields, serde defaults, signature), `inference/process/mod.rs` (split at a trailing lens
  blur), `inference/depth_map.rs` or a new `inference/keep_sharp.rs` (injection), `models/lens_blur/mod.rs`
  (`with_keep_sharp` takes the signature; the tag).
- `cli`: `args.rs`, `run.rs`, `docs/CLI_REFERENCE.md`.
- `gui`: `features/mask/MaskPanel.tsx`, `stores/mask.ts`, `ipc/mask.ts`, 14 locale files.
