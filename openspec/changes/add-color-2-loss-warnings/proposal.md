> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

An export can be worse than its source in ways the user cannot see until it is too late. An Ultra HDR / ISO 21496-1
JPEG from a phone (a Display P3 base image plus a gain map in an MPF secondary image) is written as an SDR JPEG and
its 234 KB gain map silently dropped; a 16-bit TIFF or PNG written as JPEG or WebP comes back at 8 bits; a Display P3
photograph written as AVIF, HEIF, BMP or GIF loses its profile and is read as sRGB. Slice 1 logs the profile case, but
nobody reads the log. Joseph's requirement: "if HDR, bit depth, color profile is changed, the user should be warned."

## What Changes

Task estimate: **10**. The feature needs about 18 tasks across `rust-sak`, `opai`, the CLI and the GUI, so it is two
slices of the `add-color` series: this one (`rust-sak`, `opai` and the CLI) and `add-color-3-loss-warnings-gui` (the
GUI's dialog notice and per-image indicator). The pending colour slices are renumbered 4-8; see design.md - Slices.

- **Loading records what the source holds that an export can lose** (`Fidelity`): its true bits per channel (10 and 12
  for AVIF/HEIF, not the 16 they decode to), whether its samples are floating point, whether it is HDR, and the
  description of a profile the load had to discard because it did not describe the decoded pixels. It is carried by
  every derived picture, as the metadata is.
- **HDR is detected without decoding** by a new `rust-sak` function, `hdr_bytes`: a JPEG gain map (an ISO 21496-1
  `APP2` marker, an MPF entry of type Gain Map, or `hdrgm:` XMP), and a PQ or HLG transfer declared by CICP (AVIF/HEIF
  `nclx`, PNG `cICP`) or by an ICC profile's `cicp` tag.
- **Every export assesses its losses** against the source, in `opai`, before it writes: HDR written as SDR (always,
  for now: no format is written as HDR yet), fewer bits per channel or float to integer, and a profile not embedded
  (format cannot hold one, colour model mismatch, or discarded at load). `export::write` answers them on `Written` and
  logs each as a warning.
- **The CLI shows each loss**, not behind `--verbose`: one `⚠️ <path>: <loss>; <loss>` line per affected image, after
  its row off a terminal and listed after the batch's view on one, and for several images a count line ("2 of 5 images
  lost something: 2 HDR, 1 bit depth"). The exit code is unchanged.
- **Wording** is specific: `HDR gain map dropped: JPEG output is SDR only for now`, `16-bit → 8-bit in JPEG`,
  `Display P3 profile not embedded in AVIF`.

Crates touched: `opai` (fidelity at load, loss assessment, `export::write`), `cli` (terminal lines only), `gui` (one
destructuring of `Written` gains `..`; its own warnings are slice 3). No logic is placed in a front end: both read the
same `Loss` list from `opai`.

`rust-sak` coverage found: `probe_bytes` already reports the true bit depth and names an `nclx` description
(`Rec. 2100 PQ`), and `container.rs` already walks AVIF/HEIF to the primary item's `colr`; nothing detects a gain map,
reads PNG `cICP` or an ICC `cicp` tag, or answers "is this HDR" as a value. That is general-purpose (any image tool
converting files wants it), so it is added to `rust-sak`'s `image` module, not to `opai`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `image-io`: loading records the source's depth, float samples, HDR and discarded profile; writing an export reports
  what it lost against them.
- `cli-enhance`: each image whose result lost HDR, bit depth or its colour profile is named with the loss in the
  terminal, and a batch counts them.

## Impact

- `../rust-sak/src/image/` (new `hdr.rs`, `hdr_bytes`, `Hdr`; README and changelog) and a new fork tag
  `26.10.6-meta.4`; the workspace `[patch]` moves to it.
- `crates/opai/src/image/` (new `fidelity.rs`, `picture.rs`, `load.rs`, `profile.rs`, `mod.rs` docs),
  `crates/opai/src/export/` (new `loss.rs`, `write.rs`, `mod.rs`).
- `crates/cli/src/` (`run.rs`, `output.rs`, `look.rs`), `docs/CLI_REFERENCE.md`.
- `crates/gui/src/export/run.rs` (`Written { .. }` only).
- No new third-party crate. No run-cache change: `Fidelity` is re-attached from the source like the metadata.
- Expected merge conflicts with the separate CLI stack (`cli-output-default`, `cli-report-enhancements`) in
  `cli/src/output.rs` (`write`'s return type) and `cli/src/run.rs` (the per-image match and the summary); both are
  mechanical.
