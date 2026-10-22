## ADDED Requirements

### Requirement: A loaded image carries its source's metadata

A loaded image SHALL carry the descriptive metadata its source file held, as the blocks the file stored them in: the
EXIF block, the XMP packet and the IPTC block. Each is kept byte for byte as it was read, and each is either present
or absent; none is parsed, edited or reordered on the way in.

For JPEG, PNG and WebP sources, every one of those blocks that the file holds in its format's standard location SHALL
be carried. A JPEG's XMP that is split across extended XMP segments SHALL be carried as one whole packet. For every
other format the image SHALL be loaded exactly as before and SHALL carry no metadata, until a later change adds that
format.

The metadata SHALL be read from the same bytes the pixels and the identity are derived from, so that the file is read
once and all three describe the same file. A metadata block that is malformed SHALL be left out rather than making the
load fail: a file that decodes today SHALL still decode.

The ICC colour profile travels in the same carried metadata but is governed by its own capability; this requirement
neither adds nor removes it.

#### Scenario: A JPEG's metadata is carried
- **WHEN** a JPEG holding an EXIF block, an XMP packet and an IPTC block is loaded
- **THEN** the loaded image carries all three, each byte-identical to the block in the file

#### Scenario: PNG and WebP metadata is carried
- **WHEN** a PNG holding an `eXIf` chunk and an XMP `iTXt` chunk, or a WebP holding `EXIF` and `XMP ` chunks, is
  loaded
- **THEN** the loaded image carries that EXIF block and that XMP packet

#### Scenario: Extended XMP is reassembled
- **WHEN** a JPEG whose XMP is split into a main packet and extended XMP segments is loaded
- **THEN** the loaded image carries the whole XMP, not only the main packet

#### Scenario: A file without metadata carries none
- **WHEN** an image holding no EXIF, XMP or IPTC is loaded
- **THEN** it loads as before and carries no metadata

#### Scenario: Malformed metadata does not stop the load
- **WHEN** a JPEG whose EXIF segment is truncated or corrupt is loaded
- **THEN** its pixels are decoded as before, and that block is absent from what it carries

#### Scenario: A format without a metadata reader yet
- **WHEN** a TIFF, AVIF, HEIF, BMP, GIF or camera RAW file is loaded
- **THEN** it loads exactly as before and carries no metadata

### Requirement: A computed image keeps its source's metadata

An image computed from another --- by an enhancement, a chain of enhancements, a crop, a preview rendition, or a
result restored from the run cache --- SHALL carry the same metadata as the image it was computed from. Metadata SHALL
never be carried onto pixels computed from a different source, and SHALL NOT affect the identity: two images with the
same pixels and identity SHALL be treated as the same image whatever metadata they carry.

Carrying the metadata SHALL NOT copy it: images computed from one source share one copy of it.

#### Scenario: An enhanced image keeps the metadata
- **WHEN** a loaded JPEG carrying EXIF and XMP is enhanced through a chain of operations
- **THEN** the resulting image carries the same EXIF and XMP as the loaded one

#### Scenario: A cached result keeps the metadata
- **WHEN** an enhancement's result is restored from the run cache rather than computed
- **THEN** it carries the metadata of the image it was requested for

#### Scenario: Metadata does not change the identity
- **WHEN** two images with the same pixels and identity carry different metadata
- **THEN** their identities are equal and the run cache treats them as the same image

### Requirement: Carried EXIF, XMP and IPTC are not yet written

Until the export applies the rewrite that keeps written metadata true of the written pixels, an exported file and a
preview rendition SHALL NOT contain the EXIF, XMP or IPTC blocks a picture carries. Withholding them SHALL NOT affect
any colour profile the picture carries, which is written or withheld by its own capability's rules.

A preview rendition SHALL never contain the picture's EXIF block, so that a viewer cannot turn the preview away from
the frame the picture's pixels are in.

#### Scenario: An export does not yet write the carried blocks
- **WHEN** a JPEG carrying EXIF, XMP and IPTC is loaded and exported as JPEG, PNG or WebP
- **THEN** the exported file contains no EXIF, XMP or IPTC block

#### Scenario: A preview carries no EXIF
- **WHEN** a preview rendition is produced for a picture whose source carries an EXIF Orientation other than 1
- **THEN** the rendition contains no EXIF block
