## Context

See proposal.md - Why. This design is the series' own: it records every decision for the whole lens blur (D1 to D17),
and the later slices' design.md files refer back to it rather than repeating it.

State of the code the series builds on (`main@upstream` at `baf684df`):

- **Data runs.** `Analysis` (`models/operation.rs`) is `Copy` and has one arm, `Detection`. The sealed `DataModel`
  contract (`type Output: Serialize + DeserializeOwned + Send`, `as_subject`, `pipeline`) is implemented for `Analysis`
  with `Output = Faces`, so the output is fixed per carrier. `inference/execute.rs::execute<B, O: DataOperation>` is
  already generic; the public `Opai::execute(&Analysis) -> Executed<Faces>` is not. New York
  (`models/detection/newyork/`) is the template for a single-graph data model: `SingleGraph`, `OnOneGraph`,
  `DataPipeline<B>`, `run_named_outputs`.
- **The run cache** (`cache.rs`, `rust_sak::memo` on disk, 24 h TTL) stores images as PNG and values as JSON
  (`put_value`/`get_value`); a data run is keyed `identity_of_data(identity, subject)`. `write_value`'s comment
  anticipates "a data family that produced something large - a per-pixel mask", to be reopened with a measurement.
- **Faces reach face recovery by injection.** `process_with` runs detection through the cached `execute` only when the
  chain holds a face recovery (`inference/faces.rs::find_chain_faces`), then `fill_chain_faces` rewrites the operation
  with the faces, which its cache tag folds in. Depth reaches the lens blur the same way (D6).
- **The chain runs at full resolution.** The GUI's `enhance` and export both run the chain on the full-resolution framed
  picture; the canvas only resizes the result for display. No step is told an output size; a step sees its input pixels.
  A step's parameters are part of its cache tag, so changing one re-runs that step and the ones after it; earlier steps
  are served from the cache.
- **Order.** `Family::APPLY_ORDER` is `Family::ALL` without Detection (asserted in a `const` block), Upscale last
  (asserted by a test).
- **Pixels.** `imaging` has no single-channel plane type, no linear-light conversion (colorization has a private `f64`
  pair), no Gaussian or box filter in public, and parallelises CPU passes with `rows::for_each_row`
  (`std::thread::scope`, no rayon).
- **EXIF.** Nothing in `opai` reads EXIF fields; `image` 0.25 returns a JPEG's, PNG's or WebP's raw EXIF block
  (`ImageDecoder::exif_metadata`); `Picture` carries no metadata on `main@upstream` (the fork's colour stack adds it).

The prototype that settled the numbers is local and not shipped (`opai-notes/blur/`: `moge_onnx.py`, `moge_fix.py`,
`guide_depth.py`, `render_bokeh.py`).

## Goals / Non-Goals

**Goals:**

- A lens blur set by a focus point and an f-number, following a real lens's depth of field for the photograph's own
  camera, last in the enhancement stack, in the GUI and the CLI.
- Depth estimated once per photograph (per framing) and cached; changing the focus or the aperture re-renders only.
- Sharp subject edges (hair, leaves) without the subject's colours bleeding into the blurred background; highlights that
  turn into bokeh discs.
- An Ultra HDR gain map carried correctly through the blur, on the fork's colour stack.
- A seam for masking (#29): the render takes an optional "keep sharp" weight plane, and the depth map can be edited
  before rendering.

**Non-Goals:**

- Masking, a brush, or Los Angeles subject masks (#29); only the seam.
- Filling in what the subject hides (inpainting behind it). The layered render with normalised coverage hides the gap
  well enough at the apertures offered; a strong blur behind a thin subject can show it.
- Lens character: cat's-eye vignetting, onion rings, swirl, polygonal apertures, chromatic aberration. The kernel is an
  anti-aliased disc.
- A focal-length override, a focus-range (in-focus band) control, autofocus on faces. Deferred.
- A live preview that follows the slider while it is dragged: slice 6, not proposed now (D15).
- Hosting Lisbon's graphs in the model repository: the maintainer's call (D17).

## Slices

| # | Change | Delivers | Depends on | Base | State |
|---|---|---|---|---|---|
| 1 | `add-lens-blur-1-depth` | Depth family, Lisbon, `DepthMap`, cached data run, `Opai::estimate_depth` | - | `main@upstream` | **this change** |
| 2 | `add-lens-blur-2-render` | `imaging` primitives: `Plane`, linear light, guided upsampling, nearer-wins dilation, the layered defocus render | - (parallel to 1) | `main@upstream` | built |
| 3 | `add-lens-blur-3-operation` | Lens EXIF, the Lens Blur family and operation (last in the chain), depth injected like faces, catalogue, CLI `--lens-blur`/`--lb`, `--lb-f`, `--lb-focus` | 1, 2 | 1 + 2 | built |
| 4 | `add-lens-blur-4-gui` | Lens Blur panel: f-number slider in third stops, click on the photograph to focus, `lens_info` command, i18n | 3 | 3 | built |
| 5 | `add-lens-blur-5-hdr-gainmap` | Gain map blurred with the same radii as the SDR base and rewritten from the HDR/SDR ratio | 3 (4 for the GUI check), fork colour stack | `add-color-4-ultra-hdr-gainmap` + 1..4 | built |
| 6 | `add-lens-blur-6-live-preview` | A display-size render that follows the slider while dragging | 4 | 4 | pending, not proposed |
| 7 | `add-lens-blur-7-matte-render` | Subject composite in `defocus`, push-pull fill, blur fusion, morphology, geometric layers | 2 (4 for the stack) | 4 | proposed |
| 8 | `add-lens-blur-8-matting` | Matting family (Porto = ViTMatte-S), depth trimap, matte injected into the lens blur, `--lb-edges` | 7 | 7 | proposed |
| 9 | `add-lens-blur-9-matte-gainmap` | The matte composite applied to an Ultra HDR gain map | 5, 8 | 5 + 7..8 | pending, not proposed |

## How the work divides

- `opai`: everything that both front ends need: the depth model and its recovery (slice 1), the lens maths, EXIF
  reading, the operation, the depth injection and the cache (slice 3), the gain-map maths (slice 5).
- `imaging`: pixel primitives with no knowledge of lenses or models (slice 2).
- `cli`: flags and their parsing, mapped onto the catalogue (slice 3).
- `gui`: the panel, the click-to-focus overlay, and one command that answers the photograph's own lens (slice 4); the
  command calls an `opai` function.

## `rust-sak` coverage

- `memo` (the run cache) stores the depth map and the blur results as it stores every other result; `crypto` hashes
  keys. Used unchanged.
- `image` decodes and orients the photograph. It does not read EXIF fields. Reading five EXIF tags is general-purpose
  and could belong in `rust-sak`; it is done here with `kamadak-exif` instead (D7), because `rust-sak` has no EXIF
  module to extend, a new module would be a release on another repository before this feature could land, and the crate
  reads every container the application loads (JPEG, TIFF and TIFF-based RAW, PNG, WebP, HEIF). If the maintainer
  prefers, slice 3's reader moves into `rust-sak` unchanged in behaviour.
- `image-raw` (zenraw): a RAW's EXIF is read from the file by the same crate; nothing from zenraw is needed.

## Decisions

### D1. Two families: Depth (an analysis) and Lens Blur (an enhancement)

Depth estimation is its own data family, `Family::Depth`, prefix `dp`, label "Depth", with one variant, **Lisbon**. The
blur is an image family, `Family::LensBlur`, prefix `lb`, label "Lens Blur", whose one variant is also called Lisbon
because it names the depth model it is driven by; its required artifacts are Lisbon's graphs.

This is the Detection / Face Recovery split again: one family's result is not an image and is never in a chain; the
other consumes it. A user sees one enhancement, "Lens Blur", with one model, "Lisbon". A future feature that wants depth
(fog, relighting, a depth-aware mask) uses the Depth family without going through lens blur.

`Family::ALL` begins with the two families that are never applied (`Detection`, `Depth`), and `APPLY_ORDER` stays
derived from it: its `const` block asserts both, and takes `ALL.last_chunk::<8>()`. Rejected: one family whose model is
run both as an analysis and inside an enhancement, which would make one family belong to both run paths, the thing
`operation.rs` is written to make unwritable.

Module names: `models/depth/` (the family; `inference/depth.rs` is `OutputDepth`, bits per channel, an unrelated name
the module docs disambiguate) and `models/lens_blur/`.

### D2. Lisbon's graphs: two fixed-size FP16 graphs, stretched to fill

MoGe-2 ViT-B (`Ruicheng/moge-2-vitb-normal-onnx`, `model.onnx`, MIT), re-exported by the fork's `moge_fix.py`:
`num_tokens` fixed at **3600**, the image input fixed, `onnxsim` folding the shape arithmetic (149 of 2,220 nodes that
Core ML cannot take), the 40 edge-mode `Pad`s rewritten as `Slice` + `Concat` (exact), converted to FP16 with FP32
inputs and outputs (`keep_io_types`).

- **Two graphs**: `dp_lisbon_landscape_fp16` at 1280x1024 (W x H) and `dp_lisbon_portrait_fp16` at 1024x1280, named with
  `ArtifactId::suffixed(Family::Depth, "lisbon", "_landscape" | "_portrait", Fp16)`. A photograph uses the graph of its
  orientation (square: landscape); only that one is opened.
- **Stretched to fill the graph**: the photograph is resized (anisotropically, to exactly the graph's size) so no pixel is padding and no rectangle is cut from the result; a 5:4 photograph fits the landscape graph in shape, a 3:2 one (5562x3708) is stretched to 1280x1024. The resize is a box prefilter to three times the graph's size and then Lanczos3, which matches a single Lanczos pass (the canopy at 1.8953 m against 1.8953 m in PIL; a triangle filter from twice the frame put it 2.3% farther). The depth is resampled back to the photograph's aspect, **in disparity** (normalised by the nearest distance first: `image`'s resize clamps `f32` samples to 0..1, which turned everything nearer than 1 m into exactly 1 m, Nora's eyes reading 1.000 m, until a unit test at 0.25 m pinned the fix). The recovery (D3) uses the graph's own image plane. Edge replication of a letterbox is the series' first design and is rejected by the measurement below.
- **Measured** (M-series Mac mini, 24 GB, 1280x1024 FP16, 3600 tokens): Core ML (MLProgram, CPU and GPU) 1041/1043 nodes
  in 3 partitions, first load (compile) about 64 s, then **0.4 s** per run; CPU 3.0 s; peak RSS 6.8 GB (including the
  compile) on Core ML, 6.8 GB on the CPU with FP16 (the casts cost memory). Core ML FP16 vs CPU: correlation 0.999998.
  2000 tokens: 0.2 s, but correlation 0.986 with 3600 and a metric distance about 8% off (2.04 m vs 2.22 m on the
  canopy), so 3600 it is. File size 324 MB each (more than half of the 419 MB FP32 original because the folded, resized
  position embeddings become constants).
- **Live, as built** (M4 Pro Mac mini, 24 GB, FP16 graphs from the app's model directory, the three test photographs): Core ML first run 65 to 68 s (the compile, once per graph; 0.68 s where an earlier process compiled it), then 0.62 to 0.68 s end to end, of which the input (box prefilter, then Lanczos3) is 0.14 to 0.20 s, the graph 0.46 s and recovery, resampling and quantisation 7 to 33 ms; a later process loads a compiled graph in about 0.4 s (its first run 0.84 to 1.2 s). CPU 3.0 to 3.3 s end to end, of which the graph is 2.85 to 3.06 s (7 to 9.5 s for the whole call in a test build, which is what the earlier 3.0 s figure does not include). Peak RSS 7.8 GB in a process compiling both graphs, 2.2 GB once compiled, 7.2 GB on the CPU. Against the dynamic graph's canopy 1.90 m, pot 2.60, eyes 0.76, hood face 3.00, the stretched fixed graph gives 1.92, 2.62, 0.78, 3.01 (Core ML) and the same within 0.01 on the CPU. Windows and Linux were not checked.
- **EP profile**: Core ML MLProgram on the CPU and GPU (the Neural Engine compile is slower and gained nothing in the
  Kazan measurements), CPU elsewhere; the existing `cpu_and_gpu_at_fp16` profile.
- **FP16 only for now.** An FP32 graph would suit a CPU-only machine better (lower peak memory than FP16 on the CPU, 2.0
  s for the dynamic FP32 graph at 1600x1280 with 5.0 GB), at another 2 x ~420 MB. Recorded as an open question; the
  variant's precision list makes it a row change later.
- **Letterbox against stretch, measured** (CPU, the FP16 fixed graphs against the original dynamic FP32 graph at the photograph's own aspect, 1280 long side, 3600 tokens; the full table is in `opai-notes/lens-blur/README.md`). The step's rule (use edge replication unless black bars or a stretch beat it by 0.01 in correlation on two of three photographs) looked only at the shape of the depth, which is excellent in every mode (correlation 0.998 or better on the right graph), and so left edge replication. The metric scale is where the modes differ: the median relative difference of metric depth from the reference, edge replication against stretch, is Tokyo (3:2) 14.3% against 1.2% (the hooded face reads 3.57 m against 3.03 m, the reference's 3.00), Nora 7.6% against 3.2%, and the bonsai (5:4, which fills the landscape graph) 0.0% in every mode. Recovered focal, Tokyo 61.9 mm replicated, 52.3 stretched, 50.9 reference; Nora 28.7, 27.2, 27.2. Black bars sit between, and are worse on Nora. A photograph in the other orientation's graph is clearly worse (the bonsai in the portrait graph is 12 to 16% off, 3% stretched), so the graph follows the orientation. **Decision: stretch to fill**, which also needs no cut rectangle. Blur radius depends on depth differences from the focus depth read from the same map, so much of a scale error cancels, but an absolute distance should be right where the focus is asked of one.

Rejected: one dynamic-shape graph (Core ML splits it into 21 partitions); one square graph (wastes a third of the tokens
on a 3:2 photograph); per-aspect graph sets (one compile and 324 MB per aspect); a letterbox with replicated edges or black bars (measured above).

### D3. From the point map to metres

The graph returns `points` (1 x H x W x 3, affine-invariant: correct up to a shift along z), `normal` (unused), `mask` (valid where > 0.5) and `metric_scale` (1). With `uv` MoGe-2's normalised image plane of **the graph** (the photograph is stretched to fill it, D2; corners at `(+-w/d, +-h/d)`, `w` and `h` the graph's size and `d` its diagonal in pixels, pixel centres; the non-square pixels a stretch makes are approximated, as the code's comment says), and a 64x64 nearest-neighbour subsample of the valid pixels:

- For a shift `s`, the projection `p = (x, y) / (z + s)` and the focal `f(s) = sum(p . uv) / sum(p . p)` (closed form);
  the residual `f(s) p - uv`. `s` minimises the sum of squared residuals: a damped Gauss-Newton (Levenberg-Marquardt) on
  the one unknown from `s = 0`, keeping `z + s > 0` for every sample, stopping at a relative change of 1e-3, as MoGe-2's
  `recover_focal_shift` does with SciPy.
- Metric depth `Z = (z + s) * metric_scale` in metres. Invalid pixels, and any with `Z <= 0`, take the largest valid
  depth (they are mostly sky; they then blur as the farthest thing in the photograph).
- The 35 mm-equivalent focal length by the graph's diagonal: `f35 = f * 21.633 mm` (half the full-frame diagonal, 43.27 mm; `f` is in units of the graph's own half-diagonal, which is what the stretch makes of the photograph's), used only when EXIF has none (D7). Measured: the bonsai recovers 28.9 mm against EXIF 36 (-20%), Nora 27.2 against 24 (+13%), Tokyo 50.9 to 52.4 against 181 (3.5 times short: a long lens on a compressed scene, where MoGe's focal is a poor estimate), so the slice's live check is the bonsai's and Nora's within 25%, not Tokyo's.
- The shift's iteration stops on a relative change of the mean distance of 1e-3 after the step (converging at least as far as SciPy's `ftol=1e-3`; the two differ by 4e-6 in shift on the bonsai).
- Too few valid samples (fewer than 64) is `InferenceError::Unmeasurable { operation, valid }`, the run's error and not a silent flat map.

`DepthMap { width, height, metres: Arc<[f32]>, focal_35mm: Option<f32> }` at the graph's resolution resampled (in disparity) back to the photograph's aspect, 1280 on the long side (e.g. 1280x853), **and never larger than the photograph itself** (a tiny or square photograph gets a map no bigger than it, not the graph's 1280x1280), with `dimensions`, `metres_at`, `disparity()` (a plane of `1 / Z`), `focus_metres(fx, fy)` (the median depth in a window of `max(4, 0.5% of the short side)` around a point given as fractions of the frame, robust to a point on an edge) and `signature() -> u64`, computed once and stable.

### D4. The data-model contract per typed operation; one public entry

`Analysis` gains `Depth(Depth)` (still `Copy`): progress, required artifacts, `display_name` and `cache_tag` name a depth run through it as they do a detection. The `DataModel` contract is implemented by the typed operations, `Detection` (`Output = Faces`) and `Depth` (`Output = DepthMap`); `impl DataModel for Analysis` stays, `Output = Faces`, for the existing public `Opai::execute(&Analysis)`, and its pipeline refuses the `Depth` arm with a new `UnsupportedReason::OtherResult { produces: "a depth map" }` (`opai` had no unknown-operation error to reuse). That arm is not unreachable through the public API: the catalogue's `VariantEntry::build` returns an `Analysis::Depth` for a depth row, which is run through `estimate_depth`, not `execute`, and the docs on `build` and `Analysis` say so. The public entry is a sibling of `execute`:

```rust
pub async fn estimate_depth(&self, source: &Picture, depth: &Depth, options: Option<ExecuteOptions>)
    -> Result<Executed<DepthMap>, InferenceError>
```

Rejected: an `AnalysisOutput` enum as `Analysis::Output` (breaks `Executed<Faces>` for every caller); making `Opai::execute` generic (a public signature change for one new caller).

**The sealed `DataModel::pipeline` takes the picture's dimensions** (`fn pipeline<B>(&self, dimensions: (u32, u32))`), which is a change to the contract itself. `Model::sessions` is asked before the run and every session it names is opened, so the only way to open one of Lisbon's two graphs and not both is to build the pipeline for the photograph's orientation. Detection ignores the argument; `execute` passes `source.dimensions()`. The change is about 60 lines and only makes sense with `Depth`, so it is in the same commit.

**New errors**, both additive (the enums are `#[non_exhaustive]`): `UnsupportedReason::OtherResult { produces }` for an `Analysis::Depth` handed to `Opai::execute`, and `InferenceError::Unmeasurable { operation, valid }` (kind `unmeasurable`) for too few valid samples.

**For the upstream PR text**: `Family` and `Analysis` are not `#[non_exhaustive]`, so adding `Depth` (and, in slice 3, `LensBlur`) breaks an exhaustive `match` in a downstream crate. That is intended, and the PR says so.
### D5. The depth map in the run cache

Cached by `execute` like faces: `identity_of_data(picture identity, subject)`, a JSON value through `put_value`/`get_value`. The serialized form is compact: `{ width, height, near, far, focal_35mm, disparity: [u16] }`, disparity `1/Z` quantised linearly between `1/far` and `1/near` (16 bits is far finer than the model's accuracy). The map is held already quantised, so a map served from the cache equals the one computed (checked live).

**Measured** on the 1280x1024 bonsai map: 7,476,268 bytes of JSON, serde encode 7.9 ms, decode 10.0 to 10.7 ms; a served `estimate_depth` takes 10.6 to 11.1 ms end to end. That is far under the 100 ms threshold, so the 16-bit grey PNG fallback was not built.

Per framing: a crop changes the picture's identity, so the depth is re-estimated for the new framing (0.6 s once the graph is compiled), which keeps its coordinates the chain's.

**The key carries no version of the recovery maths or of the graphs**, as a detection's carries none of its model's: a change to either is not seen by a stored map until its 24 h TTL expires, or its operation's tag (`dp-lisbon-fp16`) is changed.

### D6. Depth reaches the blur by injection, like faces

`process_with` gains a sibling of `find_chain_faces`: when the chain holds a `LensBlur`, it runs `Depth` through the
cached `execute` on the chain's **input** picture (the framed source, before any enhancement, so editing denoise or
sharpen never re-estimates depth), and `fill_chain_depth` rewrites each `LensBlur` with the map
(`LensBlur::with_depth`). The operation holds it as `DepthHandle(Arc<DepthMap>)`, whose equality, hash and cache-tag
text are the map's signature and which is skipped when serialized. Its progress is spliced into the chain's bar as
detection's is (`inference/handover.rs`). The lens read from EXIF (D7) is injected the same way (`with_lens`). Slice 3.

### D7. The lens: EXIF, full-frame equivalent, fallbacks

All lens maths is in **35 mm full-frame equivalent** terms: focal length `f35`, f-number `N_eq = N * crop` (crop `= f35
/ f`), circle of confusion measured against the full-frame diagonal. The blur a lens adds is the same fraction of the
frame on every camera at the same equivalent settings, so the slider's f/1.4 looks the same on a phone photograph and a
camera's, and the maths needs no sensor width. (The thin-lens circle of confusion in equivalent terms equals the real
one scaled by the crop factor, so this is exact, not an approximation.)

- **EXIF** (slice 3, `opai::image::read_lens`, with `kamadak-exif`, BSD-2-Clause, reading from the file through its container reader): `FNumber`, `FocalLength`, `FocalLengthIn35mmFilm`, and for the crop when the 35 mm tag is missing, `FocalPlaneXResolution`/`FocalPlaneYResolution`/`FocalPlaneResolutionUnit` with `PixelXDimension` and `PixelYDimension` (not IFD0's `ImageWidth`, which a RAW's preview can own). Bounded and fallible: anything missing or implausible (f-number outside 0.7 to 64, focal outside 1 to 2000 mm) is absent, never an error.
- **Crop**: `f35 / f`; else from the focal-plane resolution (the sensor's diagonal from the pixel dimensions and **both** focal-plane resolutions, Y defaulting to X, so no 3:2 is assumed; crop = 43.267 mm / diagonal), kept only where the EXIF's own image size matches the file's header within 1%; else, when only `FocalLength` is known, from the depth model's own `f35` estimate.
- **Focal length**: EXIF `f35`, else `f * crop`, else the depth model's estimate (D3), else 50 mm.
- **The photograph's own aperture `N0_eq`**: EXIF `N * crop`; when either is missing, **f/8 equivalent** (a typical
  phone photograph is f/6 to f/10 equivalent, a kit-lens camera photograph f/6 to f/9: assuming f/8 adds a little more
  blur than the truth for the deep-focus photographs that have no EXIF, never less than intended for a shallow one).
- Lens EXIF is read from the picture's path (the source file; the EXIF describes the whole frame). **A crop is a smaller sensor behind the same lens**: the framing is the file's diagonal over the picture's, and the EXIF's focal length and f-number are both multiplied by it (as built, `Lens::resolve(exif, depth_focal, framing)`; slice 3's design has the arithmetic). The EXIF block is still read from the file, not from `Picture::metadata()` on the colour stack, which would have changed slice 3's signatures.

The test photographs: bonsai (a6700, 24 mm, f/4, crop 1.5: 36 mm f/6 equivalent), Nora (Pixel, 6.9 mm, f/1.7, crop 3.48:
24 mm f/5.9 equivalent), Tokyo (a6700, 121 mm, f/5.6: 181 mm f/8.4 equivalent).

### D8. How much blur: the thin lens, minus what the photograph already has

Focused at `zf` (the depth at the focus point, D3), a lens of focal length `f = f35` at `N = N_eq` images a point at
depth `z` as a disc of diameter on the full-frame sensor

`c(z) = f^2 * |z - zf| / (N * z * (zf - f))` (equivalently `f^2 zf |1/zf - 1/z| / (N (zf - f))`),

which is linear in disparity `1/z`. In output pixels, with the output's diagonal `D_px` standing for 43.27 mm, the
radius is `r(z) = c(z) / 2 * D_px / 43.27 mm`; **the radius therefore scales with the output size by construction** (an
upscale before the blur gets proportionally larger discs; the preview and the export, both full resolution, match).

The photograph already contains its own aperture's blur, and blur discs add roughly in quadrature, so the blur added is
`r_add(z) = r(z) * sqrt(1 - (N / N0_eq)^2)`, and **nothing** when `N >= N0_eq` (the slider's start: the photograph as it
is). Signed: negative behind the focal plane, positive in front, which orders the layers (D10). Guards: `zf` at least
`1.5 f`; `|r_add|` capped at 6% of the output's diagonal (a foreground at a few centimetres would otherwise ask for
discs larger than the photograph). There is no in-focus band: the depth of field is the lens's.

### D9. Refining the depth to the output: guided upsampling and edge grow

The depth map (about 1280 px wide) is brought to the output resolution at render time, in disparity (`1/Z`, which the
radius is linear in), normalised to 0..1 by its maximum:

- **Fast guided filter** (He and Sun 2015): coefficients `a` (3 per pixel, colour guide) and `b` solved at 1/4 of the
  output resolution with a radius of 24 px on a 6000 x 4000 output (scaled with the diagonal, `24 * D_px / 7200`; 6 at
  the subsampled resolution) and `eps = 1e-3`, box filters by running sums, then bilinearly upsampled and applied to the
  full-resolution guide: `q = a . I + b`. The guide is the input photograph's sRGB values in 0..1. Depth edges then
  follow the photograph's outlines (hair, leaves) instead of the model's soft 1280 px ones.
- **Edge grow**: a grey dilation of the disparity (nearer wins) with a disc of radius `max(1, 3 * D_px / 7200)` px (3 px
  on a 6000 x 4000 photograph), so pixels that mix subject and background stay with the nearer, sharp layer instead of
  smearing the subject's colour into the blurred background. Applied after the focus depth is read, so the focus is the
  depth at the point as estimated.

Both are `imaging` primitives (slice 2); the radii and `eps` are the lens blur's constants (slice 3).

### D10. The render: layered depth of field, scatter as gather, no FFT

From the signed radius plane `s` (output pixels), in linear light:

1. **Layers**: about 14 layer radii spanning `s`'s range evenly, one exactly at 0 (the focal plane). Each pixel's weight
   is shared between its two neighbouring layers with linear (tent) weights summing to 1, so the blur grades smoothly
   with depth instead of stepping.
2. **Per layer**, far to near, over the layer's bounding box (padded by its radius): the premultiplied colour `w * rgb`
   and the coverage `w`, four planes, convolved with an **anti-aliased disc** of the layer's radius; the in-focus layer
   (`|r| < 0.5`) is not convolved.
3. **Composite** "over": `acc = layer + acc * (1 - min(layer.coverage, 1))`; the result is `acc.rgb / acc.coverage`
   (normalised convolution), so a layer's blur never pulls in colour from a layer in front of it, and nearer, sharper
   layers cover what is behind them.
4. **Linear light**: sRGB decoded before (lookup table for 8 and 16 bit), encoded after into the input's channel depth;
   bright points spread into discs instead of greying out. (On the colour stack, the decoding is the working space's
   transfer function; the base stays the same.)

**The disc convolution** is a sum of horizontal runs: a disc of radius `R` is, row by row, a horizontal box of
half-width `sqrt((R + 0.5)^2 - dy^2)` with fractional end weights (the anti-aliasing), so with one prefix sum per row
the cost is `2R + 1` lookups per pixel per plane. **Layers with `R > R_MAX` (12 px) are rendered at a reduced
resolution**: the layer is box-downsampled by `k = ceil(R / R_MAX)`, convolved with radius `R / k`, and bilinearly
upsampled into the box (a blurred layer has no detail finer than `k`); cost per layer is then bounded whatever the
radius. Rows run in parallel through `imaging::rows::for_each_row`. Estimated: 24 MP at full resolution about 1.5 to 3 s
on an M-series CPU (the layers within 12 px of focus dominate), a 2 MP photograph under 0.2 s; measured in slice 2 and
recorded here.

Rejected: FFT convolution (`rustfft`/`realfft`, MIT/Apache, not in the tree; at 24 MP the transforms of four planes per
layer cost more than the run sums, and a reduced-resolution layer is cheap either way); separable or hexagonal box
approximations (polygonal bokeh, visibly not a disc); a gather-only variable-radius blur (cannot let a near, blurred
subject spread over a sharp background, nor stop a sharp subject bleeding into a blurred one).

Memory: the linear photograph, the accumulator and one layer at full resolution, four `f32` planes each: about 1.2 GB
peak at 24 MP. Recorded as a risk; bands of rows are the fallback if it bites.

### D11. The masking seam

The render (`imaging::defocus`, slice 2) takes `keep_sharp: Option<&Plane>`: a weight in 0..1 at any resolution
(resampled to the output), scaling each pixel's radius by `1 - weight` before layering, so a future brush or subject
mask forces areas sharp, partly or wholly. The depth map is a plain value the operation holds (D6), so a future depth
brush edits it before the render without any change here. `LensBlur` has the field (`keep_sharp: Option<PlaneHandle>`),
always `None` and not in the catalogue until masking exists; a test renders with a plane to keep the seam working.
`imaging::mix::blended` is not reused: the blur is not a blend of two results; a mask over it is a radius weight, which
is what the face-recovery plane is to its composite.

### D12. Parameters and the cache tag

- `f_number`: the equivalent f-number to simulate, `FNumber`, a quantised parameter (**thousandths**, the one resolution `quantized_param!` has: the tag spells `n1.4`), range **1.0 to 32**, default **2.8** (the catalogue's default; the GUI starts at the photograph's own, D15).
- `focus_x`, `focus_y`: the focus point as fractions of the framed photograph (0 = left/top), `Fraction`, quantised to thousandths (tag spelling `x0.55`), range 0 to 1, default 0.5 (the centre); the operation holds them as two fields, serialised flat.
- `ParameterValues` gains the three; the catalogue's `ParameterEntry` rows publish them as `Range`s.
- The cache tag: `lb-lisbon-fp16-n{f_number}-x{focus_x}-y{focus_y}-d{depth signature:016x}-l{lens}` where `l` folds in the lens actually used (`f35` to 0.1 mm, `N0_eq` to 0.01, rounded on construction), so an image whose EXIF differs is never served another's blur; a blur that adds nothing and was planned without a depth map reads `d0`. No `_` (the cache-tag rule).

### D13. Last in the chain

`Family::LensBlur` is appended last to `Family::ALL`, so it is applied after Upscale: it is a creative choice rather
than a restoration, every faithful step runs on the sharp photograph, and nothing after it sharpens or denoises the
blur. Its radii are in output pixels (D8), so it is right after an upscale too. The test that Upscale is last becomes
"Lens Blur is last, Upscale just before it". Autopilot never suggests it (the Restore precedent: `isSuggestible`, no
`OPAI_AP_*` variable).

### D14. The CLI

`--lens-blur [MODEL]` with the alias `--lb` (default `lisbon`), `--lb-f <F>` (the equivalent f-number, 1.0 to 32,
decimals allowed, `f/1.4` accepted as `1.4`), and `--lb-focus <X,Y>` (fractions of the frame, default `0.5,0.5`), each
requiring `--lens-blur`. Without `--lb-f`, f/2.8. The CLI has no click; the reference documents how to choose the point
(measure it in a viewer as a fraction of the width and height).

### D15. The GUI

A Lens Blur panel (`LensBlurOptions.tsx`) with:

- An **f-number slider** in third stops (f/1.0, 1.1, 1.2, 1.4, 1.6, 1.8, 2, 2.2, 2.5, 2.8, 3.2, 3.5, 4, 4.5, 5, 5.6, 6.3, 7.1, 8, 9, 10, 11, 13, 14, 16, 18, 20, 22, 25, 29, 32) from f/1.0 to **the first stop at or above** the photograph's own equivalent f-number (not the nearest: a right end below the own f-number would start a new lens blur already blurring), which is the slider's right end and its starting value ("as taken", no added blur), shown as `f/2.8` with "full-frame equivalent" beneath; a note under it when the photograph's own is assumed (no EXIF). The slider's value is the stop's index and the parameter written is the stop's nominal number (f/1.4 is 1.4). The end comes from a new `lens_info(source, crop)` command (an `opai::own_lens(path, framed)` call, EXIF only), so adding the enhancement changes nothing until the slider moves, as asked. The framing matters: a crop is a smaller sensor behind the same lens, so a cropped photograph's own equivalent f-number is higher and the slider ends higher; cropping carries a lens blur across ("as taken" stays "as taken", a stop below the end is kept, and the focus point is moved to the same place on the subject).
- **Click to focus**: the options open in a popover that closes on any click outside it, so focusing is a short mode
  rather than a click "while the panel is open". A **Set focus** button in the panel closes the popover and arms the
  preview: the cursor over the photograph becomes a crosshair, a hint says "Click the photograph where it should be
  sharp", and the next click that does not drag (moved under 4 px) sets the focus point, as fractions of the framed
  photograph computed as the wheel zoom computes its anchor (`imageFraction`), and ends the mode; Escape ends it without
  a change. Panning by drag, wheel zoom and the comparison modes are unchanged; either pane of a comparison can be
  clicked (both are the same photograph's coordinates). A small ring marks the focus point while the mode is armed and
  for two seconds after the click. Rejected: a dialog with its own copy of the photograph, as faces are chosen (a
  smaller picture than the preview, for a choice that wants precision); clicking the preview at any time a lens blur is
  in the stack (a click on the photograph would then silently change the result).
- The slider commits on release (`onValueCommit`, as every slider does); each commit re-runs the blur at full resolution
  (D10's timing). A display-size render that follows the drag is slice 6.

### D16. Ultra HDR gain maps (slice 5, fork colour stack)

With a gain map, the HDR rendition is reconstructed at the gain map's resolution (`HDR = (SDR + k_sdr) * G - k_hdr`, `G = 2^(lerp(min, max, g^(1/gamma)))` per channel from the gain map's own parameters: its ISO 21496-1 binary metadata where it carries it, else its `hdrgm` XMP, the two agreeing on the test photographs to 1e-5), the SDR base is reduced to the same resolution, both are blurred with the same radius plane (scaled to that resolution), and the gain map is rewritten from their ratio, encoded with the same metadata (clamped to its `min`..`max`, which a ratio of blurs stays inside except at rounding). Highlights the gain map lifts then spread into discs on an HDR display. Details, what was built and the cache consequence in slice 5's design.

### D17. Fork-only: installing Lisbon locally

Not part of the upstream change. Until the maintainer decides whether to host Lisbon, the fork installs its graphs as it
installs Kazan's: the files in `models/dp_lisbon_*_fp16/`, a hand-written `.manifest.json`, and entries in
`models/.fork-listing.json`, which the descriptor consults only for an artifact the published listing does not name.
That reader is a fork-only commit (taken from `add-sharpen-kazan`) at the base of the series, dropped before any PR.

## Risks / Trade-offs

- **Depth is wrong where single-image depth is wrong**: glass, mirrors, reflections, thin structures, hair against a
  similar background (Nora's dark hair on a dark, busy background is the hard case). The masking seam (D11) is the way
  out later; for now, the focus point and the f-number are the only controls.
- **Letterboxing could bias depth near the padded edges** (D2): it did, on the metric scale (a 3:2 photograph read 14% off by median); measured before implementing, and the photograph is stretched to fill the graph instead.
- **Full-resolution re-render per slider commit** (D10, D15): 1.5 to 3 s at 24 MP plus the run cache's PNG write of the
  result. Acceptable for a first version; slice 6 makes the slider live.
- **Memory**: depth estimation peaks near 6.8 GB with a first Core ML compile; the render about 1.2 GB at 24 MP.
- **JSON for the depth map** (D5): measured on the 1280x1024 bonsai map, 7.5 MB, encoded in 8 ms and decoded in 10 ms; the PNG fallback was not needed and was not built.
- **The assumed f/8** for photographs with no EXIF can be wrong both ways; the slider still goes to f/1.0.

## Platforms

Validated (by the measurements above and the plan's checks): macOS on Apple silicon, Core ML (CPU and GPU) and CPU.
Untested: Windows and Linux, CUDA, TensorRT and WebGPU (the graph is a standard ONNX graph with fixed shapes, which
TensorRT prefers; WebGPU's node list is not checked).

## Open Questions

- FP32 graphs for CPU-only machines (D2)? Default: no, FP16 only, revisit after a CPU-only report.
- `kamadak-exif`, or a reader in `rust-sak` (D7)? Default: `kamadak-exif`; move it if the maintainer asks.
- The f-number in full-frame-equivalent terms (D7, D15) rather than the lens's own? Default: equivalent, labelled as
  such; with the lens's own, a phone photograph gets almost no blur at any setting.
