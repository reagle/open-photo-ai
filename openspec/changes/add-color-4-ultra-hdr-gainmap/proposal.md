> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

Joseph develops RAW files in Lightroom with HDR on and exports Ultra HDR JPEGs: an 8-bit SDR base image plus an ISO
21496-1 / Adobe `hdrgm` gain map stored as an MPF secondary image. Enhancing one in Open Photo AI (denoise, sharpen,
upscale, crop) silently throws the gain map away, so the result is SDR only; since `add-color-2-loss-warnings` the
user is at least told (`HDR gain map dropped: JPEG output is SDR only for now`). This slice makes that warning
disappear for the case he uses: an Ultra HDR JPEG in gives an Ultra HDR JPEG out.

## What Changes

Task estimate: **14**. This is slice 4 of the `add-color` series (see design.md - Slices), reprioritised ahead of the
AVIF/HEIF, depth and working-space slices, which are postponed; JPEG XL is skipped. Only this slice is proposed now.

- **`rust-sak` reads and writes the gain-map container** (its own tasks, released as a fork tag before adoption):
  `Metadata` gains the JPEG's ISO 21496-1 `APP2` payload and the gain-map image (the MPF secondary JPEG, byte for
  byte); `metadata_bytes` reads both, bounded; `encode_*_with` writes the ISO `APP2`, an MPF index with correct sizes
  and offsets, and appends the gain-map JPEG after the primary's `EOI`. No new crate.
- **Loading keeps the gain map.** A JPEG with a readable gain map yields a `Picture` that also carries the decoded
  gain map, its own metadata (the gain-map image's `hdrgm` XMP and ISO 21496-1 metadata, byte for byte) and the
  source's base-to-gain-map scale. A gain map that cannot be read is dropped with a warning; the photograph still
  loads.
- **Every derived picture keeps it, in register with the pixels.** Enhancements and cache hits keep it unchanged (they
  do not move the frame); a GUI crop applies the same flips, turn and rectangle to it, scaled to its resolution.
- **A JPEG export writes it back as Ultra HDR.** The gain map is never enlarged: it stays at its own resolution (an
  upscale leaves it smaller than the base, which ISO 21496-1 allows and viewers resample), and is shrunk only where
  the written image is smaller than it. It is re-encoded as JPEG at the export's quality with a floor of 90, with its
  own metadata, and assembled with the primary: primary XMP keeps `hdrgm:` and, where the source used one, the
  `Container:Directory` with the gain map's new `Item:Length`; the ISO 21496-1 version marker and MPF index are
  written. Exports to any other format drop it as today.
- **The loss warnings follow.** A JPEG export that carries the gain map reports no HDR loss; a gain map that could not
  be read at load, or rewritten at export, reports `HDR gain map dropped: it could not be carried` (new cause on the
  HDR loss, translated in every GUI locale). Other formats still report the SDR-only loss.
- **Never fails an export**: anything wrong with the gain map degrades the result to today's SDR JPEG plus a warning.
- **Documented caveat, not solved**: enhancements that change the base image's brightness or colour (light adjustment,
  colour balance, colourisation) leave the gain map's ratios computed against the old base, so the HDR rendition is
  slightly off. Creating HDR from non-gain-map sources is a later slice.

Crates touched: `opai` (load, `Picture`, the export's gain-map preparation, XMP policy, loss assessment), `gui` (the
crop applies its framing to the gain map - the framing already lives in `gui/src/images/crop.rs`, and the
transformation itself is an `opai` method it calls; the React loss wording gains one cause), `cli` (no code;
`docs/CLI_REFERENCE.md` only). No logic is placed in a front end beyond calling the shared method.

`rust-sak` coverage found: `image` already walks JPEG segments (`blocks/jpeg.rs`), parses an MPF index far enough to
detect a Gain Map entry (`mpf_has_gain_map`, slice 2), splices `APP` segments into an encoded JPEG (`blocks::splice`),
and carries metadata through `Metadata`/`Embedded`/`encode_*_with`. It does not return the secondary image's bytes,
keep the ISO `APP2` payload, or write an MPF index - all container work any JPEG tool needs, so they are added to
`rust-sak`, not `opai`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `image-io`: loading keeps a JPEG's gain map with the picture; a JPEG export writes it back as Ultra HDR, resized to
  the output; a gain map that cannot be carried is dropped with a warning and reported as an HDR loss.
- `gui-crop`: a crop applied to an Ultra HDR photograph frames its gain map the same way.

## Impact

- `../rust-sak/src/image/` (`metadata.rs`, `metadata_bytes.rs`, `blocks/jpeg.rs`, new `blocks/mpf.rs`, `dispatch.rs`,
  README) and fork tag `26.10.6-meta.5` on `26.10.6-meta.4`; the workspace `[patch]` moves to it.
- `crates/opai/src/image/` (new `gain_map.rs`; `picture.rs`, `load.rs`, `descriptive.rs`, `mod.rs` docs),
  `crates/opai/src/export/` (new `gain_map.rs`; `write.rs`, `loss.rs`, `metadata/mod.rs`, `metadata/xmp/mod.rs`,
  `metadata/xmp/edit.rs`), `crates/opai/src/inference/process/tests/`.
- `crates/gui/src/images/crop.rs`; `crates/gui/frontend/` (`formatLoss` and one key in all 14 locale files).
- `docs/CLI_REFERENCE.md` (Output formats, Metadata).
- No new third-party crate. Run cache unchanged: it stores pixels only and the gain map is re-attached from the
  source, whose identity already hashes the whole file.
- Depends on `add-color-2-loss-warnings` and `add-color-3-loss-warnings-gui` (in progress in workspace
  `opai-ws/warnings`); implementation starts when they land.
