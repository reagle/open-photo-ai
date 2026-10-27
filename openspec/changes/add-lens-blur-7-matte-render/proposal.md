> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I tested its application in practice.

## Why

The layered render gives each pixel one depth. A pixel on a strand of hair or fur is part strand and part background, so
it either stays sharp with the background between strands still crisp ("curls" of sharp street along a fur hood) or
carries the strand's colour into the blurred background (a dark haze). The edge grow (series D9) moves the choice, it
cannot make it right. Testing slices 1 to 5 on a fur hood at f/1.4 showed both.

The fork's matting prototype (`opai-notes/lens-blur/matting/README.md`, 2026-10-27) settled the fix: give the in-focus
subject a per-pixel coverage (a matte), take the background as if the subject were not there, and composite the subject
over the blurred background as one layer. With a depth trimap and ViTMatte-S the curls and the haze go on all three test
photographs and individual strands survive. This slice is the model-free half: the render primitives. Slice 8 adds the
matting model and wires it in.

## What Changes

Task estimate: **10**. Slice 7 of the `add-lens-blur` series (see `add-lens-blur-1-depth/design.md` - Slices; the matte
decisions are in this change's design, M1 to M7).

- **`defocus` composites a matted subject**: an optional `Subject { colour, alpha, radius }`, one premultiplied layer
  placed "over" the accumulator at its depth in the far-to-near loop.
- **Push-pull fill** (`imaging::fill`): values filled in where a weight is 0 from the weighted mean of their
  surroundings, a pyramid down and up; seamless and linear in the pixel count. Used for the background behind the
  subject (colour and disparity).
- **Foreground colour by blur fusion** (`imaging::fusion`, Forte and Pitié 2021): the subject's own colour where its
  pixels are mixed with the background, from two box-filter passes.
- **Grey morphology** (`imaging::morph`): erosion and dilation by a square, by running minimum and maximum (for the
  trimap in slice 8).
- **Layer spacing**: `Defocus::spacing`, `Even` (today's, still the default) or `Geometric { first, ratio }`, so layers
  are dense near focus where small radii differ visibly.
- `guided.rs`'s private box mean and box reduction move to a shared `imaging::boxes` before the second and third callers
  (fusion, fill) use them.

Crates touched: `imaging` only. `opai`'s one caller of `defocus` passes `None` for the subject and keeps `Even` spacing;
nothing visible changes in this slice.

`rust-sak` coverage found: nothing needed (pixel primitives).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `lens-blur`: the render can composite a matted subject over a background filled in behind it, and space its layers
  geometrically.

## Impact

- `imaging`: new `src/fill.rs`, `src/fusion.rs`, `src/morph.rs`, `src/boxes.rs`; `src/defocus.rs` (`Subject`, the
  argument, `Spacing`); `src/guided.rs` (uses `boxes`); `lib.rs` docs and modules.
- `opai`: `models/lens_blur/render.rs` passes `None` (one line).
- Base: `add-lens-blur-4-gui` (the changes are in `imaging` only, so the slice rebases onto `add-lens-blur-2-render`
  cleanly if the upstream PR wants the primitives with the render).
