## Context

An Open Photo AI export carries no EXIF, IPTC, XMP or ICC today. The path a photograph takes explains why:

- **Decode.** `opai::image::load_blocking` (`crates/opai/src/image/load.rs`) reads the file into one buffer, hashes it
  for the identity, and hands it to `rust_sak::image::decode_bytes` or `decode_raw_bytes`. Native formats go through
  `image::load_from_memory_with_format`; AVIF, HEIF and WebP through `avif-rs`, `heif-rs` and `webp-rs`; RAW through
  zenraw over rawler. Each returns a `DynamicImage` and nothing else, so everything outside the pixels is discarded
  here.
- **Carry.** `Picture` (`crates/opai/src/image/picture.rs`) holds a source path, `Arc<DynamicImage>` and an identity.
  Derived pictures are made with `Picture::new(source.path(), pixels, identity)` in `inference/process/mod.rs`,
  `inference/execute.rs`, `models/upscale/osaka/live.rs`, `models/colorization/live.rs`, `live_support.rs` and
  `gui/src/images/crop.rs`.
- **Encode.** Both front ends end in `opai::export::write` (`crates/opai/src/export/write.rs`): the GUI from
  `crates/gui/src/export/run.rs`, the CLI from `crates/cli/src/output.rs::write`. It calls `image::save`, which
  streams `rust_sak::image::encode_writer` into a counted file writer. `--format` / the export dialog's format choice
  (including Preserve, which writes RAW as TIFF) picks the `ImageFormat`.
- **Orientation.** Native decodes do **not** apply EXIF Orientation (`image` 0.25 only does so on an explicit
  `apply_orientation`), so a JPEG tagged Orientation 6 is held, enhanced and exported in its stored, unrotated frame.
  A developed RAW **is** oriented by the decoder (rust-sak `image` README: "then the crop and EXIF orientation the
  camera recorded"). Whatever Orientation an export writes has to be right for each of those two cases.

`rust-sak` was checked first. Its `image` module has the codecs, an ISOBMFF box walker (`container.rs`) used for the
`colr` box, and an ICC `desc` reader (`icc.rs`) used by `probe`. It has no EXIF, IPTC or XMP reader or writer of any
kind.

## Goals / Non-Goals

**Goals (this slice):**

- `rust-sak` can read the EXIF, XMP and IPTC blocks out of JPEG, PNG and WebP bytes, and splice given blocks into a
  JPEG, PNG or WebP it encodes.
- `opai`'s `Picture` carries those blocks from load to export, through every derived picture, without copying them and
  without affecting the identity.

**Goals (the series):** an export always keeps all of the source's metadata that is still true of the output, with no
setting to turn it off --- capture date/time and offsets, camera/lens/exposure, GPS, IPTC/XMP keywords, title,
description, caption, rating, label, creator, copyright --- and records that Open Photo AI processed it, keeping the
earlier software in the XMP history.

**Non-Goals (this slice):** writing anything into an export, interpreting any field, TIFF/AVIF/HEIF/RAW sources or
outputs, ICC profiles (the `add-color-` change), and applying orientation on decode.

## Slices

| # | Change | Delivers | Depends on | Status |
|---|---|---|---|---|
| 1 | `preserve-metadata-1-capture` | `rust-sak`: `exif`/`xmp`/`iptc` on the shared `Metadata` (created here if `add-color-1-icc-profile` has not landed), `metadata_bytes`, and splicing in the shared `encode_*_with` for JPEG/PNG/WebP (hand-written container surgery, not `img-parts`); fork tag `26.10.6-meta.1`. `opai`: `[patch]` moved; the shared `Picture.metadata` carries the blocks; `Picture::derive` used by every derived picture; exports and renditions mask the blocks out. Nothing EXIF/XMP/IPTC written yet. | --- | **current** (implemented 2026-10-22) |
| 2 | `preserve-metadata-2-exif` | `rust-sak`: an offset-preserving EXIF patcher (D11) --- in-place overwrite of fixed-size values, append-only relocation of grown values and IFDs, thumbnail detach and zeroing --- with the dropped-MakerNote fallback; release. `opai`: the EXIF half of D6 (Software, DateTime/OffsetTime, Orientation, pixel dimensions, thumbnail, MakerNote kept when safe) and the IPTC-IIM resource filter, written by `export::write` into JPEG, PNG and WebP. XMP still masked. Implemented with this module's own rebuild rather than `kamadak-exif` (slice 2's D2). | 1 | implemented 2026-10-22 |
| 3 | `preserve-metadata-3-xmp` | `opai`: the XMP half of D6 with `quick-xml` --- InstanceID/DocumentID/DerivedFrom, the `xmpMM:History` events, CreatorTool, ModifyDate/MetadataDate, tiff:/exif: mirrors kept in step with slice 2's EXIF, `crs:*` dropped --- and the IPTC-IIM to XMP mapping for outputs that cannot hold IIM (D12). The mask is removed from `export::write`: everything is written. | 2 | implemented 2026-10-22 |
| 4 | `preserve-metadata-4-tiff` | TIFF read and write: EXIF as IFD tags, XMP as tag 700, IPTC as tag 33723. Needs `rust-sak`'s TIFF encode to go through the `tiff` crate's directory encoder (already in the graph at 0.11.3) instead of `image`'s `TiffEncoder`, which has no custom-tag hook. Matters for Preserve, which writes RAW as TIFF. | 3 | pending |
| 5 | `preserve-metadata-5-heif-avif` | AVIF/HEIF read and write of the `Exif` and `mime` (XMP) items. Extends `heif-rs` and `avif-rs` (first-party, wrapping libheif's `heif_context_add_exif_metadata`/`add_XMP_metadata` and libavif's `avifImageSetMetadataExif`/`XMP`), then `rust-sak`, then adopts. Matters for iPhone HEIC sources. | 3 | pending |
| 6 | `preserve-metadata-6-raw` | RAW sources: EXIF from TIFF-based RAW (NEF, ARW, CR2, DNG, PEF, ORF, RW2) by parsing the file's own IFDs, CR3 and RAF through their containers, and the XMP sidecar (`IMG_0001.xmp` / `IMG_0001.CR3.xmp`) Lightroom, darktable and Capture One write beside a RAW. A RAW's EXIF has to be rebuilt rather than patched (its IFD0 describes sensor data), so its MakerNote takes D11's relocation rule. | 3, 4 | pending |

There is **no setting** in the series: metadata, location included, is always kept (decided by the project owner). An
earlier draft's `-3-setting` slice (an export-dialog control and `--strip-metadata`/`--strip-location`) was dropped
for that reason, and the former single export slice was split in two --- EXIF and XMP --- because keeping MakerNotes
safely (D11) adds a `rust-sak` patcher and its release, which with the XMP work would exceed 15 tasks.

Slices 4, 5 and 6 are independent of each other after 3 and may be reordered by priority; 6 depends on 4 only because
Preserve writes RAW as TIFF and the RAW case is not worth much until TIFF output carries metadata.

### Decisions already taken for later slices

Recorded here so slices 2 and 3 are built against them rather than rediscovering them.

**Format capability matrix** (what each output can hold once the series is done):

| Output | EXIF | XMP | IPTC-IIM | Notes |
|---|---|---|---|---|
| JPEG | APP1 `Exif\0\0` | APP1 `http://ns.adobe.com/xap/1.0/` (+ extended XMP over 64 KB) | APP13 `Photoshop 3.0` | everything |
| PNG | `eXIf` (PNG 3rd edition) | `iTXt` `XML:com.adobe.xmp` | none standard | IPTC reaches PNG only through XMP |
| WebP | `EXIF` chunk | `XMP ` chunk | none | needs the `VP8X` extended header and its flag bits |
| TIFF | IFD tags + Exif/GPS sub-IFDs | tag 700 | tag 33723 | slice 4 |
| AVIF / HEIF | `Exif` item | `mime` item `application/rdf+xml` | none | slice 5 |
| BMP, GIF | none | (GIF: XMP application extension, not worth it) | none | metadata cannot be kept; the export logs it once per file |

JPEG XL is not one of this application's eight output formats and is out of scope.

**IPTC across formats.** The IIM block is copied into JPEG (and TIFF in slice 4). Into every other format, the IPTC
Core fields reach the output through XMP; where the source has IIM but its XMP lacks a property, slice 3 maps it
(D12).

## Decisions

### D1 --- The format plumbing goes into `rust-sak`; the policy stays in `opai`

Reading a block out of a JPEG and splicing one back in is what every re-encoding program needs, and it sits beside the
codecs it wraps, so by the project's rule it extends `rust-sak`'s `image` module rather than being written in `opai`.
**Which** fields survive, what Software says, how Orientation and the history are rewritten, is this application's
export policy and stays in `opai` (slice 2). The line is drawn at "raw bytes in, raw bytes out": `rust-sak` never
parses a tag.

Considered and rejected: putting everything in `opai` (would trap a general capability in an app, against the rule);
putting the rewrite policy in `rust-sak` too (it would encode one application's opinions, such as the Software string
and its MakerNote rules, into a shared crate). The one exception is D11's offset-preserving EXIF patcher, which is a
general primitive --- "change these tags without moving anything" --- and so goes into `rust-sak` in slice 2.

### D2 --- `rust-sak` API: the shared seam, extended

The colour series (`add-color-1-icc-profile`, its design D1--D2 and tasks 1.2, 2.2) designs the same seam for the ICC
profile. There is **one** design, and **whichever change lands first creates the shared type and seam; the other adds
its fields**. Neither renames, reshapes or duplicates what the other created. Under the existing `image` feature, no
new feature:

- **`Metadata`** --- `#[non_exhaustive]`, `Default`, `Clone`, `Debug`. The colour change owns
  `icc_profile: Option<Vec<u8>>`; this change adds `exif`, `xmp` and `iptc`, each `Option<Vec<u8>>`. `exif` is the
  TIFF stream without the JPEG `Exif\0\0` prefix (the form PNG `eXIf`, WebP `EXIF` and HEIF's item all use); `xmp` is
  the whole packet, extended XMP reassembled; `iptc` is the Photoshop image-resource block from APP13. Because it is
  `#[non_exhaustive]`, adding fields is not a breaking change for either series, and consumers build it with `Default`
  plus field assignment.
- **`Embedded`** --- the one report of what an encode wrote, `#[non_exhaustive]`, one `bool` per `Metadata` field. The
  colour change owns `icc_profile`; this change adds `exif`, `xmp` and `iptc`. A format that cannot hold a block
  reports `false`; the encode never fails on metadata.
- **`encode_file_with` / `encode_writer_with(image, ..., format, options, &Metadata) -> Result<Embedded>`** --- the
  one entry point; `encode_file`/`encode_writer` stay as wrappers passing `Metadata::default()`. The colour change
  embeds ICC through the codecs' own `set_icc_profile` (and a RIFF rewrite for WebP); this change adds a post-encode
  splice of EXIF, XMP and IPTC for JPEG, PNG and WebP. When there is nothing of this change's to splice, the encode
  streams exactly as before. The two WebP RIFF rewrites (`ICCP` there, `EXIF`/`XMP ` here, all needing `VP8X`
  promotion and the same RIFF size fix) become one helper in whichever change lands second.
- **`metadata_bytes(bytes: &[u8]) -> Metadata`** --- this change's reader, format from magic bytes, infallible: an
  unsupported format or a malformed block gives `None` for that field, because metadata is never a reason to refuse a
  photograph. Where the colour change's `icc_profile_bytes` exists, `metadata_bytes` fills `icc_profile` through it,
  so `load` makes one call; where it does not yet, `icc_profile` stays `None` and the colour change wires it in.

Buffering for the splice costs one encoded file in memory, small beside the decoded picture already held.

### D3 --- `img-parts` for segment surgery

`img-parts` 0.4.1 (MIT OR Apache-2.0, pure Rust, no C, released 2026-10-17) parses JPEG into segments, PNG into chunks
and WebP into RIFF chunks, re-serialises them byte-exactly, and has `ImageEXIF`/`ImageICC` traits for all three. XMP
and IPTC are added through its generic segment/chunk API (APP1 with the XMP namespace header, APP13, PNG `iTXt`, RIFF
`XMP ` plus the `VP8X` XMP flag). It is exactly the layer D1 draws and no more.

Alternatives weighed:

- **`image` 0.25.10's `ImageEncoder::set_exif_metadata`** (added in 0.25.7/0.25.8 for JPEG, PNG and WebP) and
  **`ImageDecoder::exif_metadata`/`xmp_metadata`** (0.25.4 / 0.25.9). Already in the graph, but EXIF-only on the write
  side (no XMP, no IPTC), and `rust-sak` encodes WebP through `webp-rs`, not `image`'s WebP encoder, so it would cover
  two of three formats for one of three blocks. Rejected as the main path; slice 4 may still use the decoder's TIFF
  readers.
- **`little_exif` 0.6.23** (MIT OR Apache-2.0, pure Rust). Reads and writes EXIF in many containers, but EXIF only,
  its `write_to_vec` supports only JPEG, and its constructors panic on unsupported input. Rejected.
- **`rexiv2` / gexiv2 / Exiv2.** The most complete metadata engine there is, but GPL-3.0-or-later bindings over a C++
  library and GObject that must be present or built on all three platforms, last released 2023. Rejected for the
  build, not the licence.
- **`xmp_toolkit` 1.12.1** (Adobe's XMP Toolkit, MIT OR Apache-2.0 wrapper, BSD SDK). Compiles ~90 k lines of C/C++
  and handles file formats via Adobe's handlers. Heavy C++ toolchain on Windows/macOS/Linux CI for what is, in this
  design, a splice and an XML edit. Kept as the fallback for slice 3's XMP editing if hand-rolled editing proves
  fragile.
- **`kamadak-exif` 0.6.1** (BSD-2-Clause) and **`nom-exif` 3.8.0**: parsers, not container writers. `kamadak-exif`'s
  `experimental::Writer` rebuilds a clean TIFF stream and synthesises every offset, which is exactly what moves a
  MakerNote; under D11 it is only the fallback rebuild (MakerNote dropped), and slice 6's rebuild of RAW EXIF.
- **Shelling out to `exiftool`.** Rejected: a Perl runtime the user must install, a subprocess per export, and
  `rust-sak` explicitly prefers no subprocesses.

All licences are compatible with AGPL-3.0.

### D4 --- The shared `Picture.metadata: Arc<Metadata>`

As in the colour design's D1--D2: `Picture` gains `metadata: Arc<Metadata>`, with `metadata()`,
`with_metadata(self, Metadata)` (used by `load`) and `derive`. Whichever change lands first adds them; the other only
fills its own fields. Behind an `Arc` for the same reason the pixels are: a clone after an 8x upscale must stay a
refcount bump, and the GUI clones pictures freely. An empty source is `Metadata::default()`. `load.rs` fills it with
`metadata_bytes` on the `bytes` buffer already held for the identity --- the file is still read once. `opai::image`
re-exports `Metadata` and `Embedded` as it already re-exports the rest of `rust-sak`'s image vocabulary.

**Until slice 2, the blocks are masked at the encoder.** If the colour change has already made `export::write` and the
rendition pass `picture.metadata()` to `encode_*_with`, the raw EXIF, XMP and IPTC would be written unrewritten --- a
wrong Orientation for a RAW, stale dimensions, a stale thumbnail, a Software tag naming a camera. So this slice adds
one `opai` function, `image::without_descriptive(&Metadata) -> Metadata` (a copy with `exif`, `xmp`, `iptc` cleared,
`icc_profile` kept), and both `export::write` and the GUI rendition encode through it. Slice 2 lifts the mask for EXIF
and IPTC in `export::write`, replacing it with their rewrite, and slice 3 lifts it for XMP. The rendition keeps the
mask permanently: a browser applies an EXIF Orientation to an `<img>`, which would turn the canvas away from the frame
the crop is measured in, and a preview has no use for the rest.

### D5 --- The shared `Picture::derive` keeps metadata by construction

`Picture::derive(&self, pixels, identity) -> Picture` keeps `self`'s path and metadata and takes the new pixels and
identity --- the same constructor the colour design's D2 specifies, created by whichever change lands first. Every
production derived-picture site listed in Context moves to it (the colour change covers `inference/process/mod.rs` and
`gui/src/images/crop.rs`; this change covers whichever of those remain plus `inference/execute.rs`,
`models/upscale/osaka/live.rs`, `models/colorization/live.rs` and `live_support.rs`), so carrying metadata is the
default rather than something each site remembers. `Picture::new` keeps its signature, for a picture with no source,
and yields default metadata. The identity is untouched (the spec's "A computed image keeps its source's metadata":
metadata does not key the cache) --- a loaded picture's identity already covers its metadata bytes, since it hashes
the whole file.

### D6 --- Rewrite policy (implemented in slices 2 and 3, decided here)

What slices 2 (EXIF, IPTC) and 3 (XMP) copy, rewrite and drop, so that nothing written is false of the output. The
rule is *keep everything*, and change only what would otherwise be false:

| Field(s) | Treatment |
|---|---|
| DateTimeOriginal, CreateDate (DateTimeDigitized), SubSecTime*, OffsetTimeOriginal, OffsetTimeDigitized | copy |
| Make, Model, LensMake, LensModel, LensSpecification, SerialNumber, BodySerialNumber, exposure (ExposureTime, FNumber, ISO, ExposureProgram, ExposureBias, MeteringMode, Flash, FocalLength, FocalLengthIn35mm, WhiteBalance...) | copy |
| GPS IFD (all), Interop IFD | copy, always; there is no option to strip location |
| Artist, Copyright, ImageDescription, UserComment, XPTitle/XPKeywords etc. | copy |
| IPTC-IIM keywords, caption, headline, by-line, copyright, city/location | copy the IIM dataset resource (0x0404) and its digest (0x0425) only; drop every other Photoshop resource (thumbnails 0x0409/0x040C, slices, paths) |
| XMP dc:*, photoshop:*, Iptc4xmpCore/Ext:*, xmp:Rating, xmp:Label, lr:hierarchicalSubject, exif:/tiff: capture properties | copy; exif:/tiff: mirrors of a rewritten EXIF field (Orientation, dimensions, DateTime, Software) are rewritten to match |
| Software (0x0131) | rewrite to `Open Photo AI <version> (<YYYY-MM-DD>)`, e.g. `Open Photo AI 26.10.7 (2026-10-22)`: the version from `CARGO_PKG_VERSION`, the date the export's local date. The earlier value survives in the XMP history and CreatorTool (below) |
| DateTime / ModifyDate (0x0132), OffsetTime (0x9010) | rewrite to the export time and local offset |
| xmp:CreatorTool | keep if present (it names the creating tool); where absent, set to the source's original EXIF Software if there was one, otherwise to the Software string above |
| xmp:ModifyDate, xmp:MetadataDate | rewrite to the export time |
| xmpMM:DocumentID | new `xmp.did:<uuid>`: the export is a new file beside the original (XMP's "Save As" case) |
| xmpMM:OriginalDocumentID | keep; where absent, set to the source's DocumentID |
| xmpMM:InstanceID | new `xmp.iid:<uuid>` |
| xmpMM:DerivedFrom | set to the source's DocumentID/InstanceID |
| xmpMM:History | keep every event; if the source has no history but has a Software value, first add a `created` event with `stEvt:softwareAgent` set to that original Software and `stEvt:when` set to the source's DateTime; then append `stEvt:action="converted"` (or `"saved"` when the format did not change) with `stEvt:softwareAgent="Open Photo AI <version>"`, `stEvt:when`, `stEvt:instanceID` and `stEvt:parameters` naming the enhancements applied |
| Orientation (EXIF 0x0112, tiff:Orientation) | copy when the pixels are in the stored frame (native sources, D7); set to 1 when the decoder already oriented them (RAW) |
| PixelXDimension, PixelYDimension, exif:PixelX/YDimension, tiff:ImageWidth/ImageLength | rewrite to the output's size (upscale and crop change it) |
| IFD0 ImageWidth/ImageLength, BitsPerSample, Compression, PhotometricInterpretation, SamplesPerPixel, strip/tile tags | rarely present in a JPEG/PNG/WebP EXIF; where present, ImageWidth/ImageLength are patched to the output size and the rest are left (a TIFF output, slice 4, writes its own) |
| IFD1 and its thumbnail, JPEGInterchangeFormat* | drop: IFD0's next-IFD pointer is set to 0 and the thumbnail bytes are zeroed in place, so nothing moves (D11). Not regenerated. A stale thumbnail shows the unenhanced, uncropped picture --- including whatever the crop cut out --- so keeping it would make a file whose preview contradicts it and can leak cropped content |
| MakerNote (0x927C) | **copy when safe**, per D11: kept byte for byte at its original offset in the original TIFF stream. Dropped, with a warning logged naming the file and the reason, only where D11 cannot keep it safely |
| XResolution, YResolution, ResolutionUnit | copy (they are print density, not pixel count) |
| ColorSpace (0xA001), photoshop:ICCProfile, InteropIndex | copy as read in slices 2 and 3, then owned by the `add-color-` change, which decides them together with the ICC it embeds: ColorSpace must say 1 only when the pixels are sRGB and 0xFFFF when an embedded profile says otherwise |
| xmpNote:HasExtendedXMP | rewritten by the JPEG writer when the packet is re-split |
| photoshop:History, crs:* (Camera Raw develop settings) | drop `crs:*` --- they describe edits to the original RAW and would be re-applied to an already-developed file by Lightroom/ACR; keep photoshop:History |

### D7 --- Orientation is described, not applied

This slice does not change decoding. Native sources stay in their stored frame and keep their Orientation tag, which
is then still correct for the exported pixels; RAW sources come out oriented, so their Orientation becomes 1.
`Metadata` therefore needs no orientation flag; slice 2 decides from the source kind, which `opai` already knows
(`is_raw`). The visible consequence --- the GUI showing an Orientation-6 JPEG sideways because it never applied the
tag --- predates this series and is noted for whoever applies orientation on decode: that change must make slice 2
write 1, and the crop's coordinates must move with it.

### D8 --- Boundary with the colour change

ICC profiles and colour conversion belong to the `add-color-` change. The two share the `Metadata` value, `Embedded`,
`encode_*_with`, `Picture.metadata` and `Picture::derive` (D2, D4, D5), but this change never reads, writes or
interprets `icc_profile`: its own reader does not parse `APP2`/`iCCP`/`ICCP` (it delegates to the colour change's
`icc_profile_bytes` where that exists), and its splice never touches a profile the codec embedded. The two touch at
three fields --- EXIF `ColorSpace`, EXIF `InteropIndex` (`R98`/`R03`) and XMP `photoshop:ICCProfile` --- which D6
hands to that change. Whichever of the two lands second makes them agree.

### D9 --- `rust-sak` features used

`image` only, already enabled by `opai`. `perf` moves its pin in lockstep (two tags of one git source would put two
`rust-sak` entries in the lock file).

### D10 --- Release shape

As in `add-crop-1-rust-sak`: fmt, clippy, tests and doc tests green in `../rust-sak`; then, **only with the repository
owner's explicit go-ahead**, push, bump to the CalVer number for the month, tag. Consumers pin by tag, so a push is a
release.

### D11 --- MakerNotes are kept by never moving them (implemented in slice 2, decided here)

A MakerNote is an opaque blob whose internal pointers are, depending on the maker, relative to its own start (Olympus,
Panasonic), to an embedded TIFF header of its own (Nikon type 3), or **absolute from the start of the enclosing TIFF
stream** (Canon, Sony, Pentax, Samsung, several others). The last kind survives only if the MakerNote keeps its exact
byte offset in the TIFF stream. Rebuilding the EXIF from parsed fields (`kamadak-exif`'s writer, `little_exif`) moves
it and silently corrupts it.

**Chosen approach: patch the original TIFF stream, append-only.** The EXIF block is kept byte for byte, and every
change is made without moving a single existing byte:

1. **Fixed-size values are overwritten in place.** A value of four bytes or fewer lives in its IFD entry, so
   Orientation, PixelX/YDimension (a SHORT entry may be retyped to LONG in place, same count, same 4-byte slot) and
   similar are rewritten where they are. DateTime is a fixed 20-byte ASCII value and is overwritten at its offset.
2. **A value that grows is appended.** Software longer than its old slot is written at the end of the stream and its
   entry's offset repointed; the old bytes are zeroed. Nothing before the end moves.
3. **A tag that must be added relocates its IFD, not its data.** Adding Software or OffsetTime where absent means a
   longer IFD; the IFD (entries only) is copied to the end with the new entry in sort order and its parent pointer
   repointed. Every out-of-line value it references keeps its absolute offset, so the MakerNote, which is such a
   value, stays where it is.
4. **The thumbnail is detached and zeroed** (D6), and its freed bytes are the first space used for appended data,
   which keeps the block within JPEG's 65,533-byte APP1 limit for a source that was close to it.

The TIFF base is the stream's own header, so the same patched stream is valid in JPEG APP1, PNG `eXIf` and WebP
`EXIF`: no container conversion moves an offset.

**"Safe" is defined as:** the original stream parses as a TIFF structure (header, IFD0, Exif IFD, the MakerNote
entry's offset and count within the stream), every change in 1--3 fits, and the result fits the target container. A
MakerNote whose own pointers are relative (Olympus, Panasonic, Nikon type 3) is also safe to **relocate**, which is
what slice 6 relies on when RAW EXIF has to be rebuilt.

**When it is not safe** --- the stream does not parse, a patched JPEG block would exceed 65,533 bytes even after
reusing the thumbnail's space, or a rebuild is unavoidable for an absolute-offset maker --- the fallback is the
rebuild with `kamadak-exif`'s writer **without** the MakerNote, and a warning logged naming the file, the maker and
the reason. Every standard field survives the fallback; lens and serial live in standard EXIF 2.3 tags for every
camera since about 2010.

Alternatives weighed: **exiv2's approach**, which re-serialises the EXIF and re-bases known MakerNote formats through
a per-maker table --- more general, but it is a maker database to maintain, and append-only patching reaches the same
result for every maker without one; **exiftool's approach**, which rebuilds and then fixes absolute offsets with a
heuristic shift and a warning --- the heuristic is what fails on unknown makers; **dropping MakerNotes always** ---
the earlier draft, rejected by the project owner.

The patcher is a general primitive ("change these tags in an EXIF block without moving anything") with no app policy
in it, so it goes into `rust-sak`'s `image` module in slice 2; which tags to change, and to what, stays in `opai`.

### D12 --- Thumbnail and IPTC-IIM to XMP (decided)

- **Thumbnail:** dropped and zeroed, not regenerated (D6, D11). Regenerating means a second encode and growing the
  block, which competes with keeping the MakerNote within the APP1 limit; every viewer generates its own from the
  pixels.
- **IPTC-IIM to XMP:** mapped in slice 3, because metadata is always kept. Where the output cannot hold IIM (PNG,
  WebP, AVIF, HEIF) and the source's XMP lacks a property, the IPTC Core datasets are mapped to XMP by the IPTC Photo
  Metadata Standard's mapping table (By-line to `dc:creator`, Caption-Abstract to `dc:description`, Keywords to
  `dc:subject`, CopyrightNotice to `dc:rights`, ObjectName to `dc:title`, Headline, City, Province-State, Country,
  Credit, Source, Instructions, DateCreated). An XMP value already present wins (the MWG rule). Into JPEG the IIM
  block is also copied as is.

## How the work divides across `opai`, `gui` and `cli`

- **`rust-sak`** (separate repository): reading and splicing raw blocks, per format, on the seam shared with the
  colour series. No policy.
- **`opai`**: everything else, in every slice --- carrying metadata on `Picture` (this slice), the EXIF and XMP
  rewrite policy and the embed in `export::write` (slices 2 and 3). Both front ends export through `export::write`, so
  neither gains metadata logic.
- **`gui`**: in this slice, at most the constructor swap in `images/crop.rs` (if the colour change has not made it)
  and the rendition in `images/serve.rs` encoding through `opai`'s mask (D4). Nothing in later slices: there is no
  setting and no UI.
- **`cli`**: nothing in any slice; there is no flag.

## Difficulty and likely paths

**Overall: moderate**, about 70 tasks across six slices; this slice is easy-to-moderate (15 tasks, roughly 2--3 days).

| Part | Difficulty | Effort | Main risk |
|---|---|---|---|
| Read/splice JPEG, PNG, WebP blocks (slice 1) | easy--moderate | 2--3 days | extended XMP reassembly; WebP `VP8X` promotion when the encoder wrote a simple `VP8 ` file |
| `Picture` carrying metadata (slice 1) | easy | <1 day | missing a derived-picture site, or landing out of order with the colour change; mitigated by `derive`, a test per site, and the land-first-creates rule |
| Offset-preserving EXIF patcher and EXIF policy (slice 2) | moderate--hard | 4--5 days, two repositories | getting every IFD walk bounds-checked; makers that point outside their declared count; the APP1 size limit |
| XMP edit with `quick-xml` and IIM mapping (slice 3) | moderate | 3 days | RDF has several equivalent serialisations (attribute vs element form, `rdf:Seq`), so edits must work on both; fallback is `xmp_toolkit` |
| TIFF (slice 4) | moderate--hard | 3--4 days | replacing `image`'s TIFF encoder path with the `tiff` crate's directory encoder without regressing 16-bit output |
| AVIF/HEIF (slice 5) | moderate--hard | 4--5 days, three repositories | C API plumbing in `heif-rs`/`avif-rs`, cross-platform builds of those, three coordinated releases |
| RAW sources and sidecars (slice 6) | moderate | 3--4 days | CR3/RAF containers; matching sidecar naming conventions |

**Recommended path:** `img-parts` for container surgery in `rust-sak` (this slice), an append-only EXIF patcher in
`rust-sak` with `kamadak-exif` only for parsing and the fallback rebuild (slice 2), `quick-xml` (already in the lock
file at 0.42.0, as are `chrono` and `uuid`) for XMP in `opai` (slice 3), first-party codec crates extended for
AVIF/HEIF (slice 5). All pure Rust except what the HEIF/AVIF codecs already link.

**Alternative path:** `xmp_toolkit` for everything XMP --- more correct RDF handling and Adobe's own history
semantics, at the price of a C++ build on all three platforms. Worth taking only if slice 3's XMP edits prove fragile
against real Lightroom/Photos/digiKam files.

**Upstream acceptability:** high. It fixes a visible defect (exports lose their dates and locations, so they sort and
map wrongly), follows the `rust-sak`-first rule, adds one small pure-Rust crate to `rust-sak` and none here in this
slice, and stays inside `opai` for policy. The contestable choice is that there is no off switch: upstream may want
one for privacy (GPS). It would be a small later change in `opai` plus a control and a flag; photo tools (Lightroom,
Photos, Topaz Photo AI itself) keep metadata by default.

## Risks / Trade-offs

- **Metadata stays in memory per picture** (an XMP from Lightroom with a long history can reach a few hundred KB) -->
  shared by `Arc` across every derived picture, so it is held once per source, not per operation.
- **Privacy: GPS is always kept** --> by decision; an export has exactly the exposure the original file had, and a
  user who wants it gone strips it with another tool. Recorded so upstream can weigh it.
- **A MakerNote is dropped in the unsafe cases** --> logged per file, so the loss is visible; standard EXIF keeps lens
  and serial.
- **`img-parts` 0.4.1 is five days old** --> pin exactly; its 0.3 line has years of use, and the surface used
  (segment/chunk lists) is stable across both.
- **Buffering the encode for formats with metadata** --> one encoded file in memory, small beside the decoded picture.
- **Malformed source metadata** --> dropped per block, never fatal; the spec requires the load to succeed.

## Platforms

Designed for macOS, Windows and Linux alike; the container code is pure Rust with no platform code. Validated on macOS
(arm64) only: `rust-sak` and workspace test suites, and a real export checked with ExifTool. Windows and Linux are
untested until CI runs. No execution provider is involved.

## Migration Plan

None: nothing is written differently after this slice. Rollback is reverting the pin and the `Picture` change.

## Open Questions

Decided by the project owner: always keep all metadata with no setting; keep MakerNotes when safe (D11); Software is
`Open Photo AI <version> (<YYYY-MM-DD>)` with the original kept in history and CreatorTool; thumbnail dropped and IIM
mapped to XMP (D12). Remaining:

- **Orientation on decode:** whether a separate change should apply EXIF Orientation on load (so the GUI stops showing
  Orientation-6 JPEGs sideways); if it does, slice 2 must write Orientation 1 for those sources too (D7).
- **Seam names:** resolved. `add-color-1-icc-profile` landed first with `Metadata`, `Embedded`,
  `encode_file_with`/`encode_writer_with`, `Picture.metadata`, `Picture::with_metadata`, `Picture::derive` and
  `Picture::shared_metadata`, exactly as D2, D4 and D5 name them; this change only added fields and `metadata_bytes`.

## Implementation notes and deviations

Recorded at apply time (2026-10-22).

- **No `img-parts` (D3 reversed).** The JPEG segment walk, PNG chunk walk and the WebP RIFF rewrite are written in
  `rust-sak`'s `src/image/blocks/` and `riff.rs` (about 450 lines with the XMP text handling). Reasons: `img-parts`
  reads from an owned `Bytes`, so reading a loaded photograph's metadata would copy the whole file; the colour change
  had already hand-written the WebP RIFF rewrite, which D2 said both changes should share, and it now writes `ICCP`,
  `EXIF` and `XMP ` in one pass (`riff::with_chunks`); and a walker written against `&[u8]` keeps every bound in view
  for the hostile-input review. `rust-sak` gained three direct dependencies, none new to its lock file: `crc32fast`
  (PNG chunk CRCs) and `miniz_oxide` (inflating a compressed XMP `iTXt`, bounded), both already there through the
  `png` codec, and `md5` (the extended-XMP GUID, which the XMP specification defines as an MD5), already there through
  `rawler`.
- **Extended XMP.** On reading, the GUID named by the standard packet's `xmpNote:HasExtendedXMP` is read once, the
  chunks with that GUID must tile the declared length exactly, and the extended packet's `rdf:RDF` contents are moved
  into the standard packet as text, with the stale reference (and its `rdf:Description`, if left empty) removed. An
  incomplete set leaves the standard packet alone. On writing, a packet over 65,502 bytes becomes a standard packet
  holding only the reference plus the whole `rdf:RDF` as extended XMP. Reading that back yields the same `rdf:RDF`
  contents in a fresh wrapper, and further round trips are byte-identical. Slice 3, which edits XMP, may choose to
  keep the most important properties in the standard part, as Adobe's writer does.
- **What "malformed" means.** An EXIF block must have a TIFF header and an IFD0 whose entries lie inside it; XMP must
  be UTF-8; a Photoshop block must start `8BIM`; a PNG metadata chunk must pass its CRC. Every block is capped at 16
  MiB. A malformed block is `None` alone. `metadata_bytes` is infallible and returns `Metadata` (not `Result`).
- **Release.** `rust-sak` commit `feat(image): Read and splice EXIF, XMP and IPTC blocks, with metadata_bytes`
  (`0ed9453d`) on bookmark `metadata` of `reagle/rust-sak`, annotated fork tag `26.10.6-meta.1`
  (`<upstream base>-meta.<n>`, stacked on `26.10.6-color.1`); crate version stays `26.10.6`; recorded in
  `src/image/README.md`. Adopted by moving the workspace `[patch]` entry; the four crates' pins are unchanged.
- **The mask** is `opai::image::without_descriptive(&Metadata) -> Metadata`, used by `export::write` and the GUI
  rendition in `serve.rs`. `image::save`/`encode` themselves embed whatever they are handed.
- **Derived-picture sites.** The four this design listed beyond the colour change's (`inference/execute.rs`,
  `models/upscale/osaka/live.rs`, `models/colorization/live.rs`, `live_support.rs`) are all `cfg(test)`; the three
  that build a derived picture were moved to `derive` (`execute.rs`'s builds a picture from nothing and stays `new`),
  as was the GUI export test's fake enhancer. The production sites (`process/mod.rs`, `gui` `crop.rs`, `perf`
  `sweep.rs`) were already on `derive`; their tests now carry EXIF/XMP/IPTC as well as the profile.
- **Fixtures.** `rust-sak`'s are files written by ExifTool (committed under `src/image/blocks/fixtures/`, with the
  ExifTool arguments). `opai`'s are generated in `image/test_support.rs` (`EXIF` with Orientation 6, `XMP`, `IPTC`,
  `described()`, `written_described(format)`), following that crate's rule of generating rather than committing.
- **Manual check.** The Sony ILCE-6700/Lightroom photograph exported with `-q sd --sh` keeps its Display P3 profile
  and carries no EXIF, XMP or IPTC, as this slice specifies; `exiftool -validate` reports OK. (The source has no
  MakerNote: Lightroom's export dropped it.)
- **After review.** The extended-XMP join dropped namespaces the extended packet declared outside its descriptions;
  fixed in `rust-sak` `26.10.6-meta.3`, adopted in slice 2 (this slice writes no XMP, so its pin stays `meta.1`).
