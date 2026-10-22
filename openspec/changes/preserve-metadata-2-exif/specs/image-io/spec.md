## ADDED Requirements

### Requirement: An export writes the source's EXIF, true of the result

An exported file whose format can hold EXIF (JPEG, PNG, WebP) SHALL carry the EXIF its source photograph held, with
every field kept as it was except those that would otherwise be false of the exported file:

- the software that wrote the file SHALL name Open Photo AI, its version and the export's local date;
- the file's modification date and time, and its UTC offset where the source records offsets, SHALL be the export's;
- the pixel dimensions SHALL be the exported image's;
- the orientation SHALL be kept for a source whose pixels are held in their stored frame, and SHALL say "as stored"
  for a camera RAW, whose pixels were already turned by the decoder;
- the embedded thumbnail SHALL be removed, since it shows the unenhanced, uncropped original.

The capture date and offsets, camera, lens and exposure, the location and every other field SHALL be written
unchanged. Location SHALL always be kept: there is no option to remove it.

The camera maker's private block (MakerNote) SHALL be kept byte for byte whenever it can be kept without moving it.
Where it cannot --- the rewritten EXIF would not fit the file's EXIF container --- the EXIF SHALL be written without
it and a warning naming the file, the camera maker and the reason SHALL be logged. EXIF that cannot be read as a valid
structure SHALL be left out with a warning, never written unchanged.

#### Scenario: An upscaled JPEG keeps its capture facts with true dimensions
- **WHEN** a JPEG whose EXIF records a camera, a capture time with offset, a GPS position, an orientation and a
  thumbnail is upscaled and exported as JPEG
- **THEN** the exported EXIF records the same camera, capture time, offset, GPS position and orientation, the exported
  image's dimensions, Open Photo AI as its software, the export time as its modification time, and no thumbnail

#### Scenario: The MakerNote is kept where it was
- **WHEN** a photograph whose EXIF holds a MakerNote is exported as JPEG, PNG or WebP
- **THEN** the exported EXIF holds the same MakerNote bytes at the same offset from the start of the EXIF data

#### Scenario: A MakerNote that cannot be kept is dropped and logged
- **WHEN** the rewritten EXIF of a photograph would exceed what a JPEG can hold
- **THEN** the exported JPEG carries the EXIF without the MakerNote, and a warning names the file, the maker and the
  reason

#### Scenario: PNG and WebP carry the EXIF too
- **WHEN** a JPEG with EXIF is exported as PNG or WebP
- **THEN** the exported file carries the rewritten EXIF

### Requirement: An export writes the source's IPTC into JPEG

An exported JPEG SHALL carry the IPTC-IIM data (keywords, caption, creator, copyright, location and the rest) its
source held, unchanged, together with the digest that identifies it. Every other resource of the block that carried it
--- such as an embedded preview --- SHALL be left out.

An export to a format that has no place for IPTC-IIM, or for EXIF, SHALL still be written, and a warning naming the
format and what was left out SHALL be logged.

#### Scenario: Keywords survive a JPEG export
- **WHEN** a JPEG whose IPTC holds keywords and a Photoshop preview is exported as JPEG
- **THEN** the exported file holds the same IPTC keywords and digest, and no Photoshop preview

#### Scenario: A format without a place for the blocks
- **WHEN** a JPEG with EXIF and IPTC is exported as BMP, or as PNG
- **THEN** the file is written, and a warning names the format and the blocks it could not hold

### Requirement: Carried XMP is not yet written

Until the export applies the rewrite that keeps written XMP true of the written pixels, an exported file and a preview
rendition SHALL NOT contain the XMP packet a picture carries. A preview rendition SHALL never contain the picture's
EXIF or IPTC, so that a viewer cannot turn the preview away from the frame the picture's pixels are in.

#### Scenario: An export does not yet write the carried XMP
- **WHEN** a JPEG carrying EXIF, XMP and IPTC is loaded and exported as JPEG
- **THEN** the exported file contains EXIF and IPTC but no XMP

#### Scenario: A preview carries no descriptive metadata
- **WHEN** a preview rendition is produced for a picture whose source carries EXIF with an orientation other than 1
- **THEN** the rendition contains no EXIF, XMP or IPTC

## REMOVED Requirements

### Requirement: Carried EXIF, XMP and IPTC are not yet written

**Reason**: Replaced by "An export writes the source's EXIF, true of the result", "An export writes the source's IPTC
into JPEG" and "Carried XMP is not yet written": this slice writes EXIF and IPTC, and only XMP is still withheld.

**Migration**: None for users; exports gain EXIF and IPTC. The preview rule moves unchanged into "Carried XMP is not
yet written".
