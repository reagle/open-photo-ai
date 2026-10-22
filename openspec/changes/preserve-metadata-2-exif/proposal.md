> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

After `preserve-metadata-1-capture`, a loaded photograph carries its EXIF and IPTC to the export, but the export still
withholds them: written unchanged they would state the source's pixel dimensions, keep a thumbnail of the uncropped
original and name another program as the software. So an exported JPEG still loses its capture date and offset, its
camera, lens and exposure, its GPS position, its keywords and its copyright. This slice writes the EXIF and IPTC,
rewritten so that every field is true of the exported file, and keeps the camera's MakerNote where that is safe.

## What Changes

Second slice of the `preserve-metadata` series (six slices, about 70 tasks; see design.md --- Slices). Estimate for
this slice: **14 tasks**. There is no setting and no flag: per the project owner's decision, metadata --- location
included --- is always kept.

**In `../rust-sak` (a separate repository), on the existing `image` feature:**

- **An offset-preserving EXIF patcher** (`patch_exif`): changes given tags of an EXIF block without moving a single
  byte that is kept --- values that fit are overwritten in place, a value that grows is appended and its entry
  repointed, an IFD that gains an entry is copied to the end with its parent pointer updated, and the IFD1 thumbnail
  is detached and zeroed, its space reused first and trimmed when it ends the block. A MakerNote, whose internal
  pointers may be absolute from the start of the TIFF stream, therefore stays valid. (Design D11 of slice 1.)
- **A rebuild fallback** (`rebuild_exif`): a fresh, compact EXIF block with the same edits, without IFD1 and without
  the MakerNote, for the case the patched block would not fit its container.
- **A reader** (`exif_field`) for single EXIF values, which the policy and slice 3 need (the source's Software and
  DateTime) and the tests use.
- **A release**: fork tag `26.10.6-meta.2`, under the go-ahead delegated for this series.

**In this repository, all in `opai`:**

- **The EXIF rewrite policy** (design D6 of slice 1), applied by `export::write`: Software becomes
  `Open Photo AI <version> (<YYYY-MM-DD>)`; DateTime and OffsetTime become the export's local time and offset;
  PixelX/YDimension (and IFD0 ImageWidth/ImageLength where present) become the output's size; Orientation is kept for
  native sources and set to 1 for camera RAW, whose decoder already applied it; the thumbnail is dropped; the
  MakerNote is kept, or dropped with a warning naming the file, the camera maker and the reason when only a rebuild
  fits.
- **The IPTC filter**: only the IPTC-IIM resource (`0x0404`) and its digest (`0x0425`) of the Photoshop block are
  written, so a stale Photoshop thumbnail and other resources are not.
- **Written into JPEG, PNG and WebP** (EXIF), and JPEG (IPTC). Where the output format cannot hold a block (BMP, GIF,
  TIFF, AVIF and HEIF until later slices, IPTC in PNG and WebP until slice 3 maps it to XMP), the file is still
  written and a warning names the format and the blocks left out.
- **XMP stays withheld** until slice 3; previews stay masked for good.
- **One new direct dependency, `chrono`**, for the export's local time and UTC offset; already in the lock file.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `image-io`: an export writes the source's EXIF, rewritten to be true of the result and with its MakerNote kept when
  safe, and its IPTC-IIM into JPEG; only XMP is still withheld.

## Impact

**Crates touched:** `opai` (`export/` gains the metadata rewrite; `image/save.rs` warns about dropped blocks;
manifest), and the workspace `[patch]` entry. `gui` and `cli` are untouched: both export through
`opai::export::write`, so both gain the behaviour with no code of their own, and the GUI's preview renditions keep the
mask. `docs/CLI_REFERENCE.md` says what an output keeps.

**`rust-sak`:** checked first. After slice 1 it reads and splices EXIF blocks but never looks inside one. The patcher,
the rebuild and the field reader are general-purpose ("change these tags in an EXIF block without moving anything")
and sit beside the readers, so they extend `rust-sak`'s `image` module; which tags to change, and to what, stays in
`opai`.

**Third-party crates introduced:** none new to `Cargo.lock`. `chrono` becomes a direct dependency of `opai` (it is
already compiled for `tracing-subscriber`). `rust-sak` adds no dependency: the EXIF code is its own, which replaces
the `kamadak-exif` writer slice 1's D11 named for the fallback.

**Not in scope:** XMP (slice 3, including the XMP mirrors of the fields rewritten here and IPTC-to-XMP mapping), TIFF
(slice 4), AVIF/HEIF (slice 5), RAW sources (slice 6), and the EXIF ColorSpace/InteropIndex fields, copied as read and
left to the colour series (slice 1's D8).
