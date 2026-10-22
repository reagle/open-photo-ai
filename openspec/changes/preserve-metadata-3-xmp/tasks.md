## 1. The XMP editor (`opai`)

- [x] 1.1 Add `quick-xml` and `uuid` (`v4`) to `opai` (design.md --- proposal Impact); verify `Cargo.lock` gains no package and `cargo build -p opai` is green
- [x] 1.2 Add `export/metadata/xmp/tree.rs`: parse a packet into a tree keeping text and attribute values as written, refuse `DOCTYPE`, unknown entities and nesting over 64, and serialise it back (design.md --- D1); test that exiftool's, Lightroom-style and attribute-form packets round-trip byte for byte, and that truncations, byte flips and an entity-expansion attempt never panic and are refused; verify `cargo test -p opai xmp::tree` is green
- [x] 1.3 Add `export/metadata/xmp/edit.rs`: find the top-level descriptions, resolve prefixes by namespace URI, read a simple property (attribute or element, decoded), remove a property or a whole namespace everywhere, set a simple property, replace a structure, append to an `rdf:Seq`/`rdf:Bag` array, and set an `rdf:Alt`; test each in both RDF forms and with a non-standard prefix; verify `cargo test -p opai xmp::edit` is green

## 2. The policy (`opai`)

- [x] 2.1 Add the identity and date rows of design.md --- D2 (DocumentID, InstanceID, OriginalDocumentID, DerivedFrom, CreatorTool, ModifyDate, MetadataDate) with an injected clock and ids; test a Lightroom-style packet and one with none of them; verify `cargo test -p opai xmp` is green
- [x] 2.2 Add the history rows: keep events, add `created` from the source's EXIF Software and DateTime where there was no history, append `converted` (with MIME types) or `saved`; test all three cases and that the new event's instanceID matches the new InstanceID; verify `cargo test -p opai xmp` is green
- [x] 2.3 Add the mirror and drop rows: `tiff:`/`exif:` mirrors in step with slice 2's EXIF (RAW Orientation 1, sizes, Software, DateTime) and `crs:`, `hdrgm:`, `Container:`, `Item:` and `xmpNote:HasExtendedXMP` dropped; test with the exiftool fixture and a Lightroom-style packet; verify `cargo test -p opai xmp` is green
- [x] 2.4 Add the IIM reader to `iptc.rs` and the IIM-to-XMP mapping of design.md --- D3 for PNG and WebP, an XMP value winning; test UTF-8 and Latin-1 records, repeated keywords, date and time, and malformed records never panicking; verify `cargo test -p opai export::metadata` is green
- [x] 2.5 Create a minimal packet when the source has EXIF or IPTC but no XMP, write none for a source with no metadata, and leave out a malformed packet with a warning (design.md --- D4); test the three cases (the warning through `test_log`); verify `cargo test -p opai export::metadata` is green

## 3. Export and checks

- [x] 3.1 Make `for_export` write the rewritten XMP (source format from the picture's path), lifting the last mask from `export::write` while the GUI rendition keeps it; extend the end-to-end test to JPEG, PNG and WebP with XMP (Lightroom-style history, Camera Raw settings, keywords) and with an IPTC-only source to PNG; verify `cargo test -p opai` and `cargo test -p gui images::serve` are green
- [x] 3.2 Update `docs/CLI_REFERENCE.md` (Metadata) and `opai`'s `image` and `export` module docs to say XMP is written and how; verify `cargo test -p cli every_example_in_the_reference_parses` and `cargo doc -p opai --no-deps` are green
- [x] 3.3 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; export the Sony/Lightroom photograph with the CLI as JPEG, PNG and WebP and compare `exiftool -G1 -a -s -XMP:all` with the source, and run `exiftool -validate -warning`; verify `openspec validate preserve-metadata-3-xmp --strict` passes
