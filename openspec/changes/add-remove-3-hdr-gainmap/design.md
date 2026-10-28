> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5, low effort) at my behest; I have not yet tested its
> application in practice.

## Context

An Ultra HDR JPEG's gain map records, per region of the scene, how much brighter the HDR rendition is than the SDR base.
On the colour line (`add-color-4-ultra-hdr-gainmap`) it is decoded at load (8-bit grayscale or RGB, full resolution or
smaller), carried on the `Picture`, reframed by a crop, and written back at export. A removal changes what the
photograph shows, so the map under the removed object is no longer true of the result. `add-remove-1-core` D12 chose a
fill of the map's own surroundings through the same feather, not LaMa.

## Goals / Non-Goals

**Goals:** the removed object leaves no gain behind; nothing outside the colour paste's hole changes; a smaller map gets
the same region; a map that cannot be rewritten is dropped with the standard warning; the rewrite is served from the run
cache like the pixels.

**Non-Goals:** reading or changing the map's parameters; generating HDR detail for the fill; the lens blur (another
line).

## Decisions

### G1. Push-pull over the cells the hole does not touch

Per region, over the map cells under its crop (the crop holds the hole with at least 64 px of context): a cell is
unknown where any photograph pixel under it is within 16 px of a mark (the hole LaMa is shown), known otherwise, and
`imaging::fill::filled` continues the known cells across the unknown ones. Push-pull is exact where known, smooth where
not, linear in the cells and has no iterations. The membrane is drawn only from cells outside the hole, as LaMa sees only
pixels outside it, so the object's own gain never feeds the fill.

### G2. The colour paste's feather, averaged per cell

Each cell takes the fill by the mean of `fill_weight` over the photograph pixels under it: 1 within 8 px of a mark, a
smoothstep to 0 at 16 px. A full-resolution map gets exactly the colour's feather; a quarter-resolution one the same
feather averaged over its 4x4 footprints. A cell whose mean is 0 keeps its byte, so outside the hole the map is
unchanged byte for byte (the export re-encodes it, at quality 90 or more).

### G3. Cells lie over pixels by the two sizes

The map covers the photograph's scene, so cell `c` of an axis `n` cells long over `N` pixels lies over pixels
`floor(c N / n)` to `ceil((c + 1) N / n)`. Its stored scale is not used: after a crop the sizes are what holds.

### G4. In the map's encoded values

The fill works on the stored bytes (the log gain after the map's gamma), where the field is smooth, and reads none of
the map's parameters, so no map is dropped for its metadata (unlike the lens blur, which rebuilds HDR in linear light).

### G5. Dropped where it cannot be laid over the photograph

A map larger than the photograph on either axis, or of a colour type other than 8-bit grayscale or RGB, cannot be
rewritten: the step answers `InferenceError::GainMap`, the chain logs "HDR gain map dropped: it could not be carried"
and goes on without a map, and the export reports the HDR as not carried. The removal itself succeeds. Kept, the map
would bring the ghost back.

### G6. The lens-blur line's chain plumbing, ported

`ImagePipeline::rewrites_gain_map` and `run_with_gain_map` (defaulted), the run carrying the map through the steps, the
store keeping `<step key>|gm` (the rewritten map's pixels, or the drop's reason as JSON), a hit stored without it run
again, and `Picture::with_shared_gain_map`, as `add-lens-blur-5-hdr-gainmap` built them, so a later merge takes one copy.
`Removal` implements `ImagePipeline` itself rather than through `DepthGeneric`, and computes its regions and distances
once for the colour paste and the map. `rewrites_gain_map` is always true: the chain already drops a removal of nothing.
A limiting mask leaves the map as the removal left it, since the removal is inside what the mask blends back; when the
lens blur's masked gain map arrives, it must limit only what follows the removals.

## Risks / Trade-offs

- A soft map edge: the gain of a very bright object can spread a few cells past its pixels in the map (the map is
  smooth), past the 16 px hole on a quarter-resolution map. On the test photograph (full-resolution map) none was seen.
- The fill in encoded values is not the ratio of a rebuilt HDR fill; for a neutral continuation of smooth gain the two
  agree closely.
