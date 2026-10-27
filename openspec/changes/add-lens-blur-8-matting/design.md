## Context

The series' decisions are in `add-lens-blur-1-depth/design.md` (D1 to D17) and the matte render's in
`add-lens-blur-7-matte-render/design.md` (M1 to M7). The prototype and its measurements are in
`opai-notes/lens-blur/matting/` (`photos.py` for the trimap, `matte_b_vitmatte.py` for the tiles, `render_matte.py
--matte` for the composite).

On the base: Depth is an analysis family (`Analysis::Depth`, `DataModel for Depth` with `Output = DepthMap`,
`Opai::estimate_depth`), reaching the lens blur by injection in `inference/depth_map.rs` (`find_chain_depth`,
`fill_chain_depth`), estimated on the chain's input so that no later change re-runs it. Lisbon's graphs are fork-only,
installed by `opai-notes/lens-blur/install_lisbon.py` into the models directory with a hand-written `.manifest.json` and
an entry in `models/.fork-listing.json`, which the fork's `fork-model-listing` commit reads.

ViTMatte-S's ONNX export (`Xenova/vitmatte-small-composition-1k`, `onnx/model.onnx`, 103,885,865 bytes, opset 12) has
dynamic input `pixel_values` (1 x 4 x H x W: RGB then the trimap) and output `alphas` (1 x 1 x H x W); the prototype
checked it against PyTorch to 2e-5. The processor rescales RGB to [0, 1] and normalises with mean 0.5 and standard
deviation 0.5, appends the trimap in [0, 1] (0, 0.5, 1) as the fourth channel, and pads to a multiple of 32 with zeros.

## Goals / Non-Goals

**Goals:** hair, fur and leaf edges right under a strong lens blur, with no user input; the matte cached so a change of
aperture re-renders only.

**Non-Goals:** a user mask feeding the trimap (masking slice 5 territory, or a later slice here); per-pixel subject blur
(the subject is one layer: a focus range, M1 of slice 7); the gain map (slice 9); a GUI control.

## Slices

| # | Change | Delivers | Depends on | Base | State |
|---|---|---|---|---|---|
| 1 | `add-lens-blur-1-depth` | Depth family, Lisbon, `DepthMap`, cached data run | - | `main@upstream` | built |
| 2 | `add-lens-blur-2-render` | `imaging` primitives and the layered defocus render | - | `main@upstream` | built |
| 3 | `add-lens-blur-3-operation` | Lens EXIF, Lens Blur operation, injection, catalogue, CLI | 1, 2 | 1 + 2 | built |
| 4 | `add-lens-blur-4-gui` | Panel, f-number slider, click to focus, `lens_info`, i18n | 3 | 3 | built |
| 5 | `add-lens-blur-5-hdr-gainmap` | Gain map blurred and rewritten | 3, colour stack | `add-color-4-ultra-hdr-gainmap` + 1..4 | built |
| 6 | `add-lens-blur-6-live-preview` | Display-size render while dragging | 4 | 4 | pending |
| 7 | `add-lens-blur-7-matte-render` | Subject composite in `defocus`, push-pull fill, blur fusion, morphology, geometric layers | 2 (4 for the stack) | 4 | proposed |
| 8 | `add-lens-blur-8-matting` | Matting family (Porto = ViTMatte-S, fork-only graph), depth trimap, matte injected into the lens blur, `--lb-edges` | 7 | 7 | **this change** |
| 9 | `add-lens-blur-9-matte-gainmap` | The matte composite applied to an Ultra HDR gain map | 5, 8 | 5 + 7..8 | pending, not proposed |

## How the work divides

- `opai`: the family, Porto's pipeline (tiles, normalisation), the matte type and its cache form, the trimap (lens blur
  maths), the injection, the render's subject path.
- `imaging` (slice 7): morphology, fill, fusion, the subject layer.
- `cli`: one flag.
- `gui`: nothing; the catalogue's Matting family has no `order`, so `lib/enhancements.ts` skips it as it skips Depth
  (add `"matting"` to the frontend's `Family` union and a progress label, as Depth's slice noted for `"depth"`).

## Decisions

### X1. A Matting family, and the model's name

An analysis family like Depth: `Family::Matting`, prefix `mt`, label "Matting", in `ALL` after Depth, never applied
(`APPLY_ORDER`'s `const` assertions extended to the first three of `ALL`). One model, **Porto** (ViTMatte-S), a sibling
of Lisbon in the lens blur's naming; the city is user-facing and the maintainer's to rename. A family rather than code
inside the lens blur because masking's edge refinement (`add-masking-5-refine`) runs the same model on a trimap from a
user's mask: the second caller is known now (the project's tier rule).

### X2. Porto's graph

Fork-only, `mt_porto_fp32.onnx`: the Xenova export with the input fixed to 1 x 4 x 1024 x 1024 (`onnx` shape rewrite, as
`moge_fix.py` does for Lisbon; a fixed shape is what Core ML compiles well) and checked against the dynamic graph on a
tile to 1e-4. FP32 only in this slice (104 MB); an FP16 graph is a measurement away (task 4.2 tries it on Core ML and
records the difference). `ArtifactId` `mt_porto_fp32`; `MattingVariant::Porto(FloatPrecision)` with `Fp32` published.
Installed by `opai-notes/lens-blur/matting/install_porto.py` (copied from `install_lisbon.py`), which keeps Kazan's and
Lisbon's entries in `models/.fork-listing.json`.

### X3. Porto's pipeline: tiles over the unknown band

`Matting { variant, trimap: Option<TrimapHandle> }` (the trimap injected like the lens blur's depth, `Arc`-held, its
signature in the cache tag); `DataModel for Matting` with `Output = Matte`. The pipeline:

1. Tiles of 1024 px with 128 px overlap over the source, keeping only tiles that contain an unknown trimap pixel (17 to
   21 on the test photographs).
2. Per tile: RGB in [0, 1], `(v - 0.5) / 0.5`; the trimap as the fourth channel (0, 0.5, 1); a tile that runs off the
   frame padded with zeros (the processor's padding).
3. The alpha of known pixels is the trimap's (0 or 1); unknown pixels take the tiles' alpha, cross-faded over the
   overlap with the prototype's linear feather.

The session is opened once per run and held for all tiles (the depth's ownership model: the data run acquires and
releases it). Progress: one report per tile.

### X4. The matte and the cache

`Matte { width, height, alpha: Arc<[u16]> }` (alpha quantised to 16 bits, held quantised so a served matte equals a
computed one), serde in a compact form as `DepthMap`'s. The data run is keyed `identity_of_data(identity, subject)`
where the subject carries the trimap's signature (XXH3-64 of its bytes). A 24 MP matte is 48 MB raw; its serde form is
measured in task 4.2 and, if a read exceeds 100 ms, stored as a 16-bit PNG instead (the depth's D5 rule).

### X5. The trimap (lens blur maths)

`models/lens_blur/trimap.rs`: from the depth map guided-upsampled to the source's size (D9's filter, as the render does)
and the focus distance: in-focus band = distance from `focus / 1.1` to `focus x 1.5` (tight in front so a nearer person
is not part of the subject; generous behind for hair, fur and leaf tips, which read farther than the face). Sure
foreground = the band eroded by `0.007 x` the long side; sure background = the outside dilated by the same; unknown
between. Morphology at quarter size (`imaging::morph`), nearest-upsampled. Values 0, 0.5, 1 in a `Plane`. Pure,
unit-tested on a synthetic depth step.

### X6. When to matte

Only when it can matter: the lens blur computes its radius range (it already does, for logging) and mattes when the
largest radius magnitude reaches 3 px at the source's size and the band holds at least 0.5 percent of the pixels.
Otherwise the render is slice 3's. `--lb-edges depth` turns it off.

### X7. Injection and the cache tag

`inference/depth_map.rs`, after the depth is filled: for each lens blur that wants a matte (X6), build the trimap and
run `execute(Matting)` through the cached data path, then `with_matte(Arc<Matte>)`. Run on the chain's input (the framed
source), so enhancements before the blur never re-matte; a focus change re-mattes (the trimap moves); an f-number change
does not (unless it crosses X6's threshold). `LensBlur::cache_tag` gains `-a{signature:016x}` when a matte is present
and `-e0` for `--lb-edges depth`. Progress: the matte shares the depth's first slot (its tiles reported within it), so
the chain's step count does not change.

### X8. The render with a matte

`models/lens_blur/render.rs`, when the operation holds a matte: alpha resized bilinearly to the input's size (the input
may be upscaled); background colour filled (`imaging::fill`) where `alpha > 0.004`; disparity filled there and over a
ring of `0.007 x` the long side around it; subject colour by blur fusion (`imaging::fusion`, radii 90 and 6 px scaled by
the diagonal over 7200 px); subject radius from the median disparity of `alpha > 0.9` eroded by the ring's width, which
inside the band is usually under 0.5 px (sharp); spacing `Geometric { first: 1.0, ratio: 1.4 }`; `defocus(...,
Some(Subject { ... }), ...)`. Without a matte, the slice-3 path unchanged (and `Even` spacing, so `--lb-edges depth`
reproduces earlier outputs bit for bit).

### X9. CLI

`--lb-edges <matte|depth>`, default `matte`, only with `--lens-blur`. `docs/CLI_REFERENCE.md`: one line in the Lens Blur
row and a note on what "matte" does and costs. When Porto is not installed (the fork-only graph missing), the run fails
with the usual missing-model error naming `mt_porto_fp32`; `--lb-edges depth` is the way around it.

## Risks / Trade-offs

- **The trimap is only as good as the depth edge** (prototype): strands beyond the unknown band are lost; holes inside
  the subject smaller than the band stay sharp (the bonsai's small gaps); a ground plane crossing the band's far limit
  gets an arbitrary soft edge.
- **The subject is one layer**: everything within the band is sharp. That is a focus range, not optics; at long focal
  lengths a real lens would soften a hood 10 cm behind the face, but the depth is not accurate enough to do that well
  (prototype, column 3).
- **Light specks** where a bright background shows through dense dark strands (blur fusion's limit).
- **Core ML and ViTMatte**: not measured; the windowed-attention ViT may fall back to the CPU (0.8 s a tile, about 15 s
  a photograph). Recorded in task 4.2; acceptable for a first version, since the matte is cached per focus point.
- **Peak memory**: a 24 MP alpha, the filled colour and disparity and the subject colour add about 400 MB to the
  render's 1.2 GB.

## Platforms

Validated on macOS (Apple silicon), Core ML and CPU. Windows and Linux untested; CUDA and TensorRT untested.

## Open Questions

1. **The model's city** (default: Porto).
2. **FP16** (default: FP32 only until measured).
3. **Default edges** (default: `matte`). If Joseph prefers to compare first, `depth` as default for one build is a
   one-line change.
4. **Band ratios** (default: 1.1 in front, 1.5 behind, from the prototype).
