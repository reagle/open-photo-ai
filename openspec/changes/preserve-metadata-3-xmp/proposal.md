> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

After `preserve-metadata-2-exif`, an export carries the source's EXIF and IPTC, but still withholds its XMP: written
unchanged it would keep the source's document identity, Camera Raw develop settings that Lightroom would re-apply to
an already-developed file, gain-map descriptions of a gain map that is no longer there, and dimensions and dates that
are no longer true. So the rating, label, Lightroom keywords and hierarchy, the IPTC Core and Extension fields that
only XMP holds, and the edit history are still lost, and nothing records that Open Photo AI processed the file. A PNG
or WebP export also loses the source's IPTC keywords and caption entirely, since those formats cannot hold IPTC-IIM.
This slice writes the XMP, rewritten so that every property is true of the exported file.

## What Changes

Third slice of the `preserve-metadata` series (six slices, about 70 tasks; see design.md --- Slices). Estimate for
this slice: **12 tasks**. No setting and no flag: metadata is always kept (project owner's decision).

**In this repository, all in `opai`:**

- **A small XMP editor** over `quick-xml`: the packet parsed into a tree that keeps every untouched byte as written,
  properties found by namespace URI in both RDF forms (attribute and element), and simple, structured and array
  properties set, appended to and removed. No DTD, no entity expansion, bounded depth.
- **The XMP rewrite policy** (slice 1's D6), applied by `export::write`: new `xmpMM:DocumentID` and `InstanceID`;
  `xmpMM:DerivedFrom` naming the source; `OriginalDocumentID` kept or set; `xmpMM:History` kept, given a `created`
  event for the source's original software where it had no history, and a `converted` or `saved` event for Open Photo
  AI; `xmp:CreatorTool` kept or set to the source's original software; `xmp:ModifyDate` and `MetadataDate` the export
  time; the `tiff:`/`exif:` mirrors of the EXIF fields slice 2 rewrites kept in step; Camera Raw settings (`crs:`),
  gain-map descriptions (`hdrgm:`, Google `Container:`/`Item:`) and the stale extended-XMP reference dropped.
- **IPTC-IIM mapped to XMP** for PNG and WebP exports, by the IPTC Photo Metadata Standard's table, where the XMP
  lacks the property (an XMP value already present wins).
- **A new XMP packet** when the source had none but carried EXIF or IPTC, so the history and the mapped IPTC have a
  home. A source with no metadata at all still exports with none.
- **The last mask is lifted** from `export::write`: JPEG, PNG and WebP exports carry EXIF, XMP and (JPEG) IPTC.
  Previews keep the mask for good.
- **One new direct dependency each, `quick-xml` and `uuid`**, both already in `Cargo.lock` through Tauri.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `image-io`: an export writes the source's XMP, rewritten to be true of the result and recording the processing; a
  PNG or WebP export carries the source's IPTC through XMP; a preview still carries no descriptive metadata.

## Impact

**Crates touched:** `opai` only (`export/metadata/xmp/` and `iptc.rs`, `export/write.rs`, manifest). `gui` and `cli`
have no code change: both export through `opai::export::write`. `docs/CLI_REFERENCE.md` says what a result keeps.

**`rust-sak`:** checked first. It reads and splices XMP packets byte for byte (slice 1), re-splitting a large one into
JPEG extended XMP, and does not interpret them. Editing XMP is this application's export policy, and slice 1's design
placed it in `opai` with `quick-xml`; nothing is added to `rust-sak`, so there is no release in this slice.

**Third-party crates introduced:** `quick-xml` 0.42 (MIT) and `uuid` 1.26 with `v4` (MIT/Apache-2.0) as direct
dependencies of `opai`; both are already compiled into the workspace (`plist`, `infer` through Tauri), so the lock
file gains no package. Slice 1's alternative, Adobe's `xmp_toolkit`, stays rejected for its C++ build.

**Not in scope:** TIFF, AVIF/HEIF and RAW sources and outputs (slices 4--6), naming the enhancements applied in the
history event (see design.md --- Open Questions), and IPTC Extension fields that have no IIM counterpart.
