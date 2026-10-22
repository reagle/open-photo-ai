> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

A photograph's embedded ICC profile is dropped on the way through the app. A Display P3 JPEG goes in, the enhanced
file comes out with no profile, and every viewer then reads its (essentially unchanged) pixel values as sRGB, so the
result looks desaturated and colour-shifted next to its source. The preview has the same fault, because the canvas
renditions are re-encoded without the profile. `crates/opai/src/image/mod.rs` records that colour management and ICC
profiles are deliberately absent; this is the first slice of a series that adds them, starting with the cheapest fix
that removes the visible shift: carrying the source's profile through to every file and preview the app writes.

## What Changes

Task estimate: **15** (at the limit). This is slice 1 of the `add-color` series (eight slices, listed in design.md -
Slices); only this slice is proposed now.

- **Loading reads the embedded ICC profile** of every format that carries one (JPEG `APP2`, PNG `iCCP`, TIFF tag
  34675, WebP `ICCP`, AVIF/HEIF `colr` of type `prof`/`rICC`) and keeps its exact bytes with the loaded picture. A
  file with no profile, a coded-only (`nclx`) colour description, or a developed camera RAW (always sRGB) carries
  none.
- **Every picture derived from a loaded one keeps its profile**: an enhancement result (fresh or served from the run
  cache), a crop, and a chain of several enhancements. No pixel values are converted in this slice; the profile is a
  label that travels with the pixels it describes.
- **Writing embeds the profile** in JPEG, PNG, TIFF and WebP, byte for byte. In BMP, GIF, AVIF and HEIF it cannot be
  embedded yet; the file is still written, and a warning naming the format and the profile is logged. AVIF and HEIF
  gain it in slice 4.
- **GUI previews and thumbnails embed the profile** in the renditions they serve, so the webview shows a wide-gamut
  photograph in its own colours on platforms whose webview colour-manages images.
- **GUI export and CLI output both gain it** without any new control or flag: they write through `opai`'s export path,
  which now passes the profile on.
- **`rust-sak`'s `image` module is extended** (its own tasks, released before adoption): reading a file's ICC profile
  bytes without decoding pixels, and encoding with an ICC profile for JPEG, PNG, TIFF and WebP. The encoder side takes
  a metadata value designed so the parallel `preserve-metadata` series can add EXIF and XMP to the same value rather
  than introducing a second one.
- **BREAKING (internal API only)**: `opai::image::save`/`encode` and their `_blocking` forms take the metadata to
  embed; every in-tree caller is updated. No file format, setting or command-line behaviour is removed.

Crates touched: `opai` (load, `Picture`, run result, save/encode, export write), `gui` (crop derivation and preview
renditions only - both already live in `gui` and only need to pass the profile on; no new logic is placed in a front
end), `cli` (no code change; documentation only). `rust-sak`: the `image` feature gains the two pieces above;
`image-raw` is unchanged.

`rust-sak` coverage found: `image` already parses an embedded profile's *description* (`ImageInfo::color_profile`, via
`icc.rs` and `container.rs`), but never returns the profile's bytes and has no way to embed one on encode. Both gaps
are general-purpose and are filled in `rust-sak`, not in `opai`.

Boundary with `preserve-metadata`: that series owns EXIF, IPTC and XMP preservation. The ICC profile, and everything
about colour (profiles, CICP, depth, gamut conversion, HDR), belongs to this series. Both write through the same
metadata value on the `rust-sak` encoder; whichever lands first introduces it, and the other adds its fields.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `image-io`: loading reports a file's embedded colour profile; writing embeds it where the target format can hold
  one, and says when it cannot.
- `gui-export`: an exported photograph carries the colour profile of the photograph it was made from, through
  enhancement and crop.
- `gui-images`: served previews and thumbnails carry the source photograph's colour profile.
- `cli-enhance`: a written result carries the input's colour profile.

## Impact

- `../rust-sak/src/image/` (`dispatch.rs`, `container.rs`, `icc.rs`, `encode_*.rs`, a new metadata type, README,
  CHANGELOG) and a new `rust-sak` release tag; the workspace `Cargo.toml` pin bumped to it.
- `crates/opai/src/image/` (`picture.rs`, `load.rs`, `save.rs`, `encode.rs`, `mod.rs` module docs),
  `crates/opai/src/inference/process/mod.rs`, `crates/opai/src/export/write.rs`.
- `crates/gui/src/images/crop.rs`, `crates/gui/src/images/serve.rs`.
- `docs/CLI_REFERENCE.md` (Output formats).
- No new third-party crate: embedding uses the `image` crate's existing `ImageEncoder::set_icc_profile` (JPEG, PNG,
  TIFF) and a RIFF chunk rewrite inside `rust-sak` for WebP.
- Run cache: unchanged format and keys - the cache stores pixels only, and the profile is re-attached from the source
  on a hit; the source identity already covers the profile's bytes.
