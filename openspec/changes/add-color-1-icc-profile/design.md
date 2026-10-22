## Context

See proposal.md - Why. The pixel path today:

- **Decode.** `opai::image::load_blocking` reads the file once, hashes it (the identity), and decodes through
  `rust_sak::image::decode_bytes` (or `decode_raw_bytes` for RAW). The result is a bare `DynamicImage`: 8-bit RGB(A)
  for most files, 16-bit for 16-bit PNG/TIFF, 10/12-bit AVIF/HEIF widened to 16, and `Rgb16` sRGB for every developed
  RAW (zenraw applies the camera matrix, tone curve and sRGB gamma). Whatever ICC profile the file carried is
  discarded; `rust-sak` only reads its *description* for `probe` (`ImageInfo::color_profile`, `icc.rs`,
  `container.rs`).
- **Model.** `imaging::tensor::Sampler` reads 8- or 16-bit pixels as 16-bit and normalises to `[0,1]` or `[-1,1]`
  floats; models were trained on sRGB 8-bit photographs and are fed whatever encoded values the file held.
  `inference::depth::OutputDepth` (`Eight`/`Sixteen`/`Source`) quantises the float output back to 8 or 16 bits; GUI
  export and the CLI already pass `Source`, the GUI canvas `Eight`. No float result is produced.
- **Encode.** `opai::image::save`/`encode` call `rust_sak::image::encode_file`/`encode_writer`, which narrows depth
  only where the codec forces it (PNG/TIFF/AVIF/HEIF keep 16; BMP/GIF/JPEG/WebP are 8; float TIFF is accepted).
  Nothing is embedded.
- **Preview.** `crates/gui/src/images/serve.rs` re-encodes every rendition as PNG or JPEG q90 and the React frontend
  shows it in `<img>` elements, so the webview's own image colour management applies to whatever profile the rendition
  carries - today none.

The upstream `image` crate (0.25.10 in `Cargo.lock`) has `ImageDecoder::icc_profile` for JPEG, PNG, TIFF, WebP and
GIF, and `ImageEncoder::set_icc_profile`, implemented for WebP since 0.25.4, JPEG and PNG since 0.25.6, and for TIFF
(the encoder writes `Tag::IccProfile`). `rust-sak` encodes WebP through its own `webp-rs` crate, not `image`'s
encoder, and AVIF/HEIF through `avif-rs`/`heif-rs`.

## Goals / Non-Goals

**Goals:**

- The exact ICC bytes a source file carried reach every file and preview the app writes from it, in every format that
  can hold them.
- One place decides what a derived picture inherits, so a future derivation cannot forget the profile.
- The `rust-sak` encoder value that carries the profile is the one the `preserve-metadata` series extends with
  EXIF/XMP.

**Non-Goals:**

- Converting pixel values between colour spaces (slice 6). Pixels are untouched; the profile is a label.
- Coded colour descriptions (CICP/`nclx`, PNG `cICP`), AVIF/HEIF embedding, high-depth export choices, JPEG XL and HDR
  (slices 4-7).
- EXIF orientation, EXIF/IPTC/XMP preservation (the `preserve-metadata` series).
- A profile control in the GUI or a flag in the CLI: carrying the source's profile is the only correct behaviour once
  pixels are unconverted, so there is nothing to choose yet.
- Embedding a profile in BMP (V5 header) or GIF (ICC application extension): both are possible but rare targets, and
  are left as a recorded gap.

## Slices

The `add-color` series, in order. Each is its own change. Slices 2 and 3 (loss warnings) were inserted after this
slice was written, so the slices this document calls 4-8 were first numbered 2-6.

| # | Change | Delivers | Depends on | Status |
|---|---|---|---|---|
| 1 | `add-color-1-icc-profile` | Source ICC profile read at load, carried on `Picture` through crop, enhancement and the run cache, embedded on export (JPEG, PNG, TIFF, WebP) and in GUI previews; `rust-sak` ICC read/embed | - | done (this slice) |
| 2 | `add-color-2-loss-warnings` | `rust-sak` `hdr_bytes`; source fidelity (depth, float, HDR, discarded profile) read at load and carried on `Picture`; the per-export loss assessment in `opai`; CLI warning lines and batch count | 1 | in progress |
| 3 | `add-color-3-loss-warnings-gui` | GUI: the export dialog's notice before export and a per-image warning after it, from slice 2's assessment; i18n in every locale | 2 | in progress |
| 4 | `add-color-4-ultra-hdr-gainmap` | Ultra HDR MVP: a gain-map JPEG's gain map read at load, carried through crop, enhancement and the run cache, kept at its own resolution (never enlarged) and re-assembled as Ultra HDR JPEG on JPEG export; `rust-sak` MPF/ISO read and write | 2, 3 | current (proposed) |
| 5 | `add-color-5-avif-heif` | ICC and CICP (`nclx`) colour descriptions embedded in AVIF and HEIF, CICP read at load (AVIF/HEIF `nclx`, PNG `cICP`), 10/12-bit AVIF and HEIF encode; extends `avif-rs`/`heif-rs` and `rust-sak` | 1 | postponed |
| 6 | `add-color-6-output-depth` | A chosen export depth (Source / 8 / 16 / 32-bit float) in `opai`, the GUI export dialog (all locales) and a CLI `--depth` flag; a float path out of `imaging`; float TIFF; optional OpenEXR as a new `rust-sak` format | 1 | postponed |
| 7 | `add-color-7-working-space` | Colour conversion with `moxcms`: source profile to the models' sRGB working space before inference and back afterwards (or to a chosen output profile), with gamut handling; a "convert to sRGB on export" option; RAW developed to a wide-gamut space | 1, 6 | postponed |
| 8 | `add-color-8-jpeg-xl` | JPEG XL decode and encode (8/16/float, ICC and CICP, PQ/HLG tagging) as a new `rust-sak` format and an export format in GUI and CLI | 5, 6 | skipped |
| 9 | `add-color-9-hdr-create` | HDR output from sources without a gain map (RAW developed scene-referred, PQ/HLG AVIF/HEIF): a computed gain map or PQ/HLG output, and the gain map recomputed after brightness-changing enhancements | 4, 6, 7 | pending |

Slices 5 and 6 may be split further when they are proposed; each must fit 15 tasks.

## Difficulty and likely paths

**Slice 1 - ICC passthrough. Easy; about 2-3 days.** Everything needed exists in the `image` crate; the work is
plumbing through `rust-sak` and `Picture`. Risks: a derivation site that builds its result with `Picture::new` and
silently drops the profile (mitigated by D2); JPEG profiles over 64 KiB need multi-segment `APP2`, which `image` must
split correctly (covered by a test with a large profile); the Linux webview (WebKitGTK) does not colour-manage images
on every distribution, so the preview fix may be macOS/Windows only. Alternative considered: convert to sRGB on load
with `moxcms` - rejected for this slice because it clips P3 colours and changes pixels, which is slice 6's decision to
make deliberately. Upstream acceptability: high - it is a bug fix with no new dependency, and the `rust-sak` additions
are general-purpose.

**Slice 4 - AVIF/HEIF colour and depth. Moderate; about 1 week.** Needs the encoders in `avif-rs` (libavif:
`avifImageSetProfileICC`, `colorPrimaries`/`transferCharacteristics`/`matrixCoefficients`, `depth` 10/12) and
`heif-rs` (libheif: `heif_image_set_raw_color_profile`/`set_nclx_color_profile`) to expose them, then `rust-sak` to
pass them through. Risks: the bundled x265 in `heif-rs` is an 8-bit build, so 10-bit HEIF needs different prebuilt
binaries for three platforms; SVT-AV1 10-bit is supported. These are the same author's crates, so the change is
upstream-friendly but is a release of two more crates. Alternative: rewrite the ISO-BMFF `colr` box after encoding
(pure Rust in `rust-sak`'s `container.rs`) - viable for ICC, not for bit depth.

**Slice 5 - Output depth. Moderate; about 1 week.** The pipeline is already 16-bit end to end; what is missing is a
user-facing choice and a float result. `ChannelDepth` gains a float member, `imaging` stops quantising for it,
`OutputDepth` gains `Float`, and the run identity folds it in. Float TIFF already encodes (`Rgb32F`); OpenEXR comes
from the `image` crate's `exr` feature (pure Rust `exr` crate, BSD-3) added to `rust-sak` as a ninth encode format.
GUI: a depth select in the export dialog limited to what the chosen format can hold, with i18n keys in all 14 locales;
CLI: `--depth source|8|16|32f`. Risks: float output from 8-bit-trained models is not HDR - it is the same image
without quantisation, which users may misread; memory doubles over 16-bit for large upscales. Upstream acceptability:
likely, since the reference had no depth concept and this is opt-in.

**Slice 6 - Working-space conversion. Hard; about 2 weeks plus evaluation.** Two defensible strategies: (a) *tag only*
(slice 1's behaviour): run models on the source's encoded values and re-tag; cheap, no clipping, but a P3 photograph
is processed as if sRGB, which mostly matters for colour balance, colourisation and light adjustment; (b)
*convert-infer-convert*: source → sRGB (or linear sRGB, for models trained so) in float with no clipping, infer, then
back to the source profile; preserves meaning but out-of-gamut values must survive the models' `[0,1]` range (clip, or
compress with a perceptual intent). Recommended: (b) with an unclipped float transform and per-family opt-out, chosen
by an A/B evaluation on P3 and Adobe RGB samples. CMS crate: **`moxcms`** (pure Rust, BSD-3/Apache-2.0, already in the
dependency tree through `image` 0.25.10, ICC v2/v4, CICP, 8/16/float transforms) over **`lcms2`** (the reference CMS,
MIT, but `lcms2-sys` compiles C on all three platforms) and **`qcms`** (Mozilla's, narrow: matrix/TRC profiles, 8-bit
focus). The colour transform belongs in `imaging` or `opai` (it is pipeline work, not codec work); a `rust-sak` helper
for "decode to sRGB" may be worth adding if a second project wants it. RAW: ask zenraw (through `rust-sak`
`image-raw`) for a wide-gamut output space instead of sRGB - a `rust-sak` extension. Upstream acceptability: moderate;
it changes model inputs and needs evidence.

**Slice 7 - JPEG XL. Moderate; about 1 week.** Decode: **`jxl-oxide`** (pure Rust, MIT/Apache-2.0, already in
`Cargo.lock` via `rawler`) handles ICC, CICP, HDR and float. Encode options: **`jxl-encoder`** (imazen, pure Rust,
first released 2026-02, 0.3.1 in May 2026; lossless and VarDCT; 16-bit and linear float input; licence **AGPL-3.0 or
commercial**) or **`jpegxl-rs`** (binding to libjxl, the reference encoder; crate licence GPL-3.0-or-later, libjxl
BSD-3; needs CMake and a C++ toolchain on all three platforms or a prebuilt static libjxl, in the same style as
`avif-rs`). Both licences are compatible with this app's AGPL-3.0, but not with `rust-sak`'s Apache-2.0 default build:
the encoder must sit behind its own `rust-sak` feature with a licence note, as `image-raw` does for zenraw.
Recommended: `jpegxl-rs` with prebuilt static libjxl if ICC/CICP/HDR tagging is required now (libjxl is complete);
`jxl-encoder` if a pure-Rust build matters more, after checking its ICC and colour-encoding support. Upstream
acceptability: moderate (new format, new build dependency).

**Slice 8 - Ultra HDR. Hard; 2-3 weeks.** Two distinct features: (a) *preserve* an input gain-map JPEG (Android Ultra
HDR, Adobe/Apple gain maps): today the gain map is silently flattened, the same failure Pillow has
(python-pillow/Pillow#8036, worked around in `~/dev/sigal-ultrahdr-gainmap` by splitting base and gain map, resizing
each, and re-merging with `ultrahdr_app`); here the gain map would be cropped and resampled alongside the enhanced
base and re-assembled. (b) *create* HDR output, which needs an HDR source (RAW developed scene-referred, or a PQ/HLG
AVIF/HEIF/JXL); 8-bit-trained models do not create highlight range. Crates: **`ultrahdr-rs`/`ultrahdr-core`** (imazen,
pure Rust, Apache-2.0, 0.3.5 April 2026; XMP `hdrgm`, ISO 21496-1, MPF; uses `moxcms`; README warns that not all code
is human-reviewed), **`ultrahdr`** (bindings to Google's libultrahdr, C++, Apache-2.0, reference behaviour but another
C++ build), or hand assembly in `rust-sak` (base JPEG + gain-map JPEG + MPF + XMP; `ultrahdr-core` alone supplies the
maths and metadata without a codec). Recommended: `ultrahdr-core` for metadata and maths with `rust-sak`'s own JPEG
encoder, behind a `rust-sak` feature. Upstream acceptability: lower; it is a niche format with ongoing standard churn.

## How the work divides

- **`rust-sak`** (`image` feature): ICC bytes read without a pixel decode; an encode-time metadata value; embedding
  for JPEG, PNG, TIFF and WebP. General-purpose - any consumer that re-encodes photographs wants it.
- **`opai`**: `Picture` carries the metadata; `load` fills it; the run result and every derivation inherit it;
  `save`/`encode` and `export::write` pass it to `rust-sak` and log a warning when the target dropped it.
- **`gui`**: `crop.rs` derives through the new `Picture` method; `serve.rs` passes the picture's metadata to
  `encode_blocking`. No logic is added to the front end, and no frontend (React) change is needed.
- **`cli`**: no code change - `export::write` carries the profile; `docs/CLI_REFERENCE.md` says so.

## `rust-sak` coverage

Uses the `image` feature (and `image-raw`, unchanged). What exists: `ImageInfo::color_profile` (the profile's *name*)
for every format, from `icc::description` over the bytes `image`'s decoders and `container.rs` already locate. What is
missing, and filled by **extending `rust-sak`** because both are codec work any consumer needs:

1. `icc_profile_bytes(bytes) -> Result<Option<Vec<u8>>>` (and a `_file` form): the embedded profile's exact bytes,
   header-only, for all eight formats. Native formats use `ImageDecoder::icc_profile`; WebP/AVIF/HEIF reuse
   `container.rs`'s walk, refactored to return the bytes and have the description derived from them, so the two cannot
   disagree. An `nclx`-only AVIF/HEIF returns `None` (slice 4 adds CICP).
2. `Metadata` (`#[non_exhaustive]`, `Default`, with `icc_profile: Option<Vec<u8>>`) and
   `encode_file_with`/`encode_writer_with(image, sink, format, options, &Metadata) -> Result<Embedded>`, where
   `Embedded` reports what was actually written (`icc_profile: bool`). The existing `encode_file`/`encode_writer`
   become wrappers passing `Metadata::default()`, so the change is additive and semver-minor. JPEG and PNG call
   `set_icc_profile` on their encoders; TIFF moves from `DynamicImage::write_to` to `TiffEncoder` with
   `set_icc_profile` (still into the seekable buffer); WebP inserts a `VP8X` header (when the encoder wrote a simple
   `VP8 `/`VP8L` file) and an `ICCP` chunk ahead of the image data, fixing the RIFF size - libwebp's mux API is not
   exposed by `webp-rs`, and the RIFF rewrite is a few dozen lines with no new dependency. BMP, GIF, AVIF and HEIF
   report `icc_profile: false`.

The `preserve-metadata` series adds `exif`/`xmp` fields to the same `Metadata` and `Embedded`; whichever change lands
first creates them. Pushing to `rust-sak` is a release and needs the user's go-ahead (tasks 1.4).

## Decisions

### D1. Carry exact bytes, not a parsed profile

`Picture` holds `Arc<Metadata>` (re-exported from `rust-sak`) whose `icc_profile` is the file's bytes verbatim.
Alternatives: a parsed `moxcms::ColorProfile` (adds a dependency now for no use until slice 6, and re-serialising can
alter a profile), or a name string (cannot be embedded). Bytes are lossless and cheap (a P3 profile is ~0.5 KiB; large
camera profiles reach a few hundred KiB, shared by `Arc`).

### D2. One derivation constructor

`Picture::derive(&self, pixels, identity) -> Picture` keeps the source's path and metadata. `process/mod.rs` (fresh
and cached results alike - the cache stores pixels only and the result is rebuilt from the source there) and
`gui/src/images/crop.rs` switch to it. `Picture::new` keeps its signature and yields no metadata;
`Picture::with_metadata` is used by `load`. Alternative: add a metadata parameter to `Picture::new` - rejected because
every test call site would need it and a production site could pass `Default` by habit.

### D3. Identity and cache unchanged

A loaded identity is the XXH3 of the whole file, so it already covers the profile; derived identities are composed
from it. The cache needs no new key part and no format change.

### D4. A dropped profile is a warning, not an error

Writing a P3 photograph as AVIF in this slice still succeeds; `opai::image::save` logs
`warn!(format, profile, "colour profile not embedded")` with the profile's description. Refusing would break existing
exports; silently dropping is the bug this series fixes. Slice 4 removes the AVIF/HEIF cases. No GUI or CLI surface is
added for it in this slice (the open question below decides whether one is wanted).

### D5. Previews embed the profile too

`serve.rs` passes the picture's metadata to `encode_blocking` for both PNG and JPEG renditions, so the canvas, the
before/after compare and the thumbnails agree with the exported file. The webview performs the conversion to the
display; no colour code enters the GUI.

## Risks / Trade-offs

- [A derivation site bypasses `derive`] → the end-to-end test (task 3.3) runs a P3 fixture through crop and an
  enhancement and asserts the written file's profile bytes; `grep Picture::new` outside tests is part of review.
- [Models see P3 values as if sRGB] → unchanged from today (pixels were already unconverted); slice 6 addresses it.
  Colour-sensitive families (colour balance, colourisation) may now look *more* saturated than before because the
  result is finally displayed in P3 - this is correct.
- [JPEG profiles above 64 KiB] → test with a large profile; `image` splits `APP2` into numbered chunks.
- [WebKitGTK does not colour-manage `<img>`] → previews on Linux may still look shifted while exports are right;
  recorded as untested.
- [`webp-rs` writes a `VP8X` itself when alpha is present] → the RIFF rewrite sets the ICC flag on an existing `VP8X`
  rather than adding a second one; tested with and without alpha.
- [Profile describes a different colour model (CMYK/Gray JPEG)] → `image` converts CMYK to RGB on decode, so a CMYK
  profile no longer describes the pixels; such a profile is dropped at load (header colour space must be `RGB ` or
  `GRAY` matching the pixels) and logged.

## Platforms

Pure Rust changes in `rust-sak` and `opai`; no execution-provider or model-session impact. Validated against: macOS
(planning only - nothing run). Untested: Windows and Linux; WebView2 is expected to colour-manage images, WebKitGTK
may not.

## Migration Plan

Additive in `rust-sak` (minor release); internal signature change in `opai::image::save`/`encode`, all callers
in-tree. Rollback: revert the pin and the `opai` changes; files written meanwhile simply carry a correct profile.

## Implementation notes and deviations

Recorded at apply time (2026-10-22).

- **`rust-sak` release.** Built on upstream tag `26.10.6` (the tag this project pinned), as one commit,
  `feat(image): Read and embed ICC colour profiles, with Metadata and encode_*_with` (`217bd352`), on bookmark
  `icc-profile` of the fork `reagle/rust-sak`, tagged there `26.10.6-color.1`. The crate version stays `26.10.6`; the
  tag is a fork tag, not an upstream release. `rust-sak` has no changelog file, so the change is recorded in
  `src/image/README.md` ("Added after 26.10.6") and in the annotated tag's message.
- **Adoption.** Not a pin bump: one workspace-level `[patch."https://github.com/vegidio/rust-sak"]` entry redirects
  every crate's `tag = "26.10.6"` pin to the fork tag, and `Cargo.lock` holds one `rust-sak`. When upstream releases
  the change, delete the `[patch]` section and move the four pins.
- **One more `rust-sak` function**, `icc_profile_description(&[u8]) -> Option<String>`, so `opai` can name the profile
  in its warnings (D4) without a second ICC parser.
- **TIFF reading fix in `rust-sak`.** `image`'s `ImageReader` gives the `tiff` crate a decoding buffer the size of the
  image's pixels, which the `tiff` crate also applies to tag values, so the ICC tag of a small TIFF (and of any TIFF
  whose profile exceeds about 1/24 of its pixel data) read as absent — for `probe` too. TIFF headers are now read
  through `TiffDecoder` directly, keeping the `tiff` crate's own 256 MiB bound.
- **Bounds on untrusted profiles.** A profile is reported only if it has the 128-byte header with `acsp` and is at
  most 16 MiB; JPEG embedding skips profiles over its 255 × 65,519-byte limit (reported `false`, encode succeeds).
- **Colour-model check at encode as well as load.** `opai::image::save`/`encode` leave out (with the same warning) a
  profile whose colour space does not match the pixels being written, e.g. a `GRAY` profile after colourisation made
  the pixels RGB.
- **The check uses the colour model the file will hold, not the pixels'.** `rust-sak` writes grayscale as colour in
  TIFF with alpha (expanded to RGBA), BMP, GIF, WebP (libwebp is always YUV), AVIF and HEIF; only JPEG, PNG and
  alpha-less TIFF keep it grayscale. `profile::written_in_colour(format, pixels)` restates that (rust-sak's
  `make_compatible` is private), and a test encodes every grayscale and colour layout through `rust-sak` in all eight
  formats and fails if the two drift. Found in review: a grey-plus-alpha PNG with a `GRAY` profile had produced an RGB
  TIFF and a WebP tagged `GRAY`.
- **Known limitation: TIFF profiles above about 8 MiB read as absent.** `tiff` 0.11.3 decodes a BYTE tag into a
  `Vec<Value>` (32 bytes per byte of profile) under its 256 MiB `decoding_buffer_size`, so the effective bound is 256
  MiB / 32 = 8 MiB, not the 16 MiB `rust-sak` allows; larger profiles are silently dropped on read. The comment on
  `native_decoder` in `rust-sak`'s `dispatch.rs` ("256 MiB per value") is therefore wrong. Not changed here, since the
  `26.10.6-color.1` tag is pushed and other work is stacked on it; fix both in the next `rust-sak` change (read tag
  34675 as raw bytes, or raise the limit for that tag only).
- **API shape.** `save`/`encode` take the metadata as their second argument (`&Metadata` blocking, `Arc<Metadata>`
  async); `Picture` gained `shared_metadata()` for the async forms.
- **Fixture location.** The CC0 profile is `fixtures/DisplayP3-v4.icc` at the workspace root (beside
  `fixtures/test.dat`) so `opai` and `gui` tests share it; the P3 JPEG/PNG/TIFF/WebP fixtures are generated from it.
  Its own description is `sP3`.
- **End-to-end test.** `opai` cannot call the GUI crop, so `inference/process/tests/colour.rs` crops with
  `Picture::derive` over `crop_imm`; the GUI crop's own test covers `crop.rs`.
- **Derived-picture sites.** Beyond `process/mod.rs` and `crop.rs`, `perf/src/sweep.rs` (production) also moved to
  `derive`. The other `Picture::new` sites the `preserve-metadata` series lists (`execute.rs`, `models/*/live.rs`,
  `live_support.rs`) are all under `cfg(test)`, so `grep Picture::new` outside tests finds only `load.rs`.
- **Reads tested per format.** BMP and GIF are tested without a profile only: no encoder available writes one into
  them.
- **Not done in 4.2.** The automated checks pass; the manual export was done with the CLI (Display P3 kept byte for
  byte in JPEG/PNG/TIFF/WebP, AVIF warned), not the GUI, and the preview's on-screen colours were not inspected.

## Open Questions

- Should the GUI export dialog show a note when the chosen format cannot carry the source's profile (BMP/GIF, and
  AVIF/HEIF until slice 4)? Deferrable to slice 4, where the set of affected formats shrinks to BMP/GIF.

## References

- `image` crate changelog (ICC writing per encoder, CICP via moxcms):
  https://github.com/image-rs/image/blob/main/CHANGES.md
- `ImageEncoder::set_icc_profile`: https://docs.rs/image/0.25.10/image/trait.ImageEncoder.html ; TIFF encoder source:
  https://docs.rs/image/0.25.10/src/image/codecs/tiff.rs.html
- moxcms: https://github.com/awxkee/moxcms , https://docs.rs/crate/moxcms/latest
- jxl-encoder: https://docs.rs/crate/jxl-encoder/latest ; jpegxl-rs: https://docs.rs/jpegxl-rs/ ; jxl-rs decoder:
  https://docs.rs/crate/jxl/0.3.0
- ultrahdr-rs: https://docs.rs/crate/ultrahdr-rs/latest ; libultrahdr bindings: https://docs.rs/ultrahdr ; Ultra HDR
  v1.1: https://developer.android.com/media/platform/hdr-image-format
- heif-rs (8-bit x265 note): https://docs.rs/heif-rs/latest/heif/
- Pillow gain-map loss: https://github.com/python-pillow/Pillow/issues/8036
