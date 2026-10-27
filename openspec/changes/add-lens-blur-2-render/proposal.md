> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I tested its application in practice.

## Why

A lens blur is mostly image processing: given a per-pixel blur radius, render the photograph as a lens would have, with
sharp edges where the subject meets the background and highlights spreading into discs. None of the pieces it needs
exist in `imaging`: there is no single-channel plane, no linear-light conversion in public, no edge-aware upsampling,
and no blur of any kind. This slice adds them as model-free primitives, so slice 3 only has to compute radii.

## What Changes

Task estimate: **9**. Slice 2 of the `add-lens-blur` series (see `add-lens-blur-1-depth/design.md` - Slices and D9 to
D11). Independent of slice 1; both are based on `main@upstream`.

- **`imaging::plane::Plane`**: a single-channel `f32` plane of any size, with bilinear resampling to another size. The
  type a depth map, a radius map and a future mask are passed as.
- **`imaging::linear`**: sRGB to linear and back for `f32`, with lookup tables for 8- and 16-bit channels.
- **`imaging::guided::upsampled`**: the fast guided filter (He and Sun), upsampling a low-resolution plane to a guide
  photograph's size with edges that follow the photograph.
- **`imaging::guided::dilated_nearer`**: a grey dilation with a disc footprint (the larger value wins), for the edge
  grow.
- **`imaging::defocus::defocus`**: the layered depth-of-field render: a signed radius per pixel, tent-weighted layers,
  premultiplied linear-light layers convolved with anti-aliased discs as sums of horizontal runs (reduced resolution
  beyond 12 px), composited far to near with normalised coverage, an optional "keep sharp" weight plane, cancellable,
  rows in parallel through `rows::for_each_row`.

Crates touched: `imaging` only (`plane`, `linear`, `guided`, `defocus`, and `bilinear::Tap`'s fields made `pub(crate)`); it stays dependent on `image` and `thiserror` alone (no FFT crate, no rayon; design D10 explains why). Measured on an M4 Pro (loaded): 24 MP renders in 0.6 to 1.0 s and 2 MP in 0.07 to 0.22 s (R5), against targets of 3 s and 0.2 s.

`rust-sak` coverage found: nothing; these are pixel primitives, which is what `imaging` is for.

## Capabilities

### New Capabilities

- `lens-blur`: the render's requirements (this slice); the operation's (slice 3) and the gain map's (slice 5) are added
  to the same capability.

### Modified Capabilities

None.

## Impact

- New: `crates/imaging/src/plane.rs`, `linear.rs`, `guided.rs`, `defocus.rs`; `lib.rs` declares them. Module docs name
  their first caller (`models::lens_blur`).
- No `crates/perf` benchmark: timings come from an ignored test run with `--release`, which keeps the slice small.
