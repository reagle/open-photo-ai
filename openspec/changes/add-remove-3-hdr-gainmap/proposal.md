> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5, low effort) at my behest; I have not yet tested its
> application in practice.

## Why

Slices 1 and 2 remove an object from the photograph's pixels only. On the colour line an Ultra HDR JPEG carries a gain
map, which every step keeps unless it rewrites it. Kept under a removal, the map still holds the removed object's gain,
so an HDR display lifts the object's highlights over the fill: a bright ghost of the lamp or the sign that was taken out.
`add-remove-1-core` D12 decided the answer; this slice builds it, so Remove can be merged into a build that carries gain
maps.

## What Changes

Task estimate: about **6**. Slice 3 of the `add-remove` series (see `add-remove-1-core/design.md` - Slices); its own
decisions are design.md G1 to G6. Base: a local merge of `add-remove-2-gui` and `add-color-4-ultra-hdr-gainmap`.

- **The gain map is filled under the removal from its own surroundings**: the cells under the hole are left out of a
  push-pull membrane fill over the cells under each region's crop, and take the fill through the colour paste's own
  feather, averaged over each cell's pixels, so a map smaller than the photograph gets the same region at its own
  resolution. Cells with no pixel in the hole keep their bytes. Not LaMa.
- **A gain map that cannot be laid over the photograph is dropped** (larger than it, or not 8-bit grayscale or RGB),
  with the standard "HDR gain map dropped: it could not be carried" warning; the export reports the HDR as not carried.
- **The chain carries the gain map through its steps** (the lens-blur line's plumbing, ported): `ImagePipeline` gains
  `rewrites_gain_map` and `run_with_gain_map`, the run cache stores what a step made of the map beside its pixels, and
  the result takes the last step's map.
- `imaging::fill` (push-pull) and the reductions it needs, from the lens-blur line.

Crates touched: `imaging`, `opai`. Nothing in the CLI or the GUI: both export what the chain hands back.

## Capabilities

### Modified Capabilities

- `object-removal` (added by `add-remove-1-core`): a removal rewrites the photograph's Ultra HDR gain map, or drops it.

## Impact

- `imaging`: `fill.rs`, `boxes.rs` (reductions only).
- `opai`: `models/remove/gain_map.rs` (new), `models/remove/process.rs` (`Removal` implements `ImagePipeline` directly),
  `pipeline/mod.rs`, `inference/process/mod.rs`, `image/gain_map.rs`, `image/picture.rs`, `error.rs`
  (`InferenceError::GainMap`).
- Merging with the lens-blur line later: the same two trait methods, `StoredGainMap` and `fill.rs` exist there; take
  theirs and keep the removal's implementation (design.md G6).
