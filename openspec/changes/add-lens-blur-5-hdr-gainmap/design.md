## Context

The series' decisions are in `add-lens-blur-1-depth/design.md` (this is D16). On `add-color-4-ultra-hdr-gainmap`:

- `opai::image::GainMap { pixels: Arc<DynamicImage> (Luma8 or Rgb8), metadata: Arc<Metadata> (the gain-map image's own
  xmp and iso_gain_map, bytes), scale: (f64, f64), source: Option<Arc<[u8]>> }`; `reframed(frame)` is the only API that
  replaces its pixels (keeps metadata and scale, clears `source`, restores L8/Rgb8).
- `Picture::derive` re-attaches the source's gain map by `Arc`; the run cache stores pixels only, so a cache hit gets
  the source's gain map back. A test asserts the gain map is pointer-equal after an enhancement.
- Export (`export/gain_map.rs::prepare`) writes the gain map at its own resolution (never enlarged), re-encoding at
  quality 90 or more when `source` is `None`, with the metadata as held.
- Nothing parses `hdrgm` or ISO 21496-1 values. The hawk fixture's gain-map XMP: `HDRCapacityMin 0`, `HDRCapacityMax
  2.3`, `OffsetSDR`/`OffsetHDR 0.015625`, per-channel `GainMapMin` about -0.57, `GainMapMax` 2.4 to 2.7, `Gamma` 0.65 to
  0.87, `BaseRenditionIsHDR False`.

## Slices

| # | Change | Delivers | Depends on | Base | State |
|---|---|---|---|---|---|
| 1 | `add-lens-blur-1-depth` | Depth family, Lisbon, `DepthMap`, cached data run | - | `main@upstream` | built |
| 2 | `add-lens-blur-2-render` | `imaging` primitives and the layered defocus render | - | `main@upstream` | built |
| 3 | `add-lens-blur-3-operation` | Lens EXIF, Lens Blur operation, injection, catalogue, CLI | 1, 2 | 1 + 2 | built |
| 4 | `add-lens-blur-4-gui` | Panel, f-number slider, click to focus, `lens_info`, i18n | 3 | 3 | built |
| 5 | `add-lens-blur-5-hdr-gainmap` | Gain map blurred and rewritten | 3, colour stack | `add-color-4-ultra-hdr-gainmap` + 1..4 | **this change** |
| 6 | `add-lens-blur-6-live-preview` | Display-size render while dragging | 4 | 4 | pending |
| 7 | `add-lens-blur-7-matte-render` | Subject composite in `defocus`, push-pull fill, blur fusion, morphology, geometric layers | 2 (4 for the stack) | 4 | proposed |
| 8 | `add-lens-blur-8-matting` | Matting family (Porto = ViTMatte-S), depth trimap, matte injected into the lens blur, `--lb-edges` | 7 | 7 | proposed |
| 9 | `add-lens-blur-9-matte-gainmap` | The matte composite applied to an Ultra HDR gain map | 5, 8 | 5 + 7..8 | pending, not proposed |

## How the work divides

`opai` only: parameters in `image/`, the hook in `pipeline/` and `inference/process/`, the maths in `models/lens_blur/`.

## Decisions

### H1. The maths (SDR base, ISO 21496-1 / Adobe `hdrgm`)

Per channel `c` (one channel for a single-channel map, applied to all three), with `g` the stored gain-map value in
0..1:

- `log2 G = min_c + (max_c - min_c) * g^(1 / gamma_c)`; `G = 2^(log2 G)`.
- `HDR_c = (SDR_c + k_sdr_c) * G_c - k_hdr_c`, `SDR_c` linear (sRGB transfer decoded; the base's primaries are
  irrelevant to a ratio per channel).
- After blurring both: `G'_c = (HDR'_c + k_hdr_c) / (SDR'_c + k_sdr_c)`; `g' = ((clamp(log2 G'_c, min_c, max_c) - min_c)
  / (max_c - min_c))^gamma_c`; stored as 8-bit.
- A single-channel map: `HDR` and `SDR` are reduced to luminance (BT.709 weights on the linear values) before the ratio, so `G'` stays one channel (both renditions packed as two channels of one render). A channel whose `max` equals its `min` keeps its stored values.

A blurred ratio is a weighted mean of the ratios under the disc, weighted by SDR brightness, so it stays within the
original min..max except for rounding; the clamp is a guard, and the metadata is unchanged.

### H2. At the gain map's resolution

The lens blur's input (linear SDR, output resolution `W x H`) is box-reduced to the gain map's `w x h` (exact area weights, any ratio); the radius plane is reduced the same way, by the largest magnitude per cell (not the box average, so a cell across a focus edge takes the blur of one side), and multiplied by the diagonal ratio; the same `keep_sharp`. Both `SDR` and `HDR` are rendered there with `imaging::defocus`. **Where the gain map is the photograph's own size** (a Lightroom gain map is full resolution) nothing is reduced and the radii, the linear decode and the blurred SDR the step has already made are reused, so the cost on top of the SDR render is the HDR render and the map's decode and encode; an Android map at a quarter costs little. Rejected: rendering HDR at full resolution and deriving the gain map from a full-resolution ratio (a render at a size the map discards).

### H3. The hook and the cache

```rust
// pipeline/mod.rs, on ImagePipeline<B>:
fn rewrites_gain_map(&self, gain_map: &GainMap) -> bool { false } // changes the gain map: rewrites or drops it
fn run_with_gain_map(&self, input, gain_map: &GainMap, sessions, depth, progress, cancelled)
    -> (DynamicImage, Option<Result<GainMap, InferenceError>>) { (self.run(...), None) }
```

`process::run`, for a picture with a gain map, calls `run_with_gain_map` in place of `run` for each step and, on `Some(Ok(map))`, keeps that map for the following steps and the result (`derive(...).with_gain_map(Some(map))`), and writes it to the cache under `<step key>|gm` (PNG, L8 or Rgb8). On `Some(Err(e))` a stop fails the step; any other error drops the gain map with the existing "HDR gain map dropped: it could not be carried" warning and the run continues, and the reason is stored under `<step key>|gm` as a JSON string (which no PNG parses as), so a hit serves the stored drop and drops again. On a cache hit for a step whose pipeline `rewrites_gain_map`, the `|gm` entry is read back; if it is missing (an older cache), the step is treated as a miss. Every other step returns `None` and the existing pointer-equality test still holds for them. New error: `InferenceError::GainMap { operation, reason }` (kind `gain_map`).

### H4. Parameters

`image/gain_map_params.rs`: `GainMapParams { min, max, gamma, offset_sdr, offset_hdr: [f32; 3], capacity_min, capacity_max: f32, base_is_hdr: bool, channels: u8 }` from the **ISO 21496-1 binary metadata first** (version, flags, common denominator, per-channel fractions) and otherwise from `hdrgm` XMP (attributes or elements; an `rdf:Seq` of three or a single value; defaults per the Adobe specification: min 0, gamma 1, offsets 1/64, base SDR). The ISO flag `0x40` is read into `in_base_colour_space` (XMP: always true). Parameters that cannot be read, an HDR base (`BaseRenditionIsHDR = True`), a gain map larger than its photograph, and three channels with `in_base_colour_space` clear: **the gain map is dropped from the result with the export's warning** (the step returns `InferenceError::GainMap`), and the lens blur still succeeds (it never fails the run).

## As built

What the implementation settled, including the review's decisions. The change is one commit (`feat(lens-blur): Blur an Ultra HDR gain map with the photograph and rewrite it from the HDR/SDR ratio`) on slice 3's CLI commit as duplicated onto the colour stack (`opai-notes/lens-blur/merge-slice5-duplicate.md`: two overlaps, both "keep both sides", in `inference/process/tests/mod.rs` and `tests/public_api.rs`; the colour stack's `[patch]` `26.10.1-meta.6` kept; 2785 tests green before any new code). Slice 4 is not in this stack: this slice touches no GUI file and should rebase over it without conflict.

- **Gain-map parameters prefer ISO 21496-1 over XMP.** `GainMapParams` is read from the gain-map image's own ISO 21496-1 payload first (big-endian; versions, flags `0x80` multichannel, `0x08` common denominator and `0x04` backward; headroom fractions; per channel min, max, gamma, base and alternate offsets; the base is HDR when the backward flag is set or its headroom exceeds the alternate's), and from its `hdrgm` XMP (attribute or element form, a single value or an `rdf:Seq`, Adobe defaults; `GainMapMax` and `HDRCapacityMax` required, through the colour stack's XMP parser, to which `Xmp::seq_values` was added beside `array_len`) only where there is no ISO payload or it cannot be read. The binary payload is the standard's own, and the two agree to 1e-5 on the hawk fixture and on the test photographs. `use_base_colour_space = false` (the gain applied in the alternate rendition's primaries) is not honoured: the ratio is taken in the base's linear RGB, which is what the hawk fixture (Lightroom) uses; the XMP, present in all three test photographs, has no such flag.
- **Where the map cannot be rewritten, it is dropped**, with the export's wording ("HDR gain map dropped: it could not be carried", `warn`, with the step's index and the reason); the blur itself succeeds and the result carries no gain map, so no HDR display shows a sharp gain over a blurred base. This replaces the first build's "kept unblurred with a warning", which left exactly the ghosts the slice exists to remove. The cases: unreadable parameters; an HDR base rendition (`BaseRenditionIsHDR = True`, or an ISO base that is HDR); a gain map larger than its photograph; and three channels with the ISO base-colour-space flag clear (the gain applied in the alternate rendition's primaries, which the ratio here does not honour; a single-channel map is still rewritten). `rewrites_gain_map` answers true for all of them, false only where the blur adds nothing.
- **At the photograph's own size, nothing is reduced and nothing is rendered twice.** The radii, the linear decode and (for a full-size map) the blurred SDR are reused within the one `run_with_gain_map` call and freed on return, so a Lightroom map (full resolution; all three test photographs) costs the HDR render and the encode on top of the SDR one, not two more renders. The full-resolution radius plane is reduced for a smaller map by the largest magnitude per cell (`largest_reduced`), not box-averaged. At a smaller gain map (Android's, a quarter), the step's input is box-reduced to the map's size by exact area weights (any ratio), the full-resolution radius plane is reduced the same way and multiplied by the diagonal ratio, and both renditions are rendered there with the same `keep_sharp`.
- **The maths** is `models/lens_blur/gain_map.rs`, pure functions: `log2 G = min + (max - min) g^(1/gamma)`, `HDR = (SDR + k_sdr) G - k_hdr` on linear SDR, both blurred by `imaging::defocus` with the same radius plane, `G' = (HDR' + k_hdr) / (SDR' + k_sdr)`, `g' = ((clamp(log2 G', min, max) - min) / (max - min))^gamma`, rounded to eight bits. Three-channel maps are one render per channel of each rendition; a single-channel map takes the luminance (BT.709) of the SDR and the HDR packed as two channels of **one** render. A channel with `max = min` keeps its stored values. Every eight-bit value round-trips through decode, HDR and back for the hawk's parameters, and nothing blurred gives every value back exactly (an in-focus region is unchanged within one code).
- **Two hook methods, not one.** `ImagePipeline::run_with_gain_map(&self, input, gain_map, sessions, depth, progress, cancelled) -> (pixels, Option<Result<GainMap, InferenceError>>)` (default: `run` and `None`) and `ImagePipeline::rewrites_gain_map(&self, gain_map) -> bool` (default `false`; now "changes the gain map", rewrites or drops): the cache must know, for a step it serves, whether a `|gm` entry is required without running the step. The chain calls `run_with_gain_map` in place of `run` when the picture carries a gain map. Chosen over a stash on the pipeline (a shared `Arc` that may serve more than one run, so a stash would need keying and invalidation and would hold a full-resolution float buffer between calls). `cancelled` is passed because the rewrite is a full render at the photograph's size, which a slider drag must be able to stop; a stop fails the step as one during `run` does.
- **`GainMap::rewritten(pixels)`** (as `reframed`: metadata and scale kept, `source` cleared, L8 or Rgb8 kept) and `GainMap::shared_pixels()`.
- **The chain.** `process::run` carries `Option<Arc<GainMap>>` through the steps and returns it; `process` puts it on the result with `Picture::with_shared_gain_map`, so an unchanged one stays the source's `Arc` (the existing pointer-equality test holds for every other step). On a miss the hook is called on the blocking thread right after `run` and a replacement is queued to the store under `<step key>|gm` (PNG, beside the pixels' entry). On a hit, a step that `rewrites_gain_map` is served only with its `|gm` entry (read and decoded on the walk, restored on a rewind); without one it is a miss ("a stored step's gain map is not stored beside it; running the step again", `debug`), which is also what happens to pixels an earlier build stored without it. Steps after it are still served from the store. What a step made of the gain map is stored under `<key>|gm` either way: the rewritten map (PNG) or, for a drop, the reason as a JSON string, so a stored drop is served and drops again (no endless rerun).
- **The rewrite has its own share of the progress bar**, by the pixels it renders against the photograph's (half the render share for a Lightroom map); the bar is monotone, ends at 1 and reports the rewrite's layers after the photograph's.
- **Task 1.1's "the lens reader takes the EXIF block from `Picture::metadata()`" was not done**: the lens is still read from the source's file (the same EXIF for every loaded photograph); doing it changes `find_chain_depth`'s signature inside the duplicated slice 3, which slice 4 has still to be stacked onto.
- **Tests** (`TZ=UTC cargo test --workspace`: 2805 passed, 0 failed, from 2785; fmt and clippy `-D warnings` clean): parameters (the hawk's XMP equals its ISO payload, element form, single channel, a single-channel ISO payload with a common denominator, every truncation of the ISO payload refused, garbage, missing and out-of-range XMP refused); the maths (the round trip per channel, out-of-range gains clamp, uniform in is uniform out, nothing blurred is identical, the single-channel luminance ratio, a highlight blooms into a disc dimmer than its peak, box reduction); the pipeline (the spec's highlight scenario at full and at quarter resolution with the square in focus unchanged within 1, single-channel stays L8, unchanged at the own f-number, unreadable parameters drop the map with one warning, an HDR base is kept with one warning); the chain with a fake rewriting pipeline (the result and a following upscale carry the replacement; a failed rewrite drops it with the warning; a second run is served with the same replacement and the hook not asked; pixels stored without `|gm` rerun the step) and with the real lens blur on the fake backend (fresh and cached carry the same rewritten map, more of it lifted; a denoise alone keeps the source's `Arc`); the export re-encodes a rewritten map with its metadata.

### Timings

M4 Pro Mac mini, release CLI, f/1.4, depth served from the cache; `/usr/bin/time -l` and `opai.log` at `debug`. All three photographs carry full-resolution RGB gain maps. **The table is the first build's**, which rendered the gain map's SDR a second time: two more full renders plus the decode and the encode of the map, about 2.4 times the SDR render. After the review fixes (the radii, the linear decode and the blurred SDR reused), bonsai at f/1.41 to force a miss: rewrite **0.94 s** (was 1.60 s), total **2.11 s** (was 3.09 s), peak RSS **2.59 GB** (was 2.46 GB: the linear photograph is kept through the render for the rewrite); served second run 0.77 s / 0.82 GB. Nora and Tokyo were not re-measured.

| Photo | SDR render | Gain-map rewrite | Total (first) | Peak RSS | Total (second, served) | Peak RSS |
|---|---|---|---|---|---|---|
| bonsai 4815x3852 | 0.66 s | 1.60 s | 3.09 s | 2.46 GB | 0.78 s | 0.73 GB |
| Nora 3064x4080 | 0.55 s | 1.25 s | 2.22 s | 1.62 GB | 0.57 s | 0.66 GB |
| Tokyo 5562x3708 | 0.52 s | 1.35 s | 2.49 s | 2.15 GB | 0.75 s | 0.76 GB |

The first run of each was also the older-cache case: slice 3's runs had stored the lens blur's pixels without a `|gm` entry, and the log shows the step treated as a miss and rerun.

### Validation

Outputs in `opai-notes/lens-blur/out/` (`*-hdr-f1.4.jpg`), nothing derived committed.

- `exiftool -validate -warning -a`: bonsai 3 warnings (IPTCDigest not current, as the source; missing ComponentsConfiguration and YCbCrPositioning), Tokyo 2 (the last two; the source has those and two more). `exiftool -MPF:all`: 2 images, the second a Gain Map Image (bonsai 1,175,099 bytes against the source's 1,595,521, Tokyo 1,192,709 against 3,326,259: a blurred map compresses better). `ultrahdr_app -m 1 -j <file> -P` reports `Ultra HDR Image: Yes` for bonsai and Tokyo.
- **Nora's output has no gain map, and that is the colour stack's, not this slice's.** The CLI prints "HDR gain map dropped: it could not be carried": the export says the XMP that declares it is too large for a JPEG's standard XMP segment (her Lightroom XMP has extended XMP). A no-op `--lb-f 8` on Nora gives the same warning. The lens blur did compute her rewritten map (1.25 s). It is an issue to note against the colour stack's export (`export/gain_map.rs::prepare` and the XMP writer), where it can be fixed once for every enhancement, and not here.
- Gain maps compared (`exiftool -b -MPImage2`, input against output): on the bonsai the mean change is 6.46/255 where the SDR changed by more than 8/255 (27.5% of pixels) against 0.64/255 where the SDR is unchanged (7.3%; the quality-90 re-encode), and 1.33/255 in a 192 px square at the focus point; Tokyo 16.0/255 in the blurred area (74.9%) and 1.5/255 at the focus point. The decisive number is the detail (mean absolute Laplacian of log2 luminance) in the blurred area: bonsai SDR out 0.078; the HDR rendition of the blurred SDR with the **source's** gain map 0.127 (the ghost: a sharp gain over a blurred base); with the **rewritten** one 0.083, matching the SDR. Tokyo 0.039, 0.080 and 0.038. Mean HDR luminance in the blurred area is kept (bonsai 0.164 to 0.167, Tokyo 0.374 to 0.363) while its peak drops (bonsai p99.9 4.93 to 3.73; Tokyo 4.81 to 1.11, radii up to 400 px): the highlights spread into discs. Visually the background of the map is blurred like the SDR, small bright points have become discs (the bonsai's floor sparkles; Tokyo's lit doorway is one large disc), and the tree, pot, hood and sign keep their sharp gain.
- The check on an HDR display in macOS Preview and Photos is Joseph's (`opai-notes/TESTING.md`, "Lens blur (2026-10-27) (HDR gain map)"), and its result is not in yet.

## Risks / Trade-offs

- An 8-bit re-encode of the gain map, at quality 90 or more: small banding in smooth bokeh is possible; the gain map was
  8-bit already.
- Single-channel maps lose the per-channel ratio in blurred mixtures of different colours (luminance ratio); invisible
  in practice.
- The SDR preview in the GUI does not show any of this; it is checked on an HDR display in Preview and Photos.
- The cache keeps a second entry (a PNG, or a drop's reason) per step that changes the gain map, and the peak memory is higher while the linear photograph is kept for the rewrite.
- A gain map dropped (unreadable parameters, an HDR base, a map larger than its photograph, three channels in the HDR rendition's colour space) is a loss of HDR from the export, which is what the export's own warning says; keeping it unblurred would have shown sharp ghosts.

## Platforms

macOS (Preview, Photos, `ultrahdr_app`), on Joseph's Ultra HDR JPEGs (bonsai, Nora, Tokyo all carry gain maps). Others
untested.
