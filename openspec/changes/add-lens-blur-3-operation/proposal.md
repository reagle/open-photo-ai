> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I tested its application in practice.

## Why

Slice 1 gives every photograph a metric depth map and slice 2 a render that follows a radius per pixel. This slice joins
them into an enhancement a user can run: Lens Blur, set by a focus point and an f-number, following the depth of field a
real lens would have on the photograph's own camera. It is the first slice a user sees, through the CLI.

## What Changes

Task estimate: **11**. Slice 3 of the `add-lens-blur` series (see `add-lens-blur-1-depth/design.md` - Slices, D6 to D8
and D12 to D14).

- **The photograph's lens, from EXIF** (`opai::image::read_lens`, new dependency `kamadak-exif`): focal length, 35 mm-equivalent focal length, f-number, and the sensor's diagonal from the pixel dimensions and both focal-plane resolutions (checked against the file's own size), resolved to a 35 mm-equivalent focal length and f-number, with the fallbacks of D7 (the depth model's focal estimate; an assumed f/8 equivalent). **A crop is a smaller sensor behind the same lens**: the file's header gives the framing (file diagonal over the picture's), and the EXIF's focal length and f-number are scaled by it.
- **A Lens Blur family (`lb`), applied last**, after Upscale, with one variant, Lisbon, whose artifacts are the depth
  model's. Its parameters are the equivalent f-number to simulate (1.0 to 32, default 2.8) and the focus point
  (fractions of the frame, default the centre). Autopilot never suggests it.
- **The blur**: the depth at the focus point; the thin-lens circle of confusion for every pixel, in output pixels; only
  the blur the photograph does not already have (none at or above its own f-number); the depth guided-upsampled to the
  output and grown at edges; the layered render of slice 2; the result in the input's channel depth.
- **Depth and lens reach the operation by injection**, as faces reach face recovery: the chain runs the cached depth
  estimate on its input picture only when it holds a lens blur, reads the lens, and the operation's cache tag folds in
  both, so moving the focus or the aperture re-renders without re-estimating, and editing an earlier enhancement never
  re-estimates either. Where the EXIF alone shows the blur would add nothing, no depth is estimated at all, and a lens blur planned without its injection is refused at plan time (`UnsupportedReason::Uninjected`).
- **CLI**: `--lens-blur [MODEL]`/`--lb`, `--lb-f <F>` and `--lb-focus <X,Y>`, documented in `docs/CLI_REFERENCE.md`. Parameters are quantised to thousandths, telemetry files the blur under `lb_lisbon_fp16`, and a render of a 24 MP photograph takes about 0.5 to 0.8 s once the depth is served (a first Core ML compile is about 65 s per orientation, once).

Crates touched: `opai` (lens reading, the family, the operation and its pipeline, the injection), `cli` (flags), `docs`.
The GUI follows in slice 4; its Rust side needs nothing for the operation itself (the catalogue is generic). The frontend's exhaustive `Family` union is **not** extended here, as first proposed: `"lens_blur"` breaks `EnhancementRow.tsx`'s exhaustive switches and two `Record<Family, ...>` tests, so it comes with the panel in slice 4.

`rust-sak` coverage found: no EXIF field reading (`image` decodes and orients only). `kamadak-exif` (BSD-2-Clause, pure
Rust, reads JPEG, TIFF and TIFF-based RAW, PNG, WebP and HEIF containers) is added to `opai`; the alternative, a reader
added to `rust-sak`, is the maintainer's call (series design - `rust-sak` coverage, Open Questions).

## Capabilities

### New Capabilities

None (`lens-blur` is created by slice 2).

### Modified Capabilities

- `lens-blur`: the operation, its parameters, the lens, its place in the chain and its cache behaviour.
- `cli-enhance`: the lens blur flags.

## Impact

- `opai`: new `image/lens.rs`, `models/lens_blur/{mod.rs, variant.rs, lens.rs, render.rs}`, `inference/depth_map.rs`
  (the injection, beside `inference/faces.rs`); `models/artifact.rs`, `operation.rs`, `catalogue.rs`, `build.rs`,
  `models/f_number.rs`, `models/fraction.rs` (`FNumber`, `Fraction`, on `quantized_param!`), `inference/process/mod.rs`, `lib.rs`, `models/test_support.rs`,
  `autopilot`; `Cargo.toml` (`kamadak-exif`).
- `cli`: `args.rs`, `chain.rs`, `events.rs`, `look.rs`, `autopilot.rs` (excluded family).
- `perf`: its selection never measures a lens blur (`SelectionError::NoModel`). `image`: `read_frame_diagonal` (the framing's probe of the file's header).
- `gui` frontend: none (the `Family` union is slice 4's).
- `docs/CLI_REFERENCE.md`.
