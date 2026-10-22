## Context

Current behaviour, with evidence:

- `crates/opai/src/image/recommended.rs:24-31`: `default_quality` - AVIF/HEIC 60, JPEG 90, WebP 75; `:40-58`
  `recommended_encode_options` clamps a caller's `Option<f64>` into 1..=100 or uses that default.
- `crates/opai/src/export/write.rs:26-35`: `WriteOptions { quality: Option<f64>, .. }`; `:116` turns it into
  `EncodeOptions`; `:157-171` hands the JPEG quality to the gain map, where `export/gain_map.rs:12,62` applies
  `MIN_GAIN_MAP_QUALITY = 90` as a floor.
- GUI: the per-format quality lives in the settings store (`frontend/stores/settings.ts:66-78`, `QualityChoices`) and
  is passed per export (`features/export/ExportSettingsPanel.tsx:99-103`).
- CLI: `crates/cli/src/output.rs:194` always writes `quality: None`, i.e. the format defaults; `-q/--quality`
  (`cli/src/args.rs:49-50`) is the model precision (HD/SD), not encoding.
- JPEG encoding is `image`'s `JpegEncoder::new_with_quality` (`rust-sak` `src/image/dispatch.rs:405`), which scales
  the IJG Annex K tables with libjpeg's formula. So a quality estimated against the IJG tables is on the same scale as
  the number this application writes.
- `rust-sak` `src/image/blocks/jpeg.rs:39` `segments()` already iterates the marker segments before the first SOS -
  the place to read `DQT` (0xFFDB) from.

## Goals / Non-Goals

**Goals:** a mode that picks the JPEG quality from the source; the chosen quality visible; CLI parity; the design of
the perceptual mode settled so slice 2 can start without re-research.

**Non-Goals (this slice):** perceptual search (slice 2); mapping a JPEG quality onto WebP/AVIF/HEIC scales (they are
not comparable - see `default_quality`'s own comment); changing the defaults.

## How the work divides

`rust-sak`: read DQT tables, estimate quality. `opai`: record the estimate when loading, and resolve a `QualityMode`
plus the per-format setting into the number to encode with - the one place both front ends call. `cli`: the flag.
`gui`: pass the mode, persist it, show it and the result.

## `rust-sak` coverage

Uses `image` (already enabled). Gap: no quantisation-table reader. Filled by **extending `rust-sak`** (D1), because it
is a general JPEG property next to code that already parses the same segments, not something specific to this app.

## Decisions

### D1. `rust_sak::image::jpeg_quality(bytes) -> Option<JpegQuality { quality: u8, exact: bool }>`

Collect `DQT` tables (8- and 16-bit precision, several per segment) from `segments()`. For q in 1..=100 build the IJG
luminance and chrominance tables with libjpeg's scaling (`scale = q < 50 ? 5000 / q : 200 - 2q`, entries
`clamp((std * scale + 50) / 100, 1, 255)`); an exact match of both tables answers `exact: true`. Otherwise answer the
q minimising the summed absolute difference, `exact: false` - the same idea as ImageMagick's `JPEGSetImageQuality`
(compares table sums and a hash against the IJG series) and `identify -format %Q`. `None` for a non-JPEG, no DQT, or a
greyscale-only/odd table set it cannot compare. Camera and Photoshop JPEGs use their own tables; they get the nearest
IJG quality, which is what ImageMagick reports for them too.

Alternative: estimate in `opai` from the source path at export. Rejected: re-reads the file (which may have moved),
and duplicates segment parsing `rust-sak` already owns.

### D2. Recorded at load, resolved at export

`opai` records the estimate on the picture's source information when it loads a JPEG (bytes are in hand there);
derived pictures (crop, enhancement) inherit it as they inherit metadata. `export::WriteOptions.quality` becomes
`EncodeQuality { mode: QualityMode, fixed: Option<f64> }` with `QualityMode::{Fixed, MatchSource}` (slice 2 adds
`Perceptual`). Resolution: `MatchSource` + JPEG export + an estimate -> that estimate; anything else -> today's rule.
`Written` gains `quality: Option<u8>` (what was used) and a `quality_source` (`Set`, `Source`, later `Perceptual`),
logged with the export.

### D3. The gain-map floor is unchanged

The gain map is encoded at `max(primary, 90)` as today, whatever chose the primary's quality. A low-quality source
does not drag the gain map down.

### D4. CLI: `--output-quality <1-100|source>`

Long only; `-q` is the precision and `-Q` would be a confusing neighbour. A number is a Fixed quality for whichever
lossy format is written; `source` is Match source with the format default as fallback; slice 2 adds `auto` and
`auto=<target>`. Absent: today's defaults. Whether the GUI's saved mode should follow into the CLI through
`cli-gui-settings` is left to that series (it currently publishes processor, models and Autopilot exclusions only).

### D5. GUI: one mode, on Settings > Export and in the export dialog

A "Quality" select - Fixed / Match source - above the per-format sliders, persisted in the settings store with a
repair (unknown -> Fixed). With Match source, the JPEG slider is labelled "if the source is not a JPEG". Each queue
row shows the quality used once written (for example "JPEG, quality 82 (source)").

### D6. Default: Fixed

Keeps parity with upstream and the reference, costs nothing, and is predictable. Match source is opt-in because an
enhanced image re-encoded at a low source quality reintroduces the blocking the enhancement removed (a denoised or
upscaled 70-quality source written at 70) - which is exactly what slice 2 measures instead of guessing.

## The perceptual mode (slice 2), researched here so the series has one design

- **Method.** Encode at candidate qualities by binary search (about 6-7 encodes over 1..100, fewer over a bounded
  range such as JPEG 60-98) and keep the smallest file whose perceptual score against the uncompressed enhanced image
  meets a target. Prior art: `jpeg-recompress` from `jpeg-archive` (binary search on quality against SSIM, MS-SSIM,
  SmallFry or MPE with presets low/medium/high/veryhigh); Google's Guetzli (butteraugli-guided JPEG; far too slow -
  around a minute per megapixel - and archived) and jpegli in libjxl (butteraugli-tuned JPEG encoder, targets a
  distance; a different encoder, not a search). SSIMULACRA 2 is the metric libjxl and jpegli are now tuned and
  evaluated with; its score reads roughly 90 "visually lossless", 80 "very high", 70 "high", 50 "medium".
- **Metric crate.** `ssimulacra2` (rust-av, BSD-2-Clause, pure Rust) is the recommendation. Alternatives:
  `butteraugli` (imazen, BSD-3-Clause, pure Rust port; slower, a max-error metric better suited to "no visible
  difference anywhere"); `dssim-core` (AGPL-3.0 - compatible with this AGPL-3.0 application, but it could not live in
  Apache-2.0 `rust-sak`, and its SSIM is less aligned with current codec tuning); `image-compare` (MIT; plain SSIM and
  RMS only - weakest). Nothing in `rust-sak` covers a perceptual metric; slice 2 decides with Joseph whether the
  search lives in `rust-sak` behind a new `image-perceptual` feature (general-purpose: "encode to a target") or in
  `opai`.
- **Cost.** A full-image search on a 24-megapixel upscale would be about 7 encodes plus 7 metric passes - seconds for
  JPEG and WebP, tens of seconds to minutes for AVIF at speed 6. Slice 2 therefore searches on a deterministic sample
  of tiles (for example 8 tiles of 256 x 256 chosen for detail), then encodes the full image once at the chosen
  quality; its first task measures this on Apple Silicon and sets the budget (target: under 2 s extra per JPEG/WebP,
  under 10 s per AVIF/HEIC). Tile-based choice transfers well for constant-quality encoders but is not identical to a
  full-image score - stated in the export report.
- **Formats.** JPEG, WebP, AVIF and HEIC, each with its own search range; lossless formats unaffected.
- **Interactions.** Ultra HDR: the search scores the SDR primary only; the gain map keeps the floor of 90. CLI:
  `--output-quality auto[=<target>]`. GUI: the mode select gains Automatic with a target choice (High 70 / Very high
  80 / Visually lossless 90; recommended default target 80).

## Risks / Trade-offs

- [Non-IJG tables give an approximate estimate] -> `exact: false` is recorded and logged; the nearest IJG quality is
  what other tools report.
- [A tiny or pathological DQT] -> `None`, falling back to the set quality.
- [Changing `WriteOptions` touches both front ends] -> one task each, compiled together.
- [Validated against] macOS by reasoning; behaviour is platform-independent; untested on Windows and Linux. Execution
  providers are not involved.

## Slices

1. **`add-export-auto-quality-1-match-source` --- current (this change).** `rust-sak` JPEG quality estimate; `opai`
   records it at load and resolves a quality mode (Fixed / Match source); quality used reported; CLI
   `--output-quality <1-100|source>`; GUI mode select. Depends on the Ultra HDR stack (`uhdr`).
2. **`add-export-auto-quality-2-perceptual` --- pending.** Automatic mode: tile-sampled binary search on SSIMULACRA 2
   for JPEG, WebP, AVIF and HEIC; target presets; CLI `auto[=<target>]`; GUI Automatic option with a target; timing
   measured and budgeted. Depends on slice 1's `QualityMode`, reporting and flag.

## Difficulty and likely paths

Slice 1: moderate (one to two days), most of it plumbing a type through `opai`, CLI and GUI; the estimator itself is
about 100 lines plus fixtures. Slice 2: harder (two to four days) - the search, sampling and measuring cost, and a
crate decision. Likely path: D1-D6 as written; the main risk is the `rust-sak` round trip (tag, adopt).

## Open Questions

- Whether Match source should also bound a Fixed setting ("never above the source quality") - deferrable; it does not
  change this slice's tasks.
