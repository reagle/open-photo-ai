## Context

See proposal.md - Why. State of the code this slice builds on (workspace `opai-ws/warnings`, stacked on
`preserve-metadata-3-xmp`, with `add-color-2`/`-3` in progress; `rust-sak` fork tag `26.10.6-meta.4`):

- **Load.** `opai::image::load_blocking` reads the file once, hashes it, decodes the primary image with
  `rust_sak::image::decode_bytes` (which stops at the primary's `EOI`), and attaches `metadata_bytes` (ICC, EXIF, XMP,
  IPTC, byte for byte) and `Fidelity::read` (`hdr_bytes` says `Hdr::GainMap`). The MPF secondary image is never read.
- **Picture.** `Picture { path, pixels, identity, metadata: Arc<Metadata>, fidelity: Arc<Fidelity> }`;
  `derive(pixels, identity)` keeps path, metadata and fidelity, and is the only way a crop (`gui/src/images/crop.rs`
  `cropped`), an enhancement result (`inference/process/mod.rs`, fresh or served from the run cache, which stores
  pixels only) or a perf sweep builds a derived picture.
- **Crop.** `gui/src/images/crop.rs` `frame(pixels, crop)`: flip, flip, turn (`rust_sak::image::rotate`, promoting to
  alpha), cut, the rectangle clamped inside; `Crop::scaled(factor)` already re-expresses a framing at another
  resolution (used for bounded previews). The CLI has no crop.
- **Export.** `export::write` builds the embedded metadata with `export::metadata::for_export` (EXIF patched, IPTC
  filtered, XMP rewritten by the `quick-xml` tree of `preserve-metadata-3`, whose policy drops `crs:`, `hdrgm:`,
  `Container:` and `Item:`), assesses `export::losses` (always an HDR loss for a gain-map source), and saves through
  `image::save` → `rust_sak::image::encode_writer_with`. JPEG is `image`'s `JpegEncoder` (8-bit, ICC in `APP2`), with
  EXIF/XMP/IPTC spliced in after `APP0` by `blocks::jpeg::splice`.
- **`rust-sak`.** `blocks/jpeg.rs` walks segments before `SOS`, and `mpf_has_gain_map` already parses the MP Index IFD
  (either byte order, every read bounded by the segment) to find an entry of type `0x050000`.

The sample, `18-1603-hawk-bird-mt_auburn-cemetery.jpg` (Lightroom 9.6, then keywords edited with ExifTool 13.58), as
inspected for this design:

| Primary segment (file offset) | Content |
|---|---|
| `APP1` @2 | EXIF (13,122 bytes, with a thumbnail) |
| `APP13` @13,128 | Photoshop / IPTC |
| `APP2` @25,444 | ICC `Display P3` |
| `APP1` @26,010 | XMP: `hdrgm:Version="1.0"` only - no `Container:Directory` (Adobe style) |
| `APP2` @43,091 | MPF, big-endian, 88 bytes: `MPFVersion 0100`, `NumberOfImages 2`, MP Entry at IFD offset 50 |
| `APP2` @43,181 | `urn:iso:std:iso:ts:21496:-1\0` + 4-byte version payload `00 00 00 00` |
| `DQT`, `DRI`, `APP14`, `SOF0` 1197×1496, `DHT`, `SOS`, … `EOI` at 525,999 | primary image |
| gain map @526,001 | 233,897 bytes; 1197×1496, **3 channels** (multichannel gain map, full resolution), 4:4:4 |

MP Entries: primary `0x20030000` (representative, Baseline MP Primary), size 521,881, offset 0; gain map `0x00050000`
(Gain Map), size 233,897, offset 482,902 = 526,001 − 43,099 (the MPF TIFF header's file position). The primary's
recorded size is **stale** (the primary really ends at 526,001; ExifTool's later edit grew it by 4,120 bytes and
updated only the offset), so a reader must locate the gain map by the offset and verify it, never by the primary's
size. The gain-map image holds `APP1` XMP (an `hdrgm:` description: `Version`, `BaseRenditionIsHDR=False`,
`HDRCapacityMin/Max`, `OffsetSDR/HDR`, per-channel `GainMapMin`, `GainMapMax`, `Gamma` as `rdf:Seq`s) and `APP2` ISO
21496-1 with a 141-byte binary payload; no ICC, no EXIF, no `APP0`.

## Goals / Non-Goals

**Goals:**

- An Ultra HDR / ISO 21496-1 JPEG (Lightroom/Adobe, Android Ultra HDR v1, both metadata forms) enhanced and written as
  JPEG is an Ultra HDR JPEG again, recognised as HDR by macOS Preview/Photos, `libultrahdr` and ExifTool.
- The gain map is geometrically in register with the written base after crop (with flips and turn), upscale, and any
  chain, served fresh or from the run cache.
- The source's metadata form is kept: what was ISO stays ISO, what was `hdrgm` XMP stays `hdrgm`, a Google
  `Container:Directory` stays one with true lengths.
- No new dependency; the container work lives in `rust-sak`; the export never fails because of a gain map.

**Non-Goals:**

- Recomputing the gain map after enhancements that change brightness or colour (light adjustment, colour balance,
  colourisation) - the documented caveat; see Risks.
- Gain maps in AVIF/HEIF/JPEG XL output, or writing a gain-map JPEG from a non-JPEG export choice; HDR creation from
  sources without a gain map (slice 9).
- Apple's pre-ISO private gain maps (`apdi:` XMP, no Gain Map type), AVIF/HEIF `tmap` input (recorded gaps of slice 2,
  still gaps).
- An HDR preview in the GUI: renditions stay SDR.
- Carrying other MPF secondary images or Google `Container` items (Motion Photo video, depth): dropped, as today.

## Slices

The `add-color` series, in order. Each is its own change. Joseph reprioritised the series on 2026-10-22: the Ultra HDR
MVP comes next; AVIF/HEIF, export depth and working space are postponed, JPEG XL is skipped. The numbers of slices not
yet proposed are provisional and are fixed when each is proposed.

| # | Change | Delivers | Depends on | Status |
|---|---|---|---|---|
| 1 | `add-color-1-icc-profile` | Source ICC profile read at load, carried on `Picture` through crop, enhancement and the run cache, embedded on export (JPEG, PNG, TIFF, WebP) and in GUI previews; `rust-sak` ICC read/embed | - | done |
| 2 | `add-color-2-loss-warnings` | `rust-sak` `hdr_bytes`; source fidelity read at load and carried on `Picture`; the per-export loss assessment in `opai`; CLI warning lines and batch count | 1 | in progress |
| 3 | `add-color-3-loss-warnings-gui` | GUI: the export dialog's notice before export and a per-image warning after it; i18n in every locale | 2 | in progress |
| 4 | `add-color-4-ultra-hdr-gainmap` | Ultra HDR MVP: a gain-map JPEG's gain map read at load, carried through crop, enhancement and the run cache, kept at its own resolution (never enlarged) and re-assembled as Ultra HDR JPEG on JPEG export; `rust-sak` MPF/ISO read and write | 2, 3 | **current** |
| 5 | `add-color-5-avif-heif` | ICC and CICP (`nclx`) colour descriptions in AVIF and HEIF, CICP read at load, 10/12-bit AVIF/HEIF encode; extends `avif-rs`/`heif-rs` and `rust-sak` | 1 | postponed |
| 6 | `add-color-6-output-depth` | A chosen export depth (Source / 8 / 16 / 32-bit float) in `opai`, GUI and CLI; a float path out of `imaging`; optional OpenEXR | 1 | postponed |
| 7 | `add-color-7-working-space` | Colour conversion with `moxcms` to and from the models' working space, gamut handling, "convert to sRGB on export", wide-gamut RAW development | 1, 6 | postponed |
| 8 | `add-color-8-jpeg-xl` | JPEG XL decode and encode as a `rust-sak` format and export format | 5, 6 | skipped |
| 9 | `add-color-9-hdr-create` | HDR output from sources without a gain map (RAW developed scene-referred, PQ/HLG AVIF/HEIF): compute a gain map (or write PQ/HLG), and recompute the gain map after brightness-changing enhancements | 4, 6, 7 | pending |

## How the work divides

- **`rust-sak`** (`image` feature): reading the ISO 21496-1 `APP2` payload and the MPF gain-map image out of a JPEG;
  writing them back with an MPF index. Container work only: no gain-map semantics, no XMP parsing, no pixels.
- **`opai`**: `image::GainMap` (decoded pixels, its own metadata, the source's scale); `load` fills
  `Picture::gain_map`; `derive` keeps it; `GainMap::reframed` applies a pixel transform to it at its own scale;
  `export::gain_map` resizes and encodes it; `for_export` keeps the gain-map XMP when it is written; `export::losses`
  stops reporting HDR for JPEG.
- **`gui`**: `crop.rs` `cropped` calls `GainMap::reframed` with its own `frame` and `Crop::scaled` (the framing is
  GUI-only today, see the comment heading that file). React: one new HDR-loss cause in `formatLoss` and all 14
  locales.
- **`cli`**: no code - it writes through `export::write`. `docs/CLI_REFERENCE.md` says what is kept.

## `rust-sak` coverage

Uses `image` (`metadata_bytes`, `encode_writer_with`, `decode_bytes`, `rotate`, `hdr_bytes`) and extends it, because
locating and assembling MPF images is JPEG container knowledge any converter needs and `rust-sak` already owns the
segment walk, the MPF parse and the splice:

1. **`Metadata::iso_gain_map: Option<Vec<u8>>`** - the payload of a JPEG's `APP2` `urn:iso:std:iso:ts:21496:-1\0`
   segment after the URN (4 bytes in a primary: minimum and writer version; the full binary metadata in a gain-map
   image). Read by `metadata_bytes` for JPEG; written by the JPEG encode as one `APP2` (refused above one segment,
   reported `false`). `Embedded::iso_gain_map`.
2. **`Metadata::gain_map: Option<Vec<u8>>`** - the gain-map image, a complete JPEG from `SOI` to `EOI`, byte for byte
   (its own XMP and ISO payload inside). Read by `metadata_bytes` for JPEG: the MP Entry of type Gain Map
   (`0x050000`); failing that, the second entry when the primary declares a gain map by ISO marker or `hdrgm`
   namespace (Android files whose secondary entry has type 0). Written by the JPEG encode as below.
   `Embedded::gain_map`. Every other format reports `false` for both fields.
3. A new `blocks/mpf.rs` holding the MP Index reader (moved from `jpeg.rs`, shared by `has_gain_map`) and writer.

Released as fork tag **`26.10.6-meta.5`** on bookmark `warnings` (on `26.10.6-meta.4`), pushed to the
`reagle/rust-sak` fork only after Joseph's permission; adopted by moving the workspace `[patch]`. Additive and
`#[non_exhaustive]`, so semver-minor. No new crate.

**Why not `ultrahdr-core`/`ultrahdr-rs`/`ultrajpeg` or `libultrahdr` bindings.** The MVP needs no gain-map maths: the
gain map's values describe ratios that a resize or crop does not change, so its metadata is copied byte for byte and
its pixels are resampled like any image. What remains is container assembly, ~300 lines beside code `rust-sak` already
has. imazen's `ultrahdr-core` (0.6.0, October 2026, Apache-2.0, pre-1.0) has moved its MPF/XMP/ISO container code out
to `zenjpeg::container`/`zencodec::gainmap`, so adopting it means a second JPEG codec family and a dozen `zen*` crates
for maths this slice does not use; `ultrahdr-rs` 0.4.0 was yanked. The `ultrahdr` crate binds Google's C++
`libultrahdr` (a CMake build on three platforms, the cost slice 1 rejected for `xmp_toolkit`). Both become candidates
for slice 9, which does need the maths (computing a gain map); `libultrahdr`'s `ultrahdr_app` is used here only as an
external validator in tests and the manual check.

## Decisions

### D1. The gain map travels decoded, beside the pixels, in register with them

`Picture` gains `gain_map: Option<Arc<GainMap>>`, where
`opai::image::GainMap { pixels: Arc<DynamicImage>, metadata: Arc<Metadata>, scale: (f64, f64) }`: the decoded gain-map
image (Luma8 or Rgb8, as stored), the gain-map image's own `xmp` and `iso_gain_map` (nothing else of its metadata),
and the source's gain-map-to-base scale per axis (`gm_width / base_width`, `gm_height / base_height`), fixed at load.
Invariant: **the gain map always covers exactly the scene the picture's pixels cover**; its resolution is free.
`derive` keeps it (`Arc` shared, like the metadata); `Picture::new` has none; `with_gain_map` sets it.

`load` takes `metadata.gain_map` out of the loaded `Metadata` (so the stale encoded bytes never reach an export),
decodes it with `decode_bytes`, reads its `metadata_bytes`, and attaches the result. The primary's own `iso_gain_map`
(the version marker) stays in `Picture::metadata`.

Alternatives: (a) carry the encoded gain map plus a record of the geometric operations and replay them at export -
avoids holding decoded pixels, but every geometric operation must be expressible in the record and replayed
identically, a second spelling of `frame`; (b) decode at export from the source file - the file may have changed or
moved since, and the run cache and GUI never re-read files. Decoded pixels cost one extra image, at the gain map's
resolution (5.4 MB for the hawk; up to the base's size for a full-resolution RGB gain map).

### D2. Only geometric derivations touch the gain map; enhancements leave it alone

An enhancement changes pixel values, not framing, so its result keeps the gain map as is (an upscale's result covers
the same scene at more pixels, which the invariant allows). Only a crop moves the frame:
`GainMap::reframed(&self, f: impl FnOnce(&DynamicImage, f64) -> DynamicImage) -> GainMap` hands `f` the gain map and
its horizontal scale, converts the answer back to the gain map's colour type (a turn adds alpha, which is dropped: the
corners it exposes are transparent in the base and flattened in a JPEG, so their gain is irrelevant), and keeps
`metadata` and `scale`. `cropped` in `crop.rs` calls it with `|map, s| frame(map, crop.scaled(s)).into_owned()` and
attaches it to the derived picture; a framing that changes nothing returns the source as today. Rounding in
`Crop::scaled` can leave the cropped gain map up to one pixel off the exact ratio, which viewers absorb when they
resample it to the base.

### D3. The gain map is never enlarged, and is encoded at quality 90 or more

Decided by Joseph (2026-10-22). At export the gain map keeps its own resolution - the source's, as cropped (D2) - and
is **never upscaled**: ISO 21496-1 and Ultra HDR allow a gain map smaller than the base and every viewer resamples it
to the base, and enlarging a smooth map adds no information, only memory and bytes. Only when the written image is
smaller than the gain map on either axis (no enhancement does that today, but a later downscale may) is the gain map
shrunk to the written image's size, so it is never larger than the base; it is then resized with `image`'s
`resize_exact` and **`FilterType::Triangle`** (bilinear, area-weighted when shrinking): the Ultra HDR spec requires
"bilinear or better", and a gain map stores log-gain, where Lanczos/Catmull-Rom overshoot would put bright and dark
halos along every edge of the HDR rendition. Otherwise it is not resampled at all. The hawk (1197×1496,
full-resolution gain map) denoised keeps a 1197×1496 gain map; upscaled 2× (2394×2992) it still has a 1197×1496 gain
map, a base-to-gain-map ratio of 2. This matches Joseph's sigal plugin, which resizes the gain map with the base's
processor but never enlarges it.

The gain-map JPEG is encoded at the export's JPEG quality (the recommended default when none is chosen) with a **floor
of 90**: the sigal plugin uses the base's quality, and the floor keeps blocking out of a map whose small errors become
visible brightness errors in the HDR rendition, for a few kilobytes.

**An unchanged gain map is not re-encoded** (review fix). Re-encoding on every export lost a generation each time (the
hawk's gain map went 266,010 → 266,009 → 266,022 bytes over three passes). `GainMap` now keeps the source's gain-map
JPEG bytes (`GainMap::source`) until `reframed` replaces its pixels; `prepare` writes those bytes back unchanged,
their own `APPn` segments intact, whenever the gain map is neither reframed nor shrunk, whatever the export's quality.
Only a cropped or shrunk gain map is encoded, as above.

### D4. Byte layout written

The gain-map image is encoded first, so its length is known before the primary's XMP and MPF are written:

```
gain-map JPEG  = SOI · APP0 JFIF (from image's encoder) · APP1 XMP (gain map's packet, byte for byte, if any)
                 · APP2 "urn:iso:std:iso:ts:21496:-1\0" + its payload (if any) · DQT … SOS … EOI
                 (the export's quality, at least 90; no ICC, no EXIF - the spec says a gain map's profile is ignored)
primary JPEG   = SOI · APP0 JFIF · APP1 EXIF · APP1 XMP (+ extended XMP) · APP13 Photoshop
                 · APP2 "urn:iso:std:iso:ts:21496:-1\0" + 4-byte version (if the source had it)
                 · APP2 MPF (88 bytes, below) · APP2 ICC_PROFILE … · DQT … SOS … EOI
file           = primary JPEG · gain-map JPEG     (tightly packed, nothing between)
```

The MPF segment, big-endian as Lightroom and `libultrahdr` write it (`P` = the file offset of its `FF E2`; offsets in
the index are relative to `P + 8`, the `MM` of its TIFF header):

| Bytes | Content |
|---|---|
| `FF E2 00 58` | marker, length 88 |
| `4D 50 46 00` | `MPF\0` |
| `4D 4D 00 2A 00 00 00 08` | TIFF header, big-endian, IFD at 8 |
| `00 03` | 3 entries |
| `B0 00 00 07 00 00 00 04 30 31 30 30` | MPFVersion `0100` |
| `B0 01 00 04 00 00 00 01 00 00 00 02` | NumberOfImages 2 |
| `B0 02 00 07 00 00 00 20 00 00 00 32` | MPEntry, 32 bytes at offset 50 |
| `00 00 00 00` | no next IFD |
| `20 03 00 00` · primary length · `00 00 00 00` · `00 00 00 00` | entry 1: representative, Baseline MP Primary |
| `00 05 00 00` · gain-map length · `primary length − (P + 8)` · `00 00 00 00` | entry 2: Gain Map |

`rust-sak` writes the segment with zeroed lengths, splices it, then patches the two lengths and the offset in place
once the primary's final length is known (the segment's size is fixed, so patching moves nothing). The primary length
is the whole primary, `SOI` to `EOI`.

### D5. The primary's XMP keeps what describes the carried gain map

`xmp::Context` gains `gain_map: Option<usize>` (the encoded gain-map length) and the policy changes only when it is
`Some`: `hdrgm:` properties of the primary are kept as written; a `Container:Directory` is kept, its `GainMap` item's
`Item:Length` set to the new length, the `Primary` item's optional `Item:Length` and every `Item:Padding` removed (the
file is tightly packed and the primary's length is not known when its XMP is written), and every other item removed (a
Motion Photo video is not carried) along with `GCamera:MotionPhoto*`. Neither form is invented when the source lacked
it: the hawk keeps only `hdrgm:Version`, as Lightroom wrote. This needs the `preserve-metadata-3` editor to find an
`rdf:li` in an `rdf:Seq` by a field and set or remove a field in it, or remove the `rdf:li` - small additions to
`edit.rs` over the existing tree. When `gain_map` is `None` the policy drops all three namespaces as it does today,
and `for_export` clears `iso_gain_map` too.

### D6. When the gain map is written, and the failure policy

Written only when the export format is JPEG and the picture carries a gain map. `export::gain_map::prepare` runs
before the primary encode and returns the encoded gain map or a reason; any failure - resize, encode, a gain map
larger than the bounds below, an XMP that cannot be rewritten while the source has no ISO marker (Google-style readers
need `hdrgm:Version` in the primary) - drops it: the primary is written exactly as today (SDR, `hdrgm` dropped) and
the export reports the HDR loss with cause "could not be carried". A gain map that failed to decode at load is dropped
there with `tracing::warn!` and the same loss at export. Nothing about the gain map ever fails a load or an export.

Two more reasons were added on review. **A gain-map image with no gain-map metadata of its own** - no `hdrgm` XMP
namespace and no ISO 21496-1 payload in the secondary image - is refused at load (`GainMapError::Undescribed`) and by
the prediction's header check, since `libultrahdr` and ImageIO find no gain map in it and writing it back would claim
HDR no reader can render. **A primary XMP over 65,502 bytes once rewritten** would be moved whole by `rust-sak`'s
`xmp::split` into extended XMP, `hdrgm:Version` and `Container:Directory` with it, where Ultra HDR readers do not
look. Keeping those two in the standard packet while moving the rest would need `rust-sak` to accept a caller-built
standard/extended pair (it deliberately does not interpret XMP), so `export::write` instead drops the gain map with a
warning and rewrites the XMP as for an SDR export. Real packets fit: Lightroom's are mostly the `crs:` settings the
rewrite drops (the hawk's is 17,048 bytes).

### D7. The loss assessment

`Loss::Hdr` gains `cause: HdrCause` (`Format` - the format cannot hold it, today's wording; `NotCarried` -
`HDR gain map dropped: it could not be carried`), serialized as `cause`. `assess` takes whether the picture carries a
gain map: a `GainMap` source written as JPEG with one reports nothing; without one, `NotCarried`; written as any other
format, `Format`. `predict_blocking` (slice 3) answers "carries" from `metadata_bytes(bytes).gain_map.is_some()`,
without decoding; a decode failure that the prediction did not foresee shows up in the after-export result, which is
the truth.

### D8. Cache, identity, previews

No change to the run cache or identities: a loaded identity hashes the whole file, gain map included; a cropped
identity folds the crop; a cache hit is rebuilt with `source.derive`, which re-attaches the source's gain map.
Previews and thumbnails stay SDR: `without_descriptive` also clears `iso_gain_map` and `gain_map`, so a JPEG rendition
never claims a gain map it lacks.

### Security bounds

All in `rust-sak`'s existing style - untrusted input, every read checked, nothing proportional to a claimed size: the
MP Entry count is bounded by the segment; the gain-map entry's offset and size must lie inside the file after the MPF
header, and the slice must start with `FF D8` and contain an `EOI`, else it is ignored; at most 2 secondary entries
are examined; the copied image is at most the file's own size and at most 64 MiB. In `opai`, the decoded gain map must
be no larger than the base on either axis and no smaller than 1/16 of it, else it is dropped (a hostile secondary
cannot make the load allocate more than the base image already did); its XMP and ISO payload are carried, never
parsed, beyond `rust-sak`'s existing XMP plausibility check. The export's target size is bounded by the output's own
size.

## Risks / Trade-offs

- [Brightness-changing enhancements (light adjustment, colour balance, colourisation) make the gain map's ratios
  slightly off: HDR highlights a little too bright or too dull where the base changed] → documented in the CLI
  reference and module docs as a known MVP caveat; recomputing is slice 9. Denoise, sharpen, upscale and face recovery
  barely change local brightness, so Joseph's main workflow is sound.
- [After an upscale the base-to-gain-map ratio is the upscale factor, and a non-integer one (a crop rounded, a 1.5×
  upscale) may be refused by a decoder that wants an integer ratio (early `libultrahdr`)] → ISO 21496-1 and Ultra HDR
  v1.1 allow any ratio with a similar aspect; the manual check runs `ultrahdr_app`, macOS and ExifTool on a 2× upscale
  and a crop; if one refuses, D3 can round the gain map to the nearest integer ratio without other changes.
- [`image`'s JPEG encoder subsamples chroma for an RGB gain map, where Lightroom wrote 4:4:4] → the per-channel gains
  are smooth; checked visually on the hawk. If it shows, encode the gain map 4:4:4 (an `EncodeOptions` addition in
  `rust-sak`).
- [The primary XMP grows past 65,502 bytes and `hdrgm:Version` lands in extended XMP, where Android does not look] →
  the policy drops `crs:`, which is most of a Lightroom packet; a test asserts the hawk's rewritten packet is
  standard-size. The ISO marker and MPF still identify the gain map for ISO readers.
- [The source's MPF primary length is stale (the hawk's is)] → the reader trusts only the gain-map entry's offset and
  size and verifies the bytes; the writer computes both lengths itself.
- [A reader that checks `NumberOfImages` against other MP attributes, or expects the MPF segment directly after EXIF]
  → ExifTool, `libultrahdr` and macOS are checked in task 4.2; the segment order is the documented choice above and
  can move without changing anything else.
- [Rounding in `Crop::scaled` for non-integer ratios] → at most one gain-map pixel, absorbed by the viewer's resample;
  a test crops with a 1/4-scale gain map and checks register within one gain-map pixel.

## Platforms

Pure Rust in `rust-sak` and `opai`; no execution-provider or model-session change. Planned validation: macOS (Apple
Silicon) - tests, `exiftool -validate`, `ultrahdr_app -P`, and Preview/Photos showing the result as HDR. Windows and
Linux are untested beyond CI; no platform code is involved.

## Migration Plan

Additive in `rust-sak` (new `Metadata`/`Embedded` fields, `#[non_exhaustive]`). In `opai`, `Loss::Hdr` gains a field
(internal, both front ends in-tree). Rollback: revert the `opai` commit and the `[patch]` move; files written
meanwhile are valid Ultra HDR JPEGs.

## Difficulty and likely paths

**Moderate; about 4-6 days.** The pieces are each known: the segment splice and MPF parse exist in `rust-sak`, the XMP
tree exists in `opai`, the crop already has a scaled form. Likely path: `rust-sak` first (read, write, round trip
against the hawk-shaped fixture and against `ultrahdr_app`), then the `opai` load/derive/export plumbing with a
synthetic fixture, then the XMP `Container` edit, then the GUI crop.

Hardest parts, in order: (1) **reader interoperability** - getting macOS, Android/`libultrahdr` and Adobe to all
accept the file, which is why the source's own forms are kept byte for byte and why the manual check covers three
readers; (2) the **XMP `Container:Directory` edit**, the one structural edit the `preserve-metadata-3` editor has not
done; (3) the **crop register** with a turn and a non-1 scale.

Alternatives if the MVP path stalls: replace the hand-built MPF/assembly with `libultrahdr` through the `ultrahdr`
crate (`uhdr_enc_set_compressed_image` + `set_gainmap_image` + metadata, which is exactly what Joseph's sigal plugin
does through `ultrahdr_app -m 0`) - more robust interop, at the cost of a C++ build; or with imazen's
`zenjpeg::container` once it stabilises. Upstream acceptability: moderate - no new dependency, a niche but growing
format, and the `rust-sak` additions are general-purpose.

## Open Questions

None. Joseph decided the two left open in the first draft (2026-10-22): the gain map is never enlarged (D3), and it is
encoded at the export's quality with a floor of 90 (D3).

## Implementation notes and deviations

Recorded on implementation (2026-10-22, Claude with Joseph's go-ahead relayed by the coordinating agent).

- **`rust-sak` released twice.** `26.10.6-meta.5` (commit `f53270df`) carries tasks 1.1-1.3. Testing on an
  ExifTool-edited copy of the hawk (`exiftool -Comment=...`) found a second kind of stale index: ExifTool inserted the
  `COM` after the MPF segment without updating the gain map's offset, so it pointed 13 bytes early. `libultrahdr` and
  Apple's ImageIO still read that file, and the strict reader dropped it. `26.10.6-meta.6` (commit `14613d79`) adds a
  fallback: where the entry's offset does not land on a valid image, the **first** secondary image is taken from
  straight after the primary's `EOI` (found by walking the primary's segments and scans) to its own `EOI`, at most 64
  MiB. It also narrows the Android rule: a declared primary's first secondary entry is taken as the gain map only when
  its type is 0, not any type. The workspace `[patch]` pins `26.10.6-meta.6`; `meta.5` was left in place, as tags are
  never moved. Both tags and the bookmark `uhdr` (not `warnings`, which stays at `meta.4`) are on `reagle/rust-sak`
  only.
- **Shrinking (D3)** is per axis: the gain map is resized to
  `(min(gain-map width, written width), min(gain-map height, written height))`, which is the written image's size
  whenever the aspect ratios agree and never larger than it otherwise.
- **Whether the gain map was written (D7)** is taken from `rust-sak`'s `Embedded::gain_map`, which the export now
  receives (`add-color-2-loss-warnings` D6, a review fix there); `rust-sak` refuses a prepared gain map only when a
  size does not fit the MPF index's 32 bits (a primary or gain map over 4 GiB), which is then reported as
  `NotCarried`.
- **D6's XMP condition** is implemented as: the gain map is dropped only when the source had XMP, its rewrite failed,
  and the source has no ISO 21496-1 marker.
- **`predict_blocking`** answers "carries" from `metadata_bytes(...).gain_map` plus the same header-only size check
  the load applies (`GainMap::plausible`), so a gain map larger than the photograph is foreseen as `NotCarried` too.
- **The hawk's primary XMP is not committed.** The 65,502-byte test uses the existing Lightroom-shaped packet padded
  with 3,000 `crs:` attributes (over 120 kB); the real hawk's primary packet is 17,048 bytes. The gain-map XMP and
  141-byte ISO payload are committed as `fixtures/hawk-gain-map.{xmp,iso}` (and in `rust-sak` as
  `blocks/fixtures/gain-map.{xmp,iso}`).
- **Chroma subsampling risk did not occur:** the `image` crate's encoder writes the gain map 4:4:4 at quality 90 and
  above (checked on the hawk's exports).
- **Task 4.2, as run.** On copies of the hawk in a scratch directory (never under `~/f/photo`), with the CLI:
  `-q sd --dn` → JPEG; `-q sd --up` → JPEG; `--dn` → PNG and AVIF; and `--up` of an ExifTool-edited copy. The turned
  (5°), mirrored, cropped-to-left-half export was made by a temporary, uncommitted test calling `crop.rs`'s `cropped`
  and `export::write` on the real hawk, not by hand in the GUI. Results:

| Output | Base | Gain map (MPF image 2) | Checks |
|---|---|---|---|
| denoise, JPEG | 1197×1496 | 1197×1496, Gain Map type, offset on `SOI` | ISO URN, `hdrgm:Version`; `ultrahdr_app -P` Yes; ImageIO ISO gain map, CoreImage HDR peak 6.96 vs SDR 1.34; `exiftool -validate`: 3 warnings, all present in the source (which has 5) |
| upscale 2×, JPEG | 2394×2992 | 1197×1496 (not enlarged) | as above; `ultrahdr_app -m 1` decodes to a 2394×2992 HDR buffer |
| upscale of the ExifTool-edited copy | 2394×2992 | 1197×1496 | as above, with `meta.6` (`meta.5` dropped it and reported `NotCarried`) |
| turned, mirrored, cropped | 538×1336 | 538×1336, visually in register | ISO URN, `hdrgm`, `ultrahdr_app` Yes, ImageIO HDR |
| PNG, AVIF | - | none | the CLI warns `HDR gain map dropped: PNG/AVIF output is SDR only for now` |

  Not checked: viewing in macOS Preview and Photos on an HDR display (ImageIO and CoreImage recognise the gain map; a
  check by eye is left to Joseph), the GUI by hand, Windows and Linux.

- **Review fixes, as run.** The hawk with `-q sd --sh` to JPEG, and that output again: both have two MPF images, the
  gain map of both is byte-identical to the source's, ImageIO reports an ISO gain map and `ultrahdr_app -m 1` decodes
  the second to an HDR buffer. The reviewer's `bare_gm.jpg` (a Gain Map MPF entry with no gain-map metadata) is now
  written as SDR with `HDR gain map dropped: it could not be carried`, and ImageIO and `ultrahdr_app` find no gain map
  in the result, as in the source.

## References

- Ultra HDR image format v1.1 (Android): https://developer.android.com/media/platform/hdr-image-format - `hdrgm:`
  properties, `Container:Directory`, "bilinear or better", ISO 21496-1 `APP2` after XMP in both images, decoders
  prefer ISO.
- Adobe gain map specification and ISO 21496-1 status: https://helpx.adobe.com/camera-raw/using/gain-map.html ; ISO
  21496-1 sample JPEGs: https://people.csail.mit.edu/ericchan/hdr/jpeg-gain-map-iso.html
- CIPA DC-007 Multi-Picture Format (MP Index IFD, MP Entry attributes, offsets relative to the MP header).
- `ultrahdr-core` 0.6.0: https://docs.rs/ultrahdr-core ; `ultrahdr-rs`: https://docs.rs/ultrahdr-rs ; `ultrajpeg`:
  https://docs.rs/ultrajpeg ; `libultrahdr` bindings: https://docs.rs/ultrahdr ; libultrahdr:
  https://github.com/google/libultrahdr
- Pillow gain-map loss and Joseph's workaround: https://github.com/python-pillow/Pillow/issues/8036 ,
  `~/dev/sigal-ultrahdr-gainmap`.
