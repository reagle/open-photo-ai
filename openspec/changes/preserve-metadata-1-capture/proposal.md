> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

An exported photograph today carries no metadata at all. A JPEG written by Open Photo AI has no EXIF, no IPTC, no XMP
and no ICC profile (verified with `exiftool` on a real export): the capture date and its offset, the camera, lens and
exposure, the GPS position, the keywords, title, caption, rating, creator and copyright the user put on the original
are all gone, and nothing in the file says that Open Photo AI produced it. For a photographer this makes the output a
worse file than the input, whatever the enhancement did to the pixels --- it falls out of date order in every library,
loses its place on the map, and loses its rights notice.

The gap is stated in the code: `crates/opai/src/image/mod.rs` says "No colour management, EXIF orientation or ICC
profiles", and gives as the reason that "neither the reference nor `rust-sak` handles them today". Nothing in this
stack reads metadata on the way in or writes it on the way out. `rust-sak`'s `image` module decodes straight to a
`DynamicImage` and keeps nothing else; its only metadata reader is the ICC `desc` lookup behind `probe`. `Picture`
carries pixels, a path and an identity, so by the time a picture reaches `export::write` the metadata has nowhere to
have been kept.

This series fixes that end to end. This first slice builds the two foundations every later slice needs: the format
plumbing to read metadata blocks out of a file and to splice them into an encoded file, added to `rust-sak` where
every consumer gets it, and a `Picture` that carries what was read from its source through every operation to the
export.

## What Changes

Task estimate for the whole feature: about **70**, so it is divided into six slices (see design.md --- Slices):
capture (this one), EXIF, XMP, TIFF, HEIF/AVIF and RAW. There is no user setting and no flag in any slice: per the
project owner's decision, metadata --- location included --- is always kept, MakerNotes too when that is safe. This
proposal covers the first slice; its estimate is **15**.

**One shared seam with the colour series.** The `add-color-1-icc-profile` change designs the same seams for the ICC
profile. The two series share one design: one `rust-sak` `Metadata` value, one `Embedded` report, one
`encode_file_with`/`encode_writer_with` entry point, one `Picture.metadata` field and one `Picture::derive`.
**Whichever change lands first creates the shared type and seam; the other adds its fields to it.** This change adds
`exif`, `xmp` and `iptc` (and the matching `Embedded` flags); the colour change adds `icc_profile`. Neither renames or
reshapes what the other created. See design.md --- D2, D4, D5.

**In `../rust-sak` (a separate repository), on the existing `image` feature:**

- **Three fields on the shared `Metadata`** (`#[non_exhaustive]`, `Default`): `exif`, `xmp` and `iptc`, each
  `Option<Vec<u8>>` holding the source's block raw and uninterpreted --- the EXIF TIFF stream, the whole XMP packet,
  and the Photoshop image-resource block from APP13 that holds the IPTC-IIM data. Raw blocks rather than parsed
  fields, because which fields to keep, drop or rewrite is a policy of the application, not of a codec layer. If the
  colour change has not landed, this change creates `Metadata` with these three fields and the colour change later
  adds `icc_profile`.
- **Reading them: `metadata_bytes(bytes) -> Metadata`**, for JPEG (APP1 `Exif`, APP1 XMP including extended XMP, APP13
  `Photoshop 3.0`), PNG (`eXIf`, `iTXt` with keyword `XML:com.adobe.xmp`) and WebP (`EXIF` and `XMP ` RIFF chunks).
  Infallible: other formats, and malformed blocks, give `None` for that field. When the colour change's
  `icc_profile_bytes` exists, `metadata_bytes` fills `icc_profile` through it, so a loader makes one call; TIFF,
  AVIF/HEIF and RAW gain EXIF/XMP readers in slices 4 to 6.
- **Writing them through the shared `encode_file_with`/`encode_writer_with`**: after encoding, the blocks present are
  spliced into the encoded bytes for JPEG, PNG and WebP, and `Embedded` gains `exif`, `xmp` and `iptc` flags saying
  which were written. A format that cannot hold a block leaves it out and reports `false`, never failing the encode.
  No block is altered. If the colour change has not landed, this change creates `encode_*_with` and `Embedded` and
  makes `encode_file`/`encode_writer` thin wrappers over them, exactly as that change specifies.
- **One new crate, `img-parts`** (MIT OR Apache-2.0, pure Rust), for the segment- and chunk-level surgery on JPEG, PNG
  and RIFF/WebP. See design.md --- D3 for why it was chosen over `little_exif`, `rexiv2`, `xmp_toolkit` and the
  `image` crate's own `set_exif_metadata`. The colour change's WebP `ICCP` rewrite (its task 1.3) and this change's
  WebP chunk splice are the same RIFF surgery; whichever lands second moves both onto one helper.
- **A release**, version bump and tag, gated on the repository owner's explicit go-ahead.

**In this repository:**

- **The `rust-sak` pin moves** to the new tag in `crates/opai/Cargo.toml` and, in lockstep, `crates/perf/Cargo.toml`.
- **The shared `Picture.metadata: Arc<Metadata>` carries the new fields.** `image::load` fills them from the same
  bytes it already holds for the identity and the decode --- no second read of the file. If the colour change has not
  landed, this change adds the field, `metadata()`, `with_metadata` and `Picture::derive` as that change specifies.
- **Every picture computed from another keeps that metadata**, through `Picture::derive`: the enhancement chain (fresh
  and cached results), the crop, the colorization preview, the Osaka resample and the live-support crop.
- **Nothing EXIF, XMP or IPTC is written yet.** Slices 2 (EXIF, IPTC) and 3 (XMP) own the rewrite rules (Software,
  Orientation, dimensions, thumbnail, MakerNote safety, XMP history) that must run before any of it is written; copied
  raw, it would put false facts into files. So until slices 2 and 3, `export::write` and the GUI's preview renditions
  hand the encoder a copy of the picture's metadata with `exif`, `xmp` and `iptc` cleared (keeping `icc_profile`, the
  colour change's). Renditions never receive EXIF in any slice: a browser applies an EXIF Orientation to an `<img>`,
  which would turn the canvas away from the frame the crop is measured in.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `image-io`: a loaded picture carries the EXIF, XMP and IPTC its source file held, a picture computed from another
  carries the metadata of the one it was computed from, and neither an export nor a preview writes those blocks yet.

## Impact

**Crates touched:** `opai` (manifest, `image/load.rs`, `image/picture.rs`, `image/mod.rs` docs, `export/write.rs`'s
metadata mask, and the sites that build a derived `Picture`), `perf` (manifest only, the lockstep pin), and `gui` at
two call sites: the crop in `images/crop.rs` moves to `Picture::derive`, and the rendition in `images/serve.rs` masks
EXIF/XMP/IPTC out of what it encodes. Both are constructor or argument changes, not logic in a front end; the mask
itself is an `opai` function both call. `cli` is untouched. Both front ends export through `opai::export::write`,
which is where slices 2 and 3 will consume the metadata, so neither front end has metadata logic now or later --- the
test "both front ends need it, so it belongs in `opai`" is met by construction.

**`rust-sak`:** checked first. (Shared with `add-color-1-icc-profile`, which adds the ICC side of the same seam.) The
`image` module has encoders and decoders for all eight formats, a minimal ISOBMFF box walker (`container.rs`, used for
the AVIF/HEIF `colr` box) and an ICC `desc` reader (`icc.rs`), and **nothing that reads or writes EXIF, IPTC or XMP**.
The gap is general-purpose --- any program that re-encodes a photograph wants to keep its metadata --- so it is filled
by extending `rust-sak`'s `image` module rather than locally, per the project's rule. The policy of what to keep and
rewrite stays in `opai` (slices 2 and 3), apart from the general offset-preserving EXIF patcher slice 2 adds to
`rust-sak`.

**Third-party crates introduced:** `img-parts` in `rust-sak`. None in this repository.

**Boundary with the colour work:** the ICC profile and colour management belong to the separate `add-color-` change.
The ICC profile travels in the same shared `Metadata` value but in the colour change's own `icc_profile` field, which
this change neither reads nor writes on its own; the EXIF `ColorSpace` tag and the XMP `photoshop:ICCProfile` property
are left for slices 2 and 3 to coordinate with that change (design.md --- D8).

**Not in scope:** writing EXIF and IPTC (slice 2) and XMP (slice 3), TIFF (slice 4), AVIF/HEIF (slice 5), RAW sources
and XMP sidecars (slice 6), and applying EXIF orientation on decode, which is a pre-existing display gap noted in
design.md --- D7.
