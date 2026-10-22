## Context

See proposal.md --- Why. After slice 2:

- `rust-sak` (`26.10.6-meta.2`) reads a file's XMP as one packet (a JPEG's extended XMP joined in) and writes one into
  JPEG (re-split into standard and extended XMP above 65,502 bytes), PNG and WebP, without interpreting it.
- `opai::export::metadata::for_export` builds what an export embeds: the EXIF rewritten (`exif.rs`), the IPTC block
  filtered (`iptc.rs`), the ICC profile as it was, and the XMP withheld (from `image::without_descriptive`).
- Exports know the picture's source path (so its format and whether it is RAW), its output size and format, and the
  local time; they do not know which enhancements produced the pixels.

The series' decisions this slice implements are slice 1's D6 (the XMP rows of the rewrite table) and D12 (IIM to XMP).

## Goals / Non-Goals

**Goals:**

- Every XMP property of the source reaches a JPEG, PNG or WebP export, except those that would be false of it, which
  are rewritten or dropped as below; untouched parts of the packet are written exactly as they were.
- The export records Open Photo AI in the history, keeping what was there.
- A PNG or WebP export keeps the IPTC Core fields of a source that had them only in IIM.

**Non-Goals:**

- A general XMP library: only what the policy needs (find, read, set, append, remove properties of the top-level
  `rdf:Description`s). Qualifiers, `rdf:value` forms and nested descriptions are preserved but never edited.
- Naming the enhancements in the history event (Open Questions).
- Keeping IIM and XMP in sync in a JPEG beyond copying both (the MWG reconciliation rules).

## Slices

The `preserve-metadata` series, in order. Each is its own change. This change is slice **3**.

| # | Change | Delivers | Depends on | Status |
|---|---|---|---|---|
| 1 | `preserve-metadata-1-capture` | `rust-sak`: `exif`/`xmp`/`iptc` on the shared `Metadata`, `metadata_bytes`, and splicing in `encode_*_with` for JPEG/PNG/WebP; fork tag `26.10.6-meta.1`. `opai`: `Picture.metadata` carries the blocks; `Picture::derive` everywhere; exports and renditions mask them. | --- | done (2026-10-22) |
| 2 | `preserve-metadata-2-exif` | `rust-sak`: offset-preserving EXIF patcher, rebuild-without-MakerNote fallback, field reader; fork tag `26.10.6-meta.2`. `opai`: the EXIF half of D6 and the IPTC-IIM resource filter in `export::write` for JPEG, PNG and WebP. XMP still masked. | 1 | done (2026-10-22) |
| 3 | `preserve-metadata-3-xmp` | `opai`: the XMP half of D6 with `quick-xml` --- InstanceID/DocumentID/DerivedFrom, the `xmpMM:History` events, CreatorTool, ModifyDate/MetadataDate, tiff:/exif: mirrors kept in step with slice 2's EXIF, `crs:*` (and gain-map) properties dropped --- and the IPTC-IIM to XMP mapping for outputs that cannot hold IIM (D12). The mask is removed from `export::write`: everything is written. | 2 | **current** (implemented 2026-10-22) |
| 4 | `preserve-metadata-4-tiff` | TIFF read and write: EXIF as IFD tags, XMP as tag 700, IPTC as tag 33723. Needs `rust-sak`'s TIFF encode to go through the `tiff` crate's directory encoder instead of `image`'s `TiffEncoder`. Matters for Preserve, which writes RAW as TIFF. | 3 | pending |
| 5 | `preserve-metadata-5-heif-avif` | AVIF/HEIF read and write of the `Exif` and `mime` (XMP) items. Extends `heif-rs` and `avif-rs`, then `rust-sak`, then adopts. Matters for iPhone HEIC sources. | 3 | pending |
| 6 | `preserve-metadata-6-raw` | RAW sources: EXIF from TIFF-based RAW by parsing the file's own IFDs, CR3 and RAF through their containers, and the XMP sidecar. A RAW's EXIF is rebuilt, so its MakerNote takes D11's relocation rule. | 3, 4 | pending |

There is no setting in the series: metadata, location included, is always kept (decided by the project owner). Slices
4, 5 and 6 are independent of each other after 3 and may be reordered by priority.

## How the work divides across `opai`, `gui` and `cli`

- **`opai`**: everything --- the editor (`export/metadata/xmp/`), the policy, the IIM reader and mapping, and the call
  in `export::write`.
- **`gui`**, **`cli`**: no code. The GUI's preview rendition keeps `image::without_descriptive`.

## `rust-sak` coverage

Nothing new. `rust-sak`'s `image` module carries the packet in and out byte for byte, including the JPEG extended-XMP
split and join, and stays free of XMP semantics (slice 1's D1). Editing XMP is application policy; slice 1 already
placed it here.

## Decisions

### D1 --- A lossless tree over `quick-xml`, not a DOM library and not `xmp_toolkit`

The packet is read with `quick-xml` into a tree of elements, text and other markup in which every text run and
attribute value is kept **as written** (escaped), and decoded only when the policy reads it; new values are escaped
when written. Serialising the tree therefore reproduces every part the policy did not touch byte for byte (attribute
quotes are re-derived: double, or single when the value holds a double quote). Namespaces are resolved by URI from the
`xmlns` declarations in scope, so `xap:` and `xmp:` (or any prefix) are the same schema.

Safety: a packet with a `DOCTYPE` is refused (so no entity can be defined, and none is ever expanded); only the five
predefined entities and character references are decoded; the tree is built iteratively and refuses nesting deeper
than 64 levels; the packet is already bounded at 16 MiB by `rust-sak`. A packet that does not parse is left out of the
export with a warning.

Alternatives: **`xmp_toolkit`** (Adobe's SDK) --- correct and complete, but a C++ build on three platforms, rejected
in slice 1; **`roxmltree`/`xmltree`** --- a second XML crate when `quick-xml` is already in the workspace, and neither
keeps untouched text byte for byte; **text substitution** as `rust-sak`'s extended-XMP join does --- fine for one
fixed shape, too fragile for properties that may be attributes or elements under any prefix.

### D2 --- The XMP rewrite table

Applied in `opai::export::metadata::xmp`, with the clock, the source format and the export format passed in:

| Property | Treatment |
|---|---|
| `xmpMM:DocumentID` | new `xmp.did:<uuid v4>` |
| `xmpMM:InstanceID` | new `xmp.iid:<uuid v4>` |
| `xmpMM:OriginalDocumentID` | kept; set to the source's DocumentID where absent and that exists |
| `xmpMM:DerivedFrom` | replaced by `stRef:documentID`, `stRef:instanceID` and `stRef:originalDocumentID` of the source, those it had |
| `xmpMM:History` | kept; if absent and the source's EXIF names a Software, first a `created` event (`softwareAgent` that Software, `when` the source's EXIF DateTime with its offset where recorded); then one event: `converted` (`parameters` `from <source MIME> to <output MIME>`) or `saved` when the format is unchanged, with `changed` `/`, `softwareAgent` `Open Photo AI <version>`, `when` the export time and the new `instanceID` |
| `xmp:CreatorTool` | kept; where absent, the source's EXIF Software, else the export Software string |
| `xmp:ModifyDate`, `xmp:MetadataDate` | the export time, ISO 8601 with offset |
| `tiff:Software` | replaced (if present) by the export Software string, as EXIF's is |
| `tiff:Orientation` | replaced (if present) by 1 for a RAW source; kept otherwise |
| `tiff:ImageWidth`, `tiff:ImageLength`, `exif:PixelXDimension`, `exif:PixelYDimension` | replaced (if present) by the output size |
| `tiff:DateTime` | replaced (if present) by the export time |
| `crs:*` | dropped: Camera Raw develop settings would be re-applied by Lightroom to the developed pixels |
| `hdrgm:*`, `Container:*`, `Item:*` (Google gain-map container) | dropped: they describe a gain map the export does not carry |
| `xmpNote:HasExtendedXMP` | dropped; `rust-sak` re-splits as needed |
| everything else (`dc:`, `photoshop:` incl. `History`, `Iptc4xmpCore/Ext:`, `xmp:Rating`/`Label`, `lr:`, `exif:` capture properties, `aux:`...) | kept as written |

A property that is replaced is removed wherever it is (attribute or element, any `rdf:Description`) and written as an
element of the first `rdf:Description`, its namespace declared there if needed.

### D3 --- IIM to XMP for PNG and WebP (slice 1's D12)

`iptc.rs` gains a reader for the IIM record inside resource `0x0404` (datasets `0x1C`, record, number, 16-bit length;
extended lengths refused as malformed; UTF-8 when dataset 1:90 says so, otherwise UTF-8 if valid, else Latin-1). For
outputs that cannot hold IIM (PNG, WebP), each IPTC Core dataset whose XMP property is absent is added by the IPTC
Photo Metadata Standard's mapping: ObjectName → `dc:title`; Keywords → `dc:subject`; By-line → `dc:creator`;
By-lineTitle → `photoshop:AuthorsPosition`; Caption-Abstract → `dc:description`; Writer-Editor →
`photoshop:CaptionWriter`; Headline → `photoshop:Headline`; CopyrightNotice → `dc:rights`; Credit →
`photoshop:Credit`; Source → `photoshop:Source`; SpecialInstructions → `photoshop:Instructions`; City →
`photoshop:City`; Sub-location → `Iptc4xmpCore:Location`; Province-State → `photoshop:State`;
Country-PrimaryLocationName → `photoshop:Country`; Country-PrimaryLocationCode → `Iptc4xmpCore:CountryCode`;
OriginalTransmissionReference → `photoshop:TransmissionReference`; DateCreated (+ TimeCreated) →
`photoshop:DateCreated`. A JPEG export keeps the IIM block itself (slice 2) and gets no mapping.

### D4 --- When a packet is created

When the source has no XMP but carries EXIF or IPTC, a minimal packet (`x:xmpmeta`, `rdf:RDF`, one `rdf:Description`)
is created and the table applied, so the history, CreatorTool and the mapped IPTC have a home. A source with no
metadata at all exports with none: there is nothing to keep, and Open Photo AI does not stamp files that carried
nothing.

### D5 --- The packet is written without padding

The rewritten packet keeps the source's `<?xpacket?>` wrapper and its padding as they were (they are untouched text);
a created packet has the wrapper and no padding. Padding exists for in-place editing of a file, which an export is
not; keeping the source's is simply not touching it.

## Risks / Trade-offs

- [An XMP writer's unusual serialisation (`rdf:value`, nested `rdf:Description` as a property value, `rdf:resource`)]
  → never edited, only carried; the tree keeps it byte for byte, and the edits only add and remove top-level
  properties.
- [Two descriptions both declaring a property] → removal acts on every occurrence before one is written.
- [Large Lightroom packets] → Camera Raw settings are most of a Lightroom packet and are dropped, so exports shrink.
- [IIM in a legacy 8-bit character set without 1:90] → decoded as UTF-8 when valid, otherwise Latin-1; Windows-1252
  specifics (curly quotes in 0x80--0x9F) are read as Latin-1 control characters, which is the common reader behaviour.
- [`converted`/`saved` without the enhancement names] → the history says Open Photo AI changed the whole document,
  which is true; which enhancements is the open question below.

## Platforms

Pure Rust in `opai`; no platform code, no execution provider. Validated on macOS (arm64); Windows and Linux untested
until CI runs.

## Migration Plan

No `rust-sak` change. Exports gain XMP; nothing else they write changes. Rollback: revert the `opai` commit.

## Implementation notes and deviations

Recorded at apply time (2026-10-22).

- **Attributes are kept as written, not only values.** Beyond D1, each element keeps its attribute text verbatim
  (whitespace, line breaks and quotes) until one of its attributes is edited, so an untouched element is reproduced
  byte for byte; a test asserts that for a whole ExifTool-style packet.
- **Names are validated.** Fuzzing found that `quick-xml` accepts an attribute such as `xmlns:/ap`, and that writing a
  property under that prefix produced markup that no longer parsed. The tree now refuses any element or attribute name
  that is not an XML qualified name, so such a packet is left out with a warning like any malformed one. The fuzz test
  (every byte of a Lightroom-style packet truncated, and replaced by ten markup-significant values) asserts that every
  rewrite that is written parses again.
- **Structures carry `rdf:parseType="Resource"`**, written on `xmpMM:DerivedFrom` and on each history item, which is
  what makes their fields valid RDF/XML children; a Lightroom history whose items are attribute-form `rdf:li`s is
  appended to with an element-form item, which readers accept side by side (ExifTool reads the merged history).
- **Where the code is.** `opai::export::metadata::xmp` (`tree.rs`, `edit.rs`, `mod.rs` policy, `tests.rs`), the IIM
  reader in `export/metadata/iptc.rs` (`iim`, `Iim`), called from `for_export_with`, which takes the new IDs so tests
  can name them; `for_export` draws them from `uuid` v4. `export::metadata` became `pub(crate)` so the end-to-end test
  in `inference/process/tests/metadata.rs` can read the packet back.
- **Source media type** is taken from the source's name: its format's type, or `image/x-<extension>` for camera RAW.
- **Manual check (macOS, CLI `-q sd --sh`).** The Sony/Lightroom photograph, as JPEG, PNG and WebP: Rating, keywords,
  hierarchy, CreatorTool, aux/exifEX lens fields and every other property identical to the source; DocumentID and
  InstanceID new; DerivedFrom names the source's document, instance and original document; history
  `edited, derived, saved` plus `saved` (JPEG) or `converted` with `from image/jpeg to image/png` (PNG); ModifyDate
  and MetadataDate the export time; all `crs:` and the `hdrgm:Version` gone. A 2013 Canon JPEG with digiKam/ExifTool
  XMP and no history exported as PNG gains `created` (Shotwell 0.15.0, from its EXIF) then `converted`.
  `exiftool -validate -warning` reports nothing about the XMP on any output; the remaining warnings are the sources'
  own.
- **Not done:** naming the enhancements in the history (Open Questions); the `IPTCDigest is not current` warning on
  the Sony and hawk exports is inherited from Lightroom's sources (their stored digest does not match their IIM).


### Fixes after independent review (2026-10-22)

Squashed into this slice's commit; regression tests come from the reviewer's repro files.

- **No panic on RDF as a default namespace.** `<RDF xmlns='…rdf-syntax-ns#'></RDF>` made the editor invent an
  undeclared `rdf:` prefix, then index an empty description list, and the CLI aborted. Every `rdf:` name is now
  written under a prefix declared where needed (on the new description itself if the packet binds none), and the
  description lookup never indexes unchecked. `rdf:Seq`/`rdf:li` likewise.
- **XML-illegal characters.** NUL padding and control characters from a Latin-1 IIM record or an EXIF Software value
  made invalid XML. `encode` now drops every character XML 1.0 forbids; such raw characters in a source packet are
  dropped on the way in; a character reference to one is refused like an undefined entity.
- **Extended XMP namespaces** (`rust-sak` `26.10.6-meta.3`, adopted in slice 2): the join now declares on `rdf:RDF`
  the namespaces the extended packet declared on its `x:xmpmeta`/`rdf:RDF`, refusing on a conflicting prefix or a
  default namespace.
- **Re-checked:** every repro now writes a file, every written packet passes `xmllint`, and the hawk, Canon and Sony
  exports are unchanged in substance.

## Open Questions

- Should the history event's `stEvt:parameters` name the enhancements applied (e.g.
  `sharpen Moscow; upscale Kyoto 2x`)? `export::write` does not know them; both front ends do, so it would be a new
  field the GUI and CLI fill in. Deferrable: the event is true without it.
