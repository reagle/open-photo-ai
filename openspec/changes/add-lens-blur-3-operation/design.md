## Context

The series' decisions are in `add-lens-blur-1-depth/design.md` (D1 to D17); this slice implements D6 (injection), D7
(lens), D8 (radius), D12 (parameters, cache tag), D13 (order) and D14 (CLI), on slices 1 and 2. The Restore family on
the fork (`add-restore-family`) is the precedent for a family appended last, excluded from Autopilot, with CLI flags;
Face Recovery's faces are the precedent for a data result injected into a chain operation.

## Slices

| # | Change | Delivers | Depends on | Base | State |
|---|---|---|---|---|---|
| 1 | `add-lens-blur-1-depth` | Depth family, Lisbon, `DepthMap`, cached data run | - | `main@upstream` | built |
| 2 | `add-lens-blur-2-render` | `imaging` primitives and the layered defocus render | - | `main@upstream` | built |
| 3 | `add-lens-blur-3-operation` | Lens EXIF, Lens Blur operation, injection, catalogue, CLI | 1, 2 | 1 + 2 | **this change** |
| 4 | `add-lens-blur-4-gui` | Panel, f-number slider, click to focus, `lens_info`, i18n | 3 | 3 | built |
| 5 | `add-lens-blur-5-hdr-gainmap` | Gain map blurred and rewritten | 3, colour stack | `add-color-4-ultra-hdr-gainmap` + 1..4 | built |
| 6 | `add-lens-blur-6-live-preview` | Display-size render while dragging | 4 | 4 | pending |

## How the work divides

`opai` holds everything: EXIF, lens maths, the operation, its pipeline, the injection. `cli` maps three flags onto the
catalogue's parameters. The frontend union gains `"lens_blur"` only so that it type-checks (slice 4 adds the panel).

## Decisions

### O1. Types

```rust
// image/lens.rs (re-exported as opai::image::read_lens)
pub struct LensExif { pub focal_mm: Option<f32>, pub focal_35mm: Option<f32>, pub f_number: Option<f32>,
    pub sensor_diagonal_mm: Option<f32>, pub diagonal_px: Option<f32> }
impl LensExif { pub fn checked_against(self, file_diagonal_px: f32) -> Self; } // drops a stale sensor
pub fn read_lens(path: &Path) -> LensExif; // never fails; absent fields are None
// models/lens_blur/lens.rs
pub struct Lens { pub focal_35mm: f32, pub own_f_number: f32, pub origin: LensOrigin } // equivalent terms, rounded
pub enum LensOrigin { Exif, Estimated, Assumed }
impl Lens { pub fn resolve(exif: &LensExif, depth_focal_35mm: Option<f32>, framing: f32) -> Self; }
pub(crate) fn added_radius(lens: &Lens, f_number: f32, focus_m: f32, depth_m: f32, diagonal_px: f32) -> f32; // signed
// models/lens_blur/mod.rs
pub struct LensBlur { variant: LensBlurVariant, f_number: FNumber, focus_x: Fraction, focus_y: Fraction,
    depth: Option<DepthHandle>, lens: Option<Lens>, keep_sharp: Option<PlaneHandle> }
impl LensBlur { pub fn lisbon(f_number: FNumber, focus_x: Fraction, focus_y: Fraction) -> Operation;
    pub fn with_depth(self, map: Arc<DepthMap>) -> Self; pub fn with_lens(self, lens: Lens) -> Self; }
```

`DepthHandle` and `PlaneHandle` compare, hash and print by a signature and are skipped by serde (a serialized lens blur carries its parameters, as a serialized face recovery carries no pixels). `Lens` is resolved where the depth map is known, because the depth model's focal estimate is a fallback; `LensOrigin` lets slice 4 say "assumed" next to the slider. As built, the sensor is a diagonal and the framing a factor: see As built.

### O2. The pipeline (`models/lens_blur/render.rs`)

An `ImagePipeline` with no session of its own (its required artifacts are the depth model's, so installs and the "needs
model" checks work; the chain acquires no session for it):

1. A lens blur without its injection is refused when the chain is planned (`UnsupportedReason::Uninjected`, before anything installs), and a depth-less one is accepted only where it adds nothing (`Blur::adds_nothing`).
2. `zf = depth.focus_metres(focus)`; the disparity plane `1/Z` from the map, normalised by its maximum.
3. Guided upsampling to the input's size (radius 24 px scaled by `diagonal / 7200`, `eps = 1e-3`), then the edge grow
   (`3 px * diagonal / 7200`), rescaled back to `1/m`.
4. The signed radius per pixel from D8 (`added_radius`), capped; all zero when `f_number >= own_f_number`, and then the
   input is returned unchanged without decoding it.
5. `imaging::defocus` in linear light, `keep_sharp` passed through; encoded to the input's channel depth.

Progress: the pipeline reports through the existing progress callback: upsampling 10%, layers 80% (per layer), encoding
10%.

### O3. Injection (`inference/depth_map.rs`)

Mirrors `inference/faces.rs`: `find_chain_depth` takes the chain's input picture and its dimensions, and when the chain holds a `LensBlur` it reads the lens (`read_lens(source.path())` on a blocking thread), reads the file's header for the framing, checks the EXIF against the file's frame, and resolves the lens (`Lens::resolve(exif, depth_focal, framing)`); where the EXIF alone shows every lens blur is opened no wider than the photograph's own aperture it hands the blurs the lens and stops (no depth estimate; see As built); otherwise it runs `Depth::lisbon()` through the cached `execute` on the chain's input picture (its progress spliced in as detection's is, `handover.rs`) and `fill_chain_depth` rewrites each `LensBlur` with the map and the lens. A depth failure fails the run with its error (unlike faces, there is no lens blur without depth). A picture with no path (a perf sweep or a test) gets an empty `LensExif` and the whole frame.

### O4. Order and Autopilot

Appended last to `Family::ALL` (D13). Autopilot: excluded from the suggestible families exactly as the fork's Restore is
(tests in `autopilot/mod.rs` and `scope.rs`); no `OPAI_AP_LENS_BLUR`.

### O5. CLI

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)] pub enum LensBlurModel { Lisbon }
#[arg(long = "lens-blur", visible_alias = "lb", value_name = "MODEL", num_args = 0..=1,
      default_missing_value = "lisbon", ignore_case = true)]
pub lens_blur: Option<LensBlurModel>,
#[arg(long = "lb-f", value_name = "F", value_parser = parse_f_number, requires = "lens_blur")]
pub lens_blur_f: Option<f64>,   // "1.4" or "f/1.4"; 1.0 to 32
#[arg(long = "lb-focus", value_name = "X,Y", value_parser = parse_focus, requires = "lens_blur")]
pub lens_blur_focus: Option<(f64, f64)>, // fractions 0..1
```

`chain.rs` pushes it last through the catalogue (`set(values, FNumber::NAME, ...)`, `focus_x`, `focus_y`). `-q hd|sd` picks the precision by position; Lisbon has FP16 only, so both give FP16 (checked, and documented in the reference). The two value parsers, `--lb-focus`'s hyphen handling and the errors are under As built.

## As built

What the implementation settled differently from the text above, what the review changed, and what was measured. The library is the first commit (`feat(lens-blur): Add Lens Blur ...`), the CLI the second.

- **No change to slice 1 for the signature.** `DepthHandle` computes `DepthMap::signature()` once, when the map is injected, and compares, hashes and tags by the stored value, so the 5 MB hash runs once per chain and not once per comparison.
- **`FNumber` and `Fraction` are both quantised to thousandths**, not hundredths: `quantized_param!` has one resolution (`STEPS_PER_UNIT = 1000`) for every bounded parameter. The tag spells `n1.4` and `x0.55`. Each is in its own file (`models/f_number.rs`, `models/fraction.rs`); `Fraction::NAME` is `"fraction"`, the catalogue names the two coordinates `focus_x` and `focus_y` (`opai::FOCUS_X`, `FOCUS_Y`), and `LensBlur` holds `focus_x` and `focus_y` fields (serialised flat, matching the parameter names) with a `focus()` getter for the tuple.
- **`read_lens` is `opai::image::read_lens`** (module `image/lens.rs` private, its items re-exported as every `image` item is).
- **The sensor is a diagonal, from the EXIF's own sizes.** `LensExif.sensor_diagonal_mm` comes from `PixelXDimension` and `PixelYDimension` (not IFD0's `ImageWidth`, which a RAW's embedded preview often owns) and **both** focal-plane resolutions (Y defaulting to X), so no 3:2 is assumed: crop = 43.267 mm / diagonal. `LensExif.diagonal_px` is the EXIF's own image size, and `LensExif::checked_against(file diagonal)` drops the sensor where it differs from the file's header by over 1% (a resized file keeping a stale resolution). A RAW whose EXIF size is not its decoded size therefore falls back to the depth model's estimate.
- **`Lens` is rounded on construction** (the focal length to 0.1 mm, the own f-number to 0.01) and compares and hashes by those bits, so two lenses that tag alike blur alike; `origin` is not in the identity. `LensOrigin::Estimated` is the EXIF's f-number with a crop from the depth model's focal; `Assumed` is the own f-number f/8.
- **A framing (crop) factor in `Lens::resolve(exif, depth_focal, framing)`.** The EXIF describes the whole frame, but the crop is applied by the front end before `process`, so the library sees only a smaller `Picture` whose path is still the whole file's. `find_chain_depth` takes the picture's dimensions, probes the file's header (`image::read_frame_diagonal`; a RAW is probed whole, as `probe_raw_blocking` reads it; a picture with no file is the whole frame) and passes `framing = file diagonal / picture diagonal` (1 within 1%, never below 1, and a ratio that is not finite or is under 1 is 1). **A crop is a smaller sensor behind the same lens**, so the EXIF's focal length and f-number are both multiplied by `framing`: the blur radius goes as `F^2 / N` times the diagonal, and `k^2 / k / k = 1`, so the same physical aperture blurs the same scene by the same pixels, while asking `f/N` of a crop is asking `f/(N/k)` of the whole frame and the cutoff moves with it (f/6 becomes f/12 on a 2x crop). The depth model's own focal estimate, and the crop factor derived from it, are of the cropped picture already and are not scaled again; the assumed f/8 is. The crop therefore changes the identity, so depth is estimated again, and the blur is no longer under-stated on a heavy crop. The GUI needs nothing: it already hands `process` the cropped picture with the source's path.
- **A lens blur planned without its injection is refused at plan time** with a new `UnsupportedReason::Uninjected { missing }`, rather than failing inside the run (O2 step 1): before anything installs. `every_operation()` therefore holds injected lens blurs.
- **A no-op skips the depth estimate.** After reading the EXIF and the framing, `Lens::without_depth` gives the lens where the depth model could not change the own f-number (everything except "the EXIF has a focal length and an f-number and nothing for the crop factor"); if every lens blur in the chain is at or above it, the blurs are handed the lens only, `estimate` is not called, and `LensBlur::pipeline` accepts a depth-less blur exactly then (`Blur::adds_nothing`), returning the input untouched; otherwise `Uninjected` as before. Cache tags of such a blur read `d0`. `CLI_REFERENCE.md` says so in a sentence.
- **`LensBlur::profile()` is its depth model's** (Lisbon's), not the default: its `required_artifacts` are Lisbon's graphs, and the cross-family test that one artifact has one profile holds only that way. Two cross-family tests (artifact prefix, model tag) make the lens blur's exception explicit: its artifacts are `dp_`.
- **Telemetry files a lens blur under `lb_lisbon_fp16`** (`LensBlur::model_tag`), not under the depth model's `dp_lisbon_*` artifact.
- **The pipeline reports per-layer progress through `defocus`'s `cancelled` callback** (it is asked once per layer, and slice 2's API has no progress callback); `stages()` is 0. The radius step builds the f32 guide only after `Blur::new` has decided there is something to add.
- **`process_with` takes a second closure, `estimate`**, beside `detect`; the tests for "depth runs once" go through the real `process`, the real Lisbon pipeline and the real cached run on the process tests' fake backend, which gained `run_named_outputs` answering a synthetic point map. A chain of a face recovery and a lens blur reports one bar that only moves forward (tested).
- **The depth's progress label in the CLI is "Lens Blur"** (it was "Depth" from slice 1), as detection reads "Face Recovery"; `events.rs` spells the family `lens_blur`. `perf` never measures a lens blur (new `SelectionError::NoModel`; `lisbon` names the depth row for a bare name), and its codename-uniqueness test skips the lens blur row, whose codename is the depth model's.
- **The frontend `Family` union is not extended here**: adding `"lens_blur"` makes `EnhancementRow.tsx`'s exhaustive switches and two `Record<Family, ...>` tests fail, so it needs the panel and is left wholly to slice 4 (the catalogue entry's `order` reaches `lib/enhancements.ts`'s apply-order map harmlessly).
- **Surface.** `LensBlur::with_keep_sharp` is `pub(crate)` (a dead-code `expect` outside tests), `ASSUMED_F_NUMBER` is `pub(crate)` and the public docs say "f/8".
- **CLI, as built.** `LensBlurModel` has one value, `lisbon`; `--lb-f` takes `1.4` or `f/1.4` (`F/` too) from `FNumber::MIN` to `FNumber::MAX`; `--lb-focus` takes two fractions with a comma, and has `allow_hyphen_values`, so `-0.1,0.5` reaches the parser and is refused with the range (`focus_x must be between 0 and 1 inclusive, but 1.2 was supplied`, new `RangeError::named(parameter)`) rather than as clap's "unexpected argument". Both require `--lens-blur`, a lens blur counts in `Selection::is_empty`, conflicts with `--autopilot`, and is pushed last in `chain.rs`. The model trait `plan` builds from is split from the autopilot one so Lens Blur can name its family without an `OPAI_AP_LENS_BLUR`, which does not exist. `-q hd` and `-q sd` both give FP16. `CLI_REFERENCE.md` has the summary row, a section (the lens maths in one paragraph, full-frame-equivalent f-numbers, the EXIF fallbacks, how to pick a focus point, Lisbon's size and first compile, the RAW formats whose EXIF is not read), the new errors and the processing order.
- **Dependency.** `kamadak-exif` 0.6.1 (BSD-2-Clause; it pulls `mutate_once`).
- **Checks.** `TZ=UTC cargo test --workspace` 2641 passed after the review fixes, clippy `-D warnings` and fmt clean, `pnpm typecheck` clean (no frontend change). Without `TZ=UTC` one unrelated test fails in the evening (`a_rotated_file_is_gzip_compressed_and_carries_the_day_it_covers`, local date against UTC).

### Timings

M4 Pro Mac mini, release CLI, `/usr/bin/time -l`; depth and render from `opai.log`'s `analysis finished` and `enhancement finished`. Render includes the guided upsample, edge grow, layers and encode; the PNG cache write is queued after it.

| Run | Provider | Depth | Render | Total | Peak RSS |
|---|---|---|---|---|---|
| bonsai f/1.4 (4815x3852), first ever | Core ML | 65.4 s (landscape compile) | 0.60 s | 67.1 s | 6.96 GB |
| bonsai f/2.8 | Core ML | served | 0.82 s | 1.38 s | 2.06 GB |
| bonsai f/8 (above its own f/6) | Core ML | served | 0.013 s | 0.43 s | 0.62 GB |
| bonsai f/1.4 again | --- | served | served | 0.50 s | 0.35 GB |
| Nora f/1.4 (3064x4080), first ever | Core ML | 64.5 s (portrait compile) | 0.46 s | 65.3 s | 7.05 GB |
| Tokyo f/1.4 (5562x3708) | Core ML | 1.08 s (compiled) | 0.49 s | 2.11 s | 2.65 GB |
| Tokyo f/2.8 | Core ML | served | 0.44 s | 0.87 s | 1.66 GB |
| no-EXIF bonsai f/2 | Core ML | 1.07 s | 0.79 s | 2.27 s | 3.27 GB |
| bonsai f/1.4 | CPU | 3.56 s | 0.58 s | 4.68 s | 5.28 GB |
| Tokyo f/1.4 | CPU | 3.43 s | 0.49 s | 4.34 s | 4.97 GB |

The CPU rows ran on copies with one byte appended (a new identity, so the cache could not serve the depth). The lens resolved (logged at `info`): bonsai 36 mm f/6 from EXIF, Nora 24 mm f/5.91, Tokyo 181 mm f/8.38, and the no-EXIF copy 28.9 mm (the depth model's estimate) at f/8, `Assumed`. Radii (`RUST_LOG=opai=debug`): bonsai f/1.4 focused at 1.92 m, -30.3 to +5.8 px; Nora at 0.78 m, -29.4 to +6.9 px; Tokyo at 3.01 m, -401 to +157 px (the 6% cap is 401 px: a 181 mm-equivalent lens at f/1.4 is an extreme request); no-EXIF f/2 -13.6 to +2.6 px.

### What the outputs show

100% crops, nothing derived committed. The bonsai at f/1.4 keeps the canopy and pot sharp and blurs the lattice and back wall strongly, close to the prototype's; the canopy's right edge is 2 to 3 px softer than the prototype's because those leaves read 10 to 20 cm off the focus plane in the FP16 stretched graph's depth, which at 36 mm f/1.4 from 1.9 m is a real lens's depth of field (about 10 cm either side). At f/8 the output equals the decoded input but for 434 of 18.5 M pixels off by at most 3/255, which is ImageMagick's JPEG decoder against `rust-sak`'s (the pipeline returns its own decode untouched). Nora's face, glasses and hair stay sharp and the hair against the dark case holds its outline with no halo. **The visible failure is Tokyo's fur hood**: fringe strands that mix fur and background become crisp curls carrying the original, lightly blurred, background between them, and a faint dark haze spreads outward from the hood into the blurred street (fringe pixels classified as far carry their dark colour into a 150 to 400 px disc). That is expected of a layered render with no matting at these radii (the series' risks); f/2.8 shows it a little less, and a wider edge grow trades the haze for a wider sharp fringe. The masking seam (`keep_sharp`) or a matte is the way out; it is not tuned here.

## Risks / Trade-offs

- A RAW's lens EXIF comes through `kamadak-exif`'s TIFF reader; a RAW format that is not TIFF-based (CR3, RAF) yields
  none and falls back. Recorded in the reference.
- An upscale before the blur multiplies the render's cost by the upscale's area.

## Platforms

macOS (Core ML and CPU) checked with the three test photographs and a no-EXIF copy; Windows and Linux untested.
