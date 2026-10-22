## Context

See proposal.md --- Why. After slice 1:

- `rust-sak` (`26.10.6-meta.1`) reads a file's EXIF (the TIFF stream without JPEG's `Exif\0\0`), XMP and IPTC (the
  Photoshop resource block) with `metadata_bytes`, and `encode_*_with` splices given blocks into JPEG (EXIF at most
  65,527 bytes), PNG and WebP. It checks an EXIF block's header and IFD0 extent and nothing more.
- `opai`'s `Picture.metadata` carries the blocks through every derived picture. `export::write` encodes through
  `image::without_descriptive`, which clears all three; the GUI's preview rendition does the same, permanently.
- Native sources are held in their stored, unrotated frame; a developed RAW is already oriented, and carries no EXIF
  until slice 6.

The series' decisions this slice implements are in slice 1's design.md: D6 (the rewrite table), D7 (orientation), D11
(MakerNotes kept by never moving them) and D12 (thumbnail dropped, not regenerated).

## Goals / Non-Goals

**Goals:**

- Every EXIF field of the source reaches a JPEG, PNG or WebP export, with the ones that would be false of it
  rewritten.
- A MakerNote is kept byte for byte at its offset whenever the patched block fits its container.
- The IPTC-IIM data reaches a JPEG export; the rest of the Photoshop block does not.

**Non-Goals:**

- XMP, including the XMP mirrors (`tiff:`/`exif:`) of the fields rewritten here: slice 3, which lifts the last mask.
- Creating an Exif IFD where the source has none (OffsetTime and PixelX/YDimension are then not added); a minimal EXIF
  without one is rare and stays as it was, apart from IFD0.
- Regenerating a thumbnail (D12), rebuilding a MakerNote for another offset (slice 6), and EXIF in TIFF, AVIF and HEIF
  outputs (slices 4 and 5).

## Slices

The `preserve-metadata` series, in order. Each is its own change. This change is slice **2**.

| # | Change | Delivers | Depends on | Status |
|---|---|---|---|---|
| 1 | `preserve-metadata-1-capture` | `rust-sak`: `exif`/`xmp`/`iptc` on the shared `Metadata`, `metadata_bytes`, and splicing in the shared `encode_*_with` for JPEG/PNG/WebP (hand-written container surgery); fork tag `26.10.6-meta.1`. `opai`: `[patch]` moved; `Picture.metadata` carries the blocks; `Picture::derive` everywhere; exports and renditions mask the blocks out. | --- | done (2026-10-22) |
| 2 | `preserve-metadata-2-exif` | `rust-sak`: an offset-preserving EXIF patcher (D11) --- in-place overwrite of fixed-size values, append-only relocation of grown values and IFDs, thumbnail detach and zeroing --- with the rebuild-without-MakerNote fallback and a field reader; fork tag `26.10.6-meta.2`. `opai`: the EXIF half of D6 and the IPTC-IIM resource filter, written by `export::write` into JPEG, PNG and WebP. XMP still masked. | 1 | **current** (implemented 2026-10-22) |
| 3 | `preserve-metadata-3-xmp` | `opai`: the XMP half of D6 with `quick-xml` --- InstanceID/DocumentID/DerivedFrom, the `xmpMM:History` events, CreatorTool, ModifyDate/MetadataDate, tiff:/exif: mirrors kept in step with slice 2's EXIF, `crs:*` dropped --- and the IPTC-IIM to XMP mapping for outputs that cannot hold IIM (D12). The mask is removed from `export::write`: everything is written. | 2 | implemented 2026-10-22 |
| 4 | `preserve-metadata-4-tiff` | TIFF read and write: EXIF as IFD tags, XMP as tag 700, IPTC as tag 33723. Needs `rust-sak`'s TIFF encode to go through the `tiff` crate's directory encoder instead of `image`'s `TiffEncoder`, which has no custom-tag hook. Matters for Preserve, which writes RAW as TIFF. | 3 | pending |
| 5 | `preserve-metadata-5-heif-avif` | AVIF/HEIF read and write of the `Exif` and `mime` (XMP) items. Extends `heif-rs` and `avif-rs`, then `rust-sak`, then adopts. Matters for iPhone HEIC sources. | 3 | pending |
| 6 | `preserve-metadata-6-raw` | RAW sources: EXIF from TIFF-based RAW by parsing the file's own IFDs, CR3 and RAF through their containers, and the XMP sidecar. A RAW's EXIF has to be rebuilt rather than patched, so its MakerNote takes D11's relocation rule; this slice's Orientation rule (1 for RAW) is already in place for it. | 3, 4 | pending |

There is no setting in the series: metadata, location included, is always kept (decided by the project owner). Slices
4, 5 and 6 are independent of each other after 3 and may be reordered by priority.

## How the work divides across `opai`, `gui` and `cli`

- **`rust-sak`**: the EXIF block primitives --- parse its IFDs with every bound checked, read one field, patch fields
  without moving bytes, rebuild compactly without the MakerNote. No policy: it is told which tags to set and to what.
- **`opai`**: the policy --- which tags, what values, the clock, the fallback and its warning, the IPTC resource
  filter --- in a new `export/metadata/` module that `export::write` calls. Both front ends export through it.
- **`gui`**, **`cli`**: no code. The GUI rendition keeps `without_descriptive`. `docs/CLI_REFERENCE.md` documents what
  an output keeps.

## `rust-sak` coverage

Uses the `image` feature only. What it has after slice 1: block read and splice, no EXIF interpretation. Filled by
**extending `rust-sak`** (D1 of slice 1: the patcher is a general primitive), in one new module `src/image/exif/`:

- `exif_field(exif, ExifIfd, tag) -> Option<ExifValue>` --- one value, for `ASCII`, `SHORT` and `LONG` (the types the
  policy reads and the tests check); other types read as `None`.
- `patch_exif(exif, &[ExifEdit]) -> Result<Vec<u8>, ExifError>` and
  `rebuild_exif(exif, &[ExifEdit]) -> Result<Rebuilt, ExifError>`, where an `ExifEdit` sets (adding if absent) or
  replaces (only if present) one tag in IFD0, the Exif IFD, the GPS IFD or the Interop IFD, or drops the thumbnail;
  `Rebuilt` says whether a MakerNote was left out. All of it `#[non_exhaustive]` for growth.

No dependency is added to `rust-sak`.

## Decisions

### D1 --- Patch by an in-memory model of the IFDs, written back over the original

`patch_exif` parses the header, IFD0, the Exif, GPS and Interop IFDs (through tags `0x8769`, `0x8825`, `0xA005`) and
IFD1 into a list of entries per IFD, each IFD table's own extent checked against the block, and every entry's
out-of-line value range computed with overflow checks. Edits change the model; then each changed IFD is written back:
**in place** when its entry count is unchanged, or **copied to newly allocated space** when it gained an entry, with
its parent's pointer (the TIFF header for IFD0, the pointer entry otherwise) updated --- children before parents, so
one pass suffices. A value that fits in four bytes is written into its entry (a `SHORT` dimension may become `LONG` in
the same slot); a longer value is overwritten in place if it is no longer than the old one, otherwise allocated and
repointed. Every byte freed (an old value, an old IFD table, IFD1 and its thumbnail) is zeroed, **unless another
entry's value overlaps it** --- then it is left alone, since something still points there.

Allocation takes the first freed range that fits (D11.4 of slice 1), aligned to two bytes, and otherwise appends.
Freed ranges that reach the end of the block are trimmed off **before** allocating, so a thumbnail stored last ---
Lightroom and most cameras store it last --- shrinks the block instead of leaving kilobytes of zeros.

Alternatives: **rebuild always** (`kamadak-exif`'s writer, or this module's own) --- simpler, but it moves every value
and so corrupts Canon, Sony, Pentax and Samsung MakerNotes, whose pointers are absolute; **byte-level patching without
a model** --- fewer allocations but every edit would re-walk the IFDs and the child-before-parent ordering would be
implicit.

### D2 --- The fallback is this module's own compact rebuild, not `kamadak-exif`

When the patched block will not fit (in practice: a JPEG whose EXIF was within a few dozen bytes of the 65,527-byte
limit and had no thumbnail), `rebuild_exif` writes a fresh stream from the same model: header, IFD0, Exif, GPS and
Interop IFDs each followed by their values, IFD1 and the MakerNote left out, an entry with an unknown type or an
out-of-range value dropped. Slice 1's D11 named `kamadak-exif`'s `experimental::Writer` for this; the model D1 needs
already holds every entry's type, count and bytes, so serialising it is about a hundred lines, keeps unknown and
private tags `kamadak-exif`'s typed writer would refuse, and adds no dependency.

### D3 --- Policy: the tags `opai` sets

Applied in `opai::export::metadata::exif`, with the clock and the source kind passed in so tests are deterministic:

| Tag | IFD | Edit |
|---|---|---|
| Software `0x0131` | IFD0 | set `Open Photo AI <CARGO_PKG_VERSION> (<local YYYY-MM-DD>)` |
| DateTime `0x0132` | IFD0 | set export local time `YYYY:MM:DD HH:MM:SS` |
| OffsetTime `0x9010` | Exif | replace with the export's local offset `±HH:MM`, only where the source records one (see Implementation notes) |
| PixelXDimension `0xA002`, PixelYDimension `0xA003` | Exif | set output width/height, `SHORT` when it fits else `LONG` (only where an Exif IFD exists) |
| ImageWidth `0x0100`, ImageLength `0x0101` | IFD0 | replace only if present |
| Orientation `0x0112` | IFD0 | kept; replaced by 1 when the source is camera RAW (D7 of slice 1) |
| IFD1 and its thumbnail | --- | dropped and zeroed (D12 of slice 1) |
| everything else (GPS, MakerNote, ColorSpace, InteropIndex, ...) | --- | kept byte for byte |

The original Software and DateTime are not lost: they are what slice 3 writes into `xmp:CreatorTool` and the XMP
history, read with `exif_field` from the carried source block.

### D4 --- When the MakerNote is dropped, and how that is said

`opai` patches; if the patched block exceeds the target's EXIF limit (JPEG only), it rebuilds and logs
`warn!(path, maker, reason = "too large for JPEG APP1", "MakerNote dropped")` when the rebuild reports a MakerNote
left out; if the source block does not parse (`ExifError::Malformed`), the EXIF is left out entirely with a warning,
since no rewrite of it can be trusted to be true. The JPEG limit is `rust-sak`'s (`MAX_JPEG_EXIF_LEN`, exported), so
the two cannot disagree.

### D5 --- The IPTC filter keeps resources `0x0404` and `0x0425` only

The Photoshop block is a list of `8BIM` resources (signature, ID, padded Pascal name, size, padded data). `opai` walks
it with the same bounds discipline and keeps the IPTC-IIM data (`0x0404`) and its MD5 digest (`0x0425`), which still
match because the IIM is unchanged. Everything else --- the Photoshop thumbnail `0x040C`, resolution info, slices,
paths --- is dropped (D6 of slice 1). A block that does not walk, or keeps nothing, is not written. This is policy
(which resources an export should carry) and so stays in `opai`, not `rust-sak`.

### D6 --- Formats that cannot hold a block say so

`image::save` already warns when an ICC profile is not embedded. It now does the same for EXIF, XMP and IPTC handed to
it but not embedded (BMP, GIF, TIFF, AVIF, HEIF; IPTC in PNG and WebP), one warning per file naming the format and the
blocks. `export::write` hands it only what policy would write, so the withheld XMP does not warn in this slice.

### D7 --- `chrono` for the local time

The export time must be local with its UTC offset (`OffsetTime` exists to say which). `chrono` (MIT/Apache-2.0) is
already compiled into this crate for `tracing-subscriber`'s timestamps; it becomes a direct dependency with only its
`clock` feature. Alternatives: `time` refuses to read the local offset in a multi-threaded process on Unix, which
every `opai` process is; `jiff` is also in the lock file but would be a third date library for one call.

## Risks / Trade-offs

- [A MakerNote whose internal pointers reach outside its declared extent, into the freed thumbnail] → freed bytes are
  zeroed only where no parsed entry overlaps them; a pointer the parser cannot see is the residual risk, which every
  tool that edits EXIF shares, and is noted in the `rust-sak` README.
- [Freed but untrimmed space] → a thumbnail not at the end of the block leaves zeros behind; viewers ignore them.
- [Sony and Canon EXIF over 64 KiB] → straight-from-camera JPEGs keep it under the limit by construction; a block that
  grows past it takes the logged rebuild.
- [Local time zone is wrong on the machine] → DateTime/OffsetTime say what the machine believed, as every editor's do.
- [IPTC lost from PNG/WebP exports] → logged; slice 3 maps it to XMP.

## Platforms

Pure Rust in `rust-sak` and `opai`; no platform code, no execution provider. Validated on macOS (arm64); Windows and
Linux untested until CI runs.

## Migration Plan

Additive in `rust-sak` (fork tag `26.10.6-meta.2`, crate version unchanged); `opai` moves its `[patch]`. Exports gain
EXIF and IPTC; nothing that was written before changes otherwise. Rollback: revert the `opai` commit; files already
written simply carry true metadata.

## Implementation notes and deviations

Recorded at apply time (2026-10-22).

- **OffsetTime is replaced, never added** (D3 changed). Adding it to a source that lacks it is, for an EXIF older than
  2.31, an error ExifTool's validator reports ("OffsetTime requires ExifVersion 0231"), seen on a 2013 Canon JPEG. A
  source that records offsets has OffsetTime and gets the export's; one that does not keeps an offset-less DateTime,
  as its own dates are.
- **`rust-sak` release.** Commit
  `feat(image): Patch EXIF fields without moving bytes, with a rebuild fallback and a field reader` (`c3f962f7`) on
  bookmark `metadata` of `reagle/rust-sak`, annotated fork tag `26.10.6-meta.2`; crate version unchanged; documented
  in `src/image/README.md` ("Editing EXIF"). Public surface: `exif_field`, `patch_exif`, `rebuild_exif`, `ExifEdit`
  (`Set`, `Replace`, `DropThumbnail`), `ExifIfd`, `ExifValue`, `ExifError` (`Malformed`, `PointerTag`), `Rebuilt`,
  `MAX_JPEG_EXIF_LEN`.
- **Patcher details beyond D1.** Freed ranges are merged before allocation (two edited entries can release the same
  bytes); an IFD at its 65,535-entry limit gains nothing; an entry of a type whose size is unknown turns off zeroing
  and trimming for the whole block (its extent cannot be seen); trimming cuts trailing zero bytes back to the last
  byte any parsed entry or table references. A sub-IFD or IFD1 whose pointer leads outside the block, or onto another
  table, is treated as absent, and the rebuild drops its dangling pointer.
- **Where the policy lives.** `opai::export::metadata` (`mod.rs` `for_export`, `exif.rs`, `iptc.rs`), called by
  `export::write` with `chrono::Local::now()`; `image::save`/`encode` log `metadata not embedded` naming the format
  and the blocks (`image/descriptive.rs`).
- **`Cargo.lock`.** No package added; `chrono` becomes a direct dependency of `opai` (it was compiled already).
- **Manual check (macOS, CLI, `-q sd --sh`, and one `--up kyoto`).** The Sony/Lightroom photograph: Software,
  ModifyDate and ExifImageWidth/Height rewritten (2394 × 2992 after the upscale), IFD1 and its 12 KB thumbnail gone
  (EXIF block 13,116 → about 1,500 bytes), the Photoshop thumbnail and resolution resources gone, IPTC keywords and
  digest unchanged, every other EXIF field identical in JPEG, PNG and WebP; `exiftool -validate` warnings are a subset
  of the source's (the export no longer lacks ExifImageWidth/Height). A 2013 Canon PowerShot JPEG (88 MakerNote tags)
  and a 2024 Sony ILCE-6700 JPEG (Lightroom-exported with its MakerNote) decode to identical MakerNote values after
  export. The photograph's XMP is not written, as specified. **Found:** it is an Ultra HDR JPEG (MPF gain map, `hdrgm`
  XMP); the gain map is lost on export, which predates this series and is `add-color-6-ultra-hdr`'s.


### Fixes after independent review (2026-10-22)

Squashed into this slice's commit; regression tests come from the reviewer's repro files.

- **`rust-sak` `26.10.6-meta.3`** (commit `a27115f5`, a new commit on `metadata`; earlier tags unmoved). The patcher
  no longer trims trailing zeros past the last byte it models, nor reuses freed ranges in the middle of the block: it
  trims only bytes it freed that reach the end, and appends past the end. A SubIFD (`0x014A`) ending the block was
  overwritten before. A dropped thumbnail is zeroed even when an unknown entry type turns off other zeroing, and when
  it cannot be located exactly the patch returns `ExifError::ThumbnailUnremovable`. A value claiming an offset inside
  the TIFF header is treated as no value. This reverses D1's "reuse freed space first" (slice 1's D11.4): the JPEG
  limit is still met by trimming a trailing thumbnail, and otherwise by the rebuild.
- **`opai`.** `ThumbnailUnremovable` takes the rebuild path (MakerNote dropped, warning with reason "the thumbnail
  could not be removed in place"). SubSecTime (`0x9290`) is replaced, where the source records one, with the export
  time's milliseconds, in step with DateTime.
- **Why the `[patch]` moves to `meta.3` here and not in slice 1.** This slice's behaviour depends on the patcher
  fixes. The other `meta.3` fix, the extended-XMP join, changes only the XMP packet slice 1 carries, which no output
  writes until slice 3, so slice 1 keeps `meta.1` with no observable difference.
- **Re-checked** on the review's repros and nine real JPEGs (hawk, two Canon, a Fujifilm, Panasonic, Samsung, Apple
  and two Sony): MakerNotes decode identically, no thumbnail survives, and no new validation warning appears.

## Open Questions

- Should the GUI say in the export dialog that metadata is kept (it is not optional)? Deferrable: nothing in the
  behaviour depends on it.
