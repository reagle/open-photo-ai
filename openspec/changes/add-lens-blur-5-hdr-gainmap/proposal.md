> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I tested its application in practice.

## Why

On the fork's colour stack (`add-color-4-ultra-hdr-gainmap`) an Ultra HDR JPEG keeps its gain map through every
enhancement, unchanged, because no enhancement so far moves light around the frame. A lens blur does: it spreads each
point over a disc. Left unchanged, the gain map would keep lifting the old, sharp highlights over a blurred base (bright
ghosts of the sharp photograph on an HDR display), and the highlights that should bloom into bokeh discs would stay
SDR-dim. This slice blurs the HDR rendition with the same radii as the SDR base and rewrites the gain map from the two.

## What Changes

Task estimate: **9**. Slice 5 of the `add-lens-blur` series (see `add-lens-blur-1-depth/design.md` - Slices and D16).
Fork-only for now: it is stacked on the colour stack, which is itself waiting for upstream (#63); it goes upstream after
that stack does.

- **Gain-map parameters are read**: the gain-map image's own ISO 21496-1 binary metadata first, else its `hdrgm` XMP (attribute and element forms, one value or one per channel): gain-map min and max (log2), gamma, SDR and HDR offsets, HDR capacity, base rendition. Nothing on the stack reads them today; they are carried as bytes.
- **The gain map is blurred with the photograph**: at the gain map's resolution, the SDR base (the lens blur's input, linear) is reduced to it, the HDR rendition reconstructed (`(SDR + k_sdr) * G - k_hdr`), both blurred with the same radius plane scaled to that resolution (at the photograph's own size, the radii and the SDR render the step has made are reused, so only the HDR rendition is rendered again), and the new gain `G' = (HDR' + k_hdr) / (SDR' + k_sdr)` encoded with the same parameters (clamped to min..max). The gain map's metadata is unchanged, so the export writes it as today (re-encoded, since its pixels changed).
- **A step can rewrite the gain map**: `ImagePipeline` gains a defaulted `run_with_gain_map` (default: `run` and no gain-map change) and `rewrites_gain_map` ("changes the gain map": rewrites or drops it), which the chain uses for a picture with a gain map; only the lens blur implements them. The run cache stores what the step made of the gain map beside the step's pixels (`<step key>|gm`: the rewritten map, or the reason for a drop) and restores it on a hit, treating a hit without it as a miss; every other step keeps today's behaviour (the same gain map, shared).
- **Where the map cannot be rewritten it is dropped**, with the export's "HDR gain map dropped" warning, and the blur still succeeds; a gain map left as it was would lift the old, sharp highlights over the blurred base. The cases: unreadable parameters, an HDR base rendition (`BaseRenditionIsHDR = True`), a gain map larger than its photograph, and a three-channel ISO map whose base-colour-space flag is clear. **Out of scope, recorded**: light adjustment, colour balance and colourisation keep their documented caveat.
- **Found while checking, and not this slice's**: Nora's Lightroom XMP has extended XMP, so the colour stack's export refuses her gain map as too large for a JPEG's standard XMP segment and writes no gain map at all, for a lens blur or for none. Noted as a colour-stack issue.

Crates touched: `opai` (gain-map parameters, the chain's gain-map hook and its cache entry, the lens blur's
implementation). No front-end change: the GUI preview is SDR, and the export already writes whatever gain map the
picture carries.

`rust-sak` coverage found: `rust-sak` carries the gain map and its metadata as bytes (`Metadata::gain_map`,
`iso_gain_map`) and parses neither; parsing `hdrgm` XMP and ISO 21496-1 metadata is done in `opai`, where the stack's
XMP editor already lives (`export/metadata/xmp`). A general ISO 21496-1 parser could move to `rust-sak` later.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `lens-blur`: a gain map is blurred with the photograph.

## Impact

- `opai`: new `image/gain_map_params.rs` (and `Xmp::seq_values` beside `array_len` in the colour stack's XMP parser); `image/gain_map.rs` (`GainMap::params`, `GainMap::rewritten`, `GainMap::shared_pixels`);
  `pipeline/mod.rs` (`ImagePipeline::run_with_gain_map`, default `run` and `None`, and `rewrites_gain_map`, default `false`); `inference/process/mod.rs` (`run`: call it, cache the result as
  `<step key>|gm`, restore it); `InferenceError::GainMap { operation, reason }`; `models/lens_blur/gain_map.rs`; tests in `inference/process/tests/gain_map.rs`.
- Base: `add-color-4-ultra-hdr-gainmap` with slices 1 to 3 duplicated onto it (`jj duplicate`, two overlaps resolved by keeping both sides); slice 4 is not in this stack, and this slice touches no GUI file.
