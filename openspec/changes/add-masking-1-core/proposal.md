> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I tested its application in practice.

## Why

Every enhancement applies to the whole frame. Issue #29 asks to "select the subject to apply enhancements to,
specifically sharpening, rather than to the whole image", and the maintainer replied that it would be great to have. The
same need comes up wherever a model is right in one part of a photograph and wrong in another: a sharpen that crisps a
face but grains the sky, an upscale that invents texture on a wall, a denoise that smears hair.

This slice is the part both front ends need: what a mask is, how it is drawn into pixels, and where a run applies it.
The brush in the GUI (slice 2) and the other uses of the mask build on it.

## What Changes

Task estimate: **13**. Slice 1 of the `add-masking` series (see design.md - Slices).

- **`opai::mask`**: a `Mask` is a list of brush strokes (paint or erase, a radius, a softness, and points as fractions
  of the framed photograph) over an optional base plane (a weight image, such as a mask file), optionally inverted. It
  is rasterised to a weight plane at any size and has a signature for the cache.
- **A masked run**: `ProcessOptions::mask`. When a run is given a mask, the chain's result is blended per pixel with the
  photograph it started from: where the weight is 1 the result shows, where it is 0 the photograph does, and in between
  a mix. When the chain changed the size (an upscale), the photograph is enlarged to the result's size with Lanczos3 for
  the blend. No step's cache key changes, so editing the mask never runs a model again; only the blend is redone.
- **`imaging`**: `stroke`, a soft-edged brush stroke rasteriser over a `Plane`; `mix::masked`, `mix::blended` with a
  per-pixel weight instead of a single number.
- **CLI**: `--mask <FILE>`, a greyscale image (white applies the enhancements, black keeps the photograph), resized to
  each photograph.

Crates touched: `imaging` (the two primitives), `opai` (the mask, the masked run), `cli` (the flag). No logic in a front
end: the CLI only reads a file into a base plane and hands it on, as the GUI will hand on strokes.

`rust-sak` coverage found: `crypto` (XXH3-64, already what identities are made from) for the mask's signature; `image`
for decoding the mask file. Nothing is missing.

## Capabilities

### New Capabilities

- `masking`: what a mask is, how it is rasterised, and how a masked run blends the result with the photograph.

### Modified Capabilities

- `cli-enhance`: `--mask` limits the requested enhancements to the white part of a mask image.

## Impact

- `imaging`: new `src/stroke.rs`; `src/mix.rs` gains `masked`; `lib.rs` lists the module.
- `opai`: new `src/mask.rs` (`Mask`, `Stroke`, rasterising, signature, `Mask::from_image`);
  `inference/process/options.rs` (`ProcessOptions::mask`), `inference/process/mod.rs` (the blend after `run`),
  `inference/process/identity.rs` (`identity_masked`); `lib.rs` re-exports; tests in `inference/process/tests/mask.rs`.
- `cli`: `args.rs` (`--mask`), `run.rs` (read once, refuse a mismatched aspect, hand on per photograph),
  `docs/CLI_REFERENCE.md`.
- No change to any model, to the catalogue or to the cache's storage format.
