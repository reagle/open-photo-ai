## Context

See proposal.md - Why. This design is the series' own: it records the decisions for the whole of masking (D1 to D14),
and the later slices' design.md files refer back to it rather than repeating it.

State of the code the series builds on (`main@upstream`):

- **A chain runs at full resolution** on the framed picture (crop, turn and flips applied by the GUI before the run; the
  CLI has no framing). `inference/process/mod.rs::process_with` resolves the faces (and, on the fork's lens blur line,
  the depth), plans, then calls `run`, which walks the steps and keys each one in the run cache by
  `identity_after(source, operations[..=index], depth)`. A step served from the cache runs nothing. The result's
  identity is `identity_after` over the whole chain.
- **A strength is already a blend.** `imaging::mix::blended(source, corrected, bias)` moves the photograph towards a
  model's result by one number; denoise and sharpen end that way. A mask is the same blend with a weight per pixel.
- **Face recovery composites through a feathered plane** (`models/face_recovery/{mask,composite}.rs`), private to that
  family. Nothing else in `opai` reads a weight per pixel.
- **No single-channel plane in `imaging` on `main@upstream`.** The fork's lens blur adds `imaging::plane::Plane` (an
  `f32` plane of any size with a bilinear resize) in `add-lens-blur-2-render`; this series needs the same type (D9).
- **The GUI's per-photograph state is in memory only** (`stores/enhancements.ts`: "Nothing here is persisted"), keyed by
  path. The preview arms itself for one click in the lens blur's focus pick (`stores/focusPick.ts`,
  `features/preview/FocusPick.tsx`) on the fork; there is no painting.
- **Los Angeles** (`dt_losangeles_encoder_int8.onnx`, 8.9 MB, and `dt_losangeles_decoder_int8.onnx`, 4.9 MB) is
  published in the model repository and used by no code. Its contract was checked on 2026-10-27 with onnxruntime on the
  CPU: encoder `pixel_values` 1x3x1024x1024 (ImageNet-normalised, longest side scaled to 1024, padded at the bottom and
  right) to `image_embeddings` and `image_positional_embeddings` (1x256x64x64 each), 0.45 s; decoder `input_points`
  (1x1xNx2, in the 1024 frame), `input_labels` (1x1xN, `int64`, 1 = subject, 0 = background), both embeddings, to
  `iou_scores` (1x1x3) and `pred_masks` (1x1x3x256x256 logits), 14 ms. On a synthetic disc one click gave IoU 0.99. It
  is the SlimSAM-style export transformers.js uses. Slice 4 builds on it.

## Goals / Non-Goals

**Goals:**

- One mask per photograph that limits where the enhancements apply, painted with a brush in the GUI and given as an
  image to the CLI.
- Preview and export agree: the mask is defined independently of resolution and rasterised for whatever size a run
  produces.
- Editing the mask never runs a model again: only a blend is redone.
- A mask that later slices can fill from elsewhere (a click on the subject, an edge refinement) and that the lens blur
  can use as "keep this sharp".

**Non-Goals (this series' later slices, or not at all):**

- A mask per enhancement (sharpen the subject, denoise the background): slice 6.
- Masks saved across launches, and masks that follow a change of crop: slice 7.
- Feeding the mask into a model's input. A masked run blends results; no model sees the mask, except the lens blur,
  which takes it as a weight it already has a seam for (slice 3).
- Luminosity or colour-range masks, gradients, a lasso or polygon tool.

## Slices

| # | Change | Delivers | Depends on | Base | State |
|---|---|---|---|---|---|
| 1 | `add-masking-1-core` | `opai::mask` (strokes, base plane, invert, rasteriser, signature), the masked run (blend after the chain, Lanczos3 for a resized result), `imaging::{plane, stroke}`, `mix::masked`, CLI `--mask` | - | `main@upstream` | **this change** |
| 2 | `add-masking-2-brush` | GUI: Mask panel, brush on the preview (paint, erase, size, softness, invert, clear, undo/redo), tinted overlay, the mask sent with every preview and export, i18n | 1 | 1 | proposed |
| 3 | `add-masking-3-lens-blur` | The mask as Lens Blur's "keep sharp" weight; the blend placed before a trailing lens blur; what the mask is used for (`--mask-use`, two switches in the panel) | 2, `add-lens-blur-4-gui` | merge of 2 and `add-lens-blur-4-gui` | proposed |
| 4 | `add-masking-4-select` | Click the subject: Los Angeles proposes a mask (subject and background clicks, a box), the base plane of the mask; the brush refines it | 2 | 2 | pending, not proposed |
| 5 | `add-masking-5-refine` | Refine edges: a trimap from the mask's edge, matted by the Matting family (ViTMatte-S, from `add-lens-blur-8-matting`), for hair and fur | 4, `add-lens-blur-8-matting` | merge | pending, not proposed |
| 6 | `add-masking-6-scope` | Per enhancement: whole photograph, inside or outside the mask; the blend moves into the run per step | 2 | 2 | pending, not proposed |
| 7 | `add-masking-7-keep` | The mask kept in the photograph's coordinates (survives a change of crop) and saved with the session | 2 | 2 | pending, not proposed |

Slices 1 to 3 are the first cut. 4 needs only the published Los Angeles graphs; 5 needs the fork-only matting graph
(D14).

## How the work divides

- `imaging`: pixel primitives with no idea what a mask is for: `Plane`, the stroke rasteriser, `mix::masked`.
- `opai`: everything both front ends need: the mask type and its wire shape (serde), rasterising, the signature, reading
  a mask image, the masked run and its identity.
- `cli`: the flag, reading the file once, checking its aspect against each photograph, handing it on.
- `gui` (slice 2): the brush, the panel, the overlay, the per-photograph store and undo; one command that rasterises a
  mask for the overlay (calling `opai`), and the mask added to the existing `enhance` and `export` commands.

## `rust-sak` coverage

- `crypto`: XXH3-64 (`xxh3_string`, `xxh3_bytes` or what the module names them) for the mask's signature, as identities
  already are.
- `image`: decoding a mask file in any format the CLI reads, already oriented.
- Nothing missing. Rasterising strokes and blending are pixel work for `imaging`, not infrastructure.

No new third-party crate.

## Decisions

### D1. One mask per photograph, applied to the stack as a whole

A photograph has at most one mask. In this slice it limits the whole chain: the chain runs over the whole frame as it
does today, and its result is blended with the photograph through the mask. Slice 3 adds a second use of the same mask
(Lens Blur keeps it sharp) and lets each use be switched off; slice 6 adds a per-enhancement choice.

Why one mask and the stack, first: the blend then happens once, after every model, so the per-step cache keys stay as
they are and a mask edit recomputes one blend (D5). A blend per step would re-run every step after the first masked one
on every brush stroke, which for an upscale is seconds to minutes. One shared mask is also what users ask of Topaz
(masks that carry across filters) and the smallest UI. The cost is that "sharpen the subject, denoise the background"
needs slice 6.

What the mask means, in one sentence for the UI and the docs: **the mask marks the part of the photograph the
enhancements are for** (white: enhanced, black: left as it was). Slice 3 keeps this meaning for the lens blur: the
masked part is the subject, kept sharp.

### D2. A mask is strokes over an optional base, not pixels

```rust
pub struct Mask {
    /// A weight image the strokes are painted over: a mask file (CLI) or, in slice 4, a selection. `None` is empty.
    pub base: Option<Base>,
    /// Painted and erased in order.
    pub strokes: Vec<Stroke>,
    /// Whether the finished weight is turned over (1 - w).
    pub inverted: bool,
}

pub struct Stroke {
    pub erase: bool,
    /// Radius as a fraction of the framed photograph's longer side.
    pub radius: f32,
    /// 0 is a hard edge (anti-aliased only); 1 feathers from the centre out.
    pub softness: f32,
    /// x0, y0, x1, y1, ... as fractions of the framed photograph's width and height.
    pub points: Vec<f32>,
}

pub struct Base { plane: Arc<Plane>, signature: u64 }
```

Strokes are a few kilobytes even for a long session, are independent of resolution (D3 rasterises them at whatever size
a run needs, so preview and export agree without resampling a bitmap), make undo the removal of the last stroke, and are
exactly what the GUI sends on each run. A base plane is for masks that arrive as pixels; it is resampled bilinearly.
Points are fractions of the **framed** photograph, the space the preview shows and the chain receives, as the lens
blur's focus point is; slice 7 moves them into the photograph's own coordinates.

Serde: camelCase on the wire (`{ strokes: [{ erase, radius, softness, points }], inverted }`); the base never crosses
the wire (the CLI builds it in-process; slice 4 sends a selection's handle, not its pixels). Values are clamped on
deserialisation (radius to `[0.0005, 0.5]`, softness to `[0, 1]`, points to `[0, 1]`, an odd trailing coordinate
dropped), never refused: a bad value can only come from a fault.

### D3. Rasterising: capsules with a soft edge, painted and erased in order

For a target `width x height` (L = the longer side):

1. Start from the base resampled bilinearly to the target, or from 0 everywhere.
2. For each stroke, in order: its pixel radius `r = radius * L`, at least 0.5; its hard core `c = r * (1 - softness)`,
   but at most `r - 1` so every edge is at least one pixel soft (anti-aliasing). A pixel at distance `d` from the
   stroke's polyline (the nearest of its segments; a single point is a dot) gets the stroke weight `s = 1` for `d <= c`,
   `0` for `d >= r`, and `smoothstep` between. A paint stroke sets `w = max(w, s)`; an erase stroke sets `w = min(w, 1 -
   s)`.
3. If inverted, `w = 1 - w`.

Max and min rather than adding, so going over the same place twice does not darken a soft edge, and an erase is the
exact opposite of a paint. Each segment touches only the rows and columns of its own bounding box grown by `r`
(`rows::for_each_row` over the box), so the cost is the painted area, not the frame times the stroke count. A stroke's
polyline is the GUI's pointer path decimated to points about `0.1 r` apart (slice 2).

The pixel primitive is `imaging::stroke::painted(plane: &mut Plane, points: &[f32], radius_px, core_px, erase)` with
points already in pixels; `opai::mask` does the fractions, the order and the inversion.

The mask is rasterised once per run, at the **framed source's** size (the chain's input), and read bilinearly at the
result's size when that differs (D4), so an 8x upscale does not allocate a full-size plane. 24 MP costs 96 MB of `f32`
for the plane while the run holds it.

### D4. The blend: the result where the mask is white, the photograph where it is black

`out = source + w (result - source)`, per channel in unit space: `mix::blended` with `w` read per pixel.
`imaging::mix::masked(source: &Sampler, corrected, weight: &Plane)` does it in place on the result (taken by value and
handed back, as `blended` is), reading the weight bilinearly when the plane's size differs from the result's. Weights of
exactly 0 and 1 copy the pixel without arithmetic, so the unmasked part is byte for byte the result and the masked-out
part byte for byte the photograph.

When the chain changed the size (an upscale), the source is first enlarged to the result's size with Lanczos3
(`image::imageops::resize`), which is what "a plain enlargement" means for the masked-out part. Only rows that have any
weight below 1 need it, but the first cut enlarges the whole frame for simplicity (a 4x upscale already holds the result
at that size); a row-banded enlargement is a later optimisation if memory says so.

The result keeps its own bit depth: the blend is generic over `imaging::tensor::Channel` as `blended` is. A gain map
(fork colour stack) is not touched: no enhancement but the lens blur changes it, and the photograph's own gain map is
what both halves of the blend already share.

### D5. The cache and the identity

- **Step keys do not change.** The mask is not an operation; `run` never sees it. Every step is served from the cache as
  before; a mask edit costs one rasterisation and one blend (well under a second at 24 MP, plus reading the cached
  result).
- **The blended result is not stored** in the run cache: it is cheaper to redo than to read back a PNG of it.
- **The result's identity folds in the mask**: `identity_masked(identity_after(...), signature)` hashes
  `"{identity}|mask|{signature:016x}"`, the word `mask` sitting where a depth tag sits in a chain key so no chain key
  can collide with it (as `identity_of_data`'s `data` does). A run with no mask (or an empty one, D6) keeps today's
  identity exactly.
- **The signature** is XXH3-64 of the mask's canonical serde JSON (strokes and `inverted`) followed by the base's
  signature, which is XXH3-64 of its plane's bytes, taken once when the base is made.

### D6. Empty masks

A mask with no base and no strokes is no mask: the run is exactly today's, identity included. An inverted empty mask is
also treated as no mask (the GUI disables Invert while the mask is empty; a CLI mask file is never empty in this sense).
A mask that rasterises to all ones is applied (it is the user's mask, and it is cheap), so it carries its own identity.

### D7. CLI: `--mask <FILE>`

- Any image format the CLI reads, decoded and oriented like a photograph. Its weight is the luminance (Rec. 709 on the
  decoded values, 16-bit masks kept at 16 bits) in `[0, 1]`: white applies the enhancements. An alpha channel is ignored
  (documented: flatten first).
- Resized to each photograph's size (bilinear). A mask whose aspect ratio differs from a photograph's by more than 1
  percent is refused for that photograph with an error naming both ratios; the other photographs of the batch still run.
  With several inputs, one mask applies to each.
- Needs at least one enhancement, or `--autopilot` (which then limits whatever Autopilot chooses). Read once per
  command, not once per photograph.
- The mask is a base plane with no strokes, so the GUI's and the CLI's masks are the same type.
- `docs/CLI_REFERENCE.md` gains a "Masks" section: what white and black mean, the aspect rule, and that an upscale shows
  a Lanczos enlargement where the mask is black.

### D8. Public API

`opai::mask::{Mask, Stroke, Base}` (re-exported at the crate root as `Mask`, `Stroke`), `Mask::from_image(&DynamicImage)
-> Mask` (a base, no strokes), `Mask::is_empty`, `Mask::rasterised(width, height) -> Plane`, `Mask::signature() -> u64`;
`ProcessOptions::mask: Option<Arc<Mask>>` (an `Arc` because a batch export shares one mask across runs and the options
are `Clone`). `ProcessOptions` is not `#[non_exhaustive]` by design, so the field is additive. `Opai::process` is
unchanged otherwise.

### D9. `imaging::plane` is shared with the lens blur

The fork's `add-lens-blur-2-render` adds `imaging/src/plane.rs` (and makes `bilinear::Tap`'s fields `pub(crate)` for
it). This series is based on `main@upstream` so that it can go upstream on its own, and needs the same type. The first
commit of this slice is that file and that `bilinear.rs` hunk **byte for byte** (taken with `jj file show -r
add-lens-blur-2-render`), plus `pub mod plane;` in `lib.rs` at the same place. When the two lines meet (the
`integration` merge, slice 3, or an upstream rebase), the identical changes merge cleanly; only `lib.rs`'s module
documentation may need a one-paragraph resolution. The alternative, splitting `plane.rs` out of `add-lens-blur-2-render`
into a shared base commit and rebasing both lines onto it, is cleaner but rewrites pushed bookmarks: the orchestrator's
call.

### D10. Where the code lives

- `imaging::stroke` and `imaging::mix::masked`: pixel primitives (cross-family tier, they know no mask semantics).
- `opai::mask`: a top-level module beside `image` and `export`, since it is neither a model nor orchestration; nothing
  under `models/` names it in this slice (slice 3 hands the lens blur a `Plane`, not a `Mask`).
- `inference/process/mod.rs`: one call after `run` returns: rasterise, enlarge the source if needed, `mix::masked`,
  `identity_masked`. Logged at `debug` with the mask's signature and the time taken, inside the run's span.
- Face recovery's Gaussian (`models/face_recovery/mask.rs`) is not promoted: the stroke rasteriser's smoothstep edge
  needs no blur. If a later slice feathers a base plane, that slice promotes it first.

### D11. Progress and cancellation

The blend is not a step: it adds no progress slot and reports nothing. It checks the run's cancellation token once
before starting (a cancelled run returns the usual stop), and is quick enough not to need checks inside.

### D12. Wire shape for the GUI (used in slice 2)

`enhance` and `export` gain `mask: Option<Mask>` (the serde shape of D2). The GUI sends the whole mask with every
request; it is small. `mask_preview(mask, width, height) -> Vec<u8>` (a greyscale PNG of the rasterised weight) lets the
overlay show exactly what a run will use. Both are slice 2's.

### D13. What slice 3 changes in the blend's placement

With a lens blur in the chain (always last, `Family::APPLY_ORDER`), the blend moves to just before it: the chain without
the lens blur is run and blended, and the lens blur runs over the blended picture with the mask as its "keep sharp"
weight (the seam `LensBlur.keep_sharp` and `imaging::defocus`'s `keep_sharp: Option<&Plane>`, which exist for this).
Otherwise the masked-out part would show the unblurred photograph. Slice 3's design has the cache details.

### D14. Models in later slices

- **Los Angeles (slice 4)**: published, small, fast (see Context). The encoder runs once per framed photograph and its
  two embeddings (8 MB) are held per photograph in memory (not the run cache: recomputing costs 0.5 s); each click runs
  only the decoder (14 ms), so selection feels immediate. The best of the three proposed masks by `iou_scores`,
  upsampled from 256 px and thresholded at logit 0 into a base plane, then softened by one pixel.
- **Edge refinement (slice 5)**: the fork's lens blur work found ViTMatte-S (Apache-2.0 weights, MIT code; 103 MB FP32
  ONNX, about 0.35 s per 1024 px tile on MPS, 0.8 s on CPU) gives strand-level alphas from a trimap, far better than
  closed-form matting or BiRefNet (`opai-notes/lens-blur/matting/README.md`). Slice 5 makes the trimap from the mask
  (sure inside where it is above 0.99 after an erosion, sure outside below 0.01 after a dilation, unknown between) and
  replaces the mask's edge with the matte. It reuses the Matting family `add-lens-blur-8-matting` adds rather than
  adding the model twice.

## Risks / Trade-offs

- **A seam where a model changed the picture a lot.** An upscale's texture next to a Lanczos enlargement, or a
  colorization next to the grey original, shows a visible transition even with a soft edge. That is what the user asked
  for, and the softness control is the tool; documented in the CLI reference and the panel's help.
- **Geometry-changing enhancements would break the blend.** None exists (crop is before the chain); an enhancement that
  moved pixels (a lens correction) would need the mask moved with it. Asserted: the result's aspect must match the
  source's within a pixel, or the blend is skipped with a warning.
- **Memory on large upscales.** The enlarged source doubles the peak for a 4x upscale result (a 24 MP photograph at 4x
  is 384 MP: about 1.2 GB per 8-bit RGB buffer). Measured in the acceptance run; a row-banded enlargement fixes it if
  needed.
- **The mask is tied to the framing** until slice 7: changing the crop in the GUI clears the mask (slice 2 says so
  before it happens).

## Platforms

The code is platform-independent CPU work. Validated on macOS (Apple silicon) only; Windows and Linux untested. No
execution provider is involved.

## Open Questions

1. **Meaning of white** (default: white = enhanced). Photoshop's layer masks agree; some tools paint the protected area
   instead. Invert covers the other habit.
2. **Softness default** (default: 0.5) and **size default** (default: 5 percent of the longer side) for slice 2.
3. **CLI alpha channel**: ignore (default) or use alpha as the weight when present. Ignoring is predictable for a grey
   PNG exported with an alpha channel.
