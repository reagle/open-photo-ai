## Context

The series' decisions are in `add-masking-1-core/design.md` (D1 to D14), the brush's in `add-masking-2-brush/design.md`
(G1 to G9). The lens blur's are in `add-lens-blur-1-depth/design.md` (on the fork's `openspec-lens-blur`), in particular
D6 (depth reaches the blur by injection, like faces), D11 (the masking seam), D12 (the cache tag) and D13 (last in the
chain).

On `add-lens-blur-4-gui`: `LensBlur` has `keep_sharp: Option<PlaneHandle>` and `pub(crate) fn with_keep_sharp(self,
weight: Arc<Plane>)`, always `None`; `LensBlurPipeline` hands it to `imaging::defocus::defocus(..., keep_sharp:
Option<&Plane>, ...)`, which scales each pixel's radius by `1 - keep_sharp` (a weight at any size, read bilinearly). A
render test with a plane keeps the seam working. `Family::APPLY_ORDER` puts Lens Blur last.

## Goals / Non-Goals

**Goals:** a painted subject stays in focus under a lens blur; a mask and a lens blur in one stack do the obvious thing;
the user can use the mask for focus only.

**Non-Goals:** using the mask as a matte (alpha composite of the subject over the blurred background). That is
`add-lens-blur-8-matting`'s render path, which takes its alpha from depth and ViTMatte; feeding a user mask into that
trimap is a later slice of either series. Also not here: a mask used only by some of the other enhancements (slice 6).

## Slices

| # | Change | Delivers | Depends on | Base | State |
|---|---|---|---|---|---|
| 1 | `add-masking-1-core` | `opai::mask`, the masked run, `imaging::{plane, stroke}`, `mix::masked`, CLI `--mask` | - | `main@upstream` | proposed |
| 2 | `add-masking-2-brush` | GUI brush, panel, overlay, mask in preview and export, i18n | 1 | 1 | proposed |
| 3 | `add-masking-3-lens-blur` | The mask as Lens Blur's "keep sharp"; blend before a trailing lens blur; `--mask-use` and two switches | 2, `add-lens-blur-4-gui` | merge of 2 and `add-lens-blur-4-gui` | **this change** |
| 4 | `add-masking-4-select` | Click the subject (Los Angeles) | 2 | 2 | pending, not proposed |
| 5 | `add-masking-5-refine` | Refine edges with the Matting family (ViTMatte-S) | 4, `add-lens-blur-8-matting` | merge | pending, not proposed |
| 6 | `add-masking-6-scope` | Per-enhancement scope | 2 | 2 | pending, not proposed |
| 7 | `add-masking-7-keep` | Mask in photograph coordinates, saved with the session | 2 | 2 | pending, not proposed |

## How the work divides

`opai` decides everything: where the blend goes, what the lens blur receives, what the cache keys hold. The CLI maps one
flag onto the mask's two fields; the GUI shows two switches bound to them.

## Decisions

### L1. Two uses, both on by default

`Mask` gains `limits_enhancements: bool` and `keeps_sharp: bool`, serde-defaulted to `true` (camelCase
`limitsEnhancements`, `keepsSharp`) and part of the signature. Which use matters depends on the stack:

| Stack | `limits_enhancements` | `keeps_sharp` |
|---|---|---|
| no lens blur | blend after the chain (slice 1) | no effect |
| lens blur only | no effect | the lens blur's `keep_sharp` |
| others + lens blur | blend before the lens blur | the lens blur's `keep_sharp` |

A use that has no effect on the stack is ignored for the identity as well (a mask whose only use is ignored is treated
as no mask, so it costs and changes nothing).

### L2. The blend before a trailing lens blur

`process_with` splits the planned chain into a head (all but a trailing lens blur) and a tail (the lens blur, or
nothing). The head runs through `run` as today; the blend (slice 1, D4) applies to its result when `limits_enhancements`
holds; the tail runs through `run` over the blended picture, with a `RunCache` for the blended picture's identity, so
the lens blur's step key folds the mask in through its input's identity as well as through its tag. Progress is
continuous: the tail's step reports as the chain's last index (an index offset on the tail's `run`, or a wrapping
callback), so the GUI's progress bar and the CLI's events read as one run. The depth is still estimated on the chain's
input (lens blur D6), so neither the mask nor the head's enhancements make it run again.

Not done: blending inside `run` between two steps. It would put mask knowledge in the step loop and buy nothing over two
calls.

### L3. The mask as `keep_sharp`

When `keeps_sharp` holds and the chain has a lens blur, the mask is rasterised at the framed source's size and handed to
the lens blur with `with_keep_sharp(weight, signature)` before planning, by the same injection that hands it the depth
(`inference/depth_map.rs`'s `fill_chain_depth`, or a sibling `fill_chain_keep_sharp` in its own file if that reads
better). `LensBlur::cache_tag` gains `-k{signature:016x}` when a weight is present (no `_`, as the tag requires).
`defocus` reads the weight at any size, so the upscaled result needs no larger plane.

A pixel with weight 1 renders at radius 0: sharp. The render's existing behaviour at the boundary of a soft mask (radius
scaled smoothly to 0) is what the softness control buys: a hard-edged mask gives a sharp-to-blurred step that can show a
halo of background on the subject's edge, which is the depth's error being held, not fixed; the matte path
(`add-lens-blur-8-matting`) is the fix for edges.

### L4. Ultra HDR gain maps (fork colour line)

`add-lens-blur-5-hdr-gainmap` blurs the gain map with the same radii. When this slice is duplicated onto that line for
`integration`, check that the gain map's render is handed the same `keep_sharp` (it calls the same pipeline; if it
builds its radii separately, pass the weight there too) and add one test: a kept-sharp region's gain map is unchanged.

### L5. CLI

`--mask-use <both|enhancements|lens-blur>`, default `both`, accepted only with `--mask`. `lens-blur` without
`--lens-blur` is refused ("the mask would have no effect"). `docs/CLI_REFERENCE.md`'s Masks section gains a paragraph:
with a lens blur, the white part is the subject, kept in focus.

### L6. GUI

In `MaskPanel.tsx`, when the photograph's stack holds a lens blur, two switches under the tools: "Limit the enhancements
to the mask" and "Keep the mask in focus (Lens Blur)", bound to the store's `limitsEnhancements` and `keepsSharp` for
that photograph, undoable like Invert. If the lens blur is the only enhancement, the first switch is hidden (it has no
effect). Keys `mask.panel.limits` and `mask.panel.keepsSharp` (with a tooltip each) in all 14 locales.

## Risks / Trade-offs

- **Merge conflicts** in `PreviewImage.tsx` (paint mode and the focus pick share the pointer handlers; the two modes are
  exclusive) and in `process_with` (depth injection and the blend). Resolve in the merge commit, before this slice's own
  commits; record the resolutions in `opai-notes/masking/merge-masking-lens-blur.md` for the `integration` merge.
- **The halo at a hard mask edge** (L3): documented; softness and the matte path address it.
- **Progress indices** across two `run` calls: a test asserts the reported indices for a three-step chain with a mask.

## Platforms

macOS (Apple silicon) only; Windows and Linux untested. No execution provider is involved beyond the lens blur's own.

## Open Questions

1. **Default uses** (default: both on). Alternative: with a lens blur in the stack, `limits_enhancements` off by
   default, since the commonest reason to paint with a lens blur is to hold the subject sharp. The switches make either
   choice one click.
