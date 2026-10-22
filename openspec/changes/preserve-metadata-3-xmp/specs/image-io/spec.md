## ADDED Requirements

### Requirement: An export writes the source's XMP, true of the result

An exported file whose format can hold XMP (JPEG, PNG, WebP) SHALL carry the XMP its source photograph held, with
every property kept as it was except those that would otherwise be false of the exported file:

- the exported file SHALL have a new document and instance identity, SHALL name the source as what it was derived
  from, and SHALL keep the source's original document identity;
- the edit history SHALL keep every event the source recorded and SHALL gain one recording that Open Photo AI wrote
  this file, when, and whether it converted the format; where the source had no history but recorded the software that
  created it, that software SHALL be recorded first;
- the creating tool SHALL be kept, or, where the source names none, SHALL be the software that created the source;
- the modification and metadata dates SHALL be the export's;
- properties that repeat the EXIF fields an export rewrites (software, modification time, orientation, dimensions)
  SHALL agree with the exported EXIF;
- develop settings for the original raw file, and descriptions of a gain map the exported file does not carry, SHALL
  be removed.

Keywords, title, description, rating, label, creator, rights and every other property SHALL be written unchanged.

Where the source carried EXIF or IPTC but no XMP, the export SHALL carry XMP holding the history and the creating
tool. A source with no metadata SHALL export with none. XMP that cannot be read as a well-formed packet SHALL be left
out with a warning, never written unchanged.

#### Scenario: A Lightroom export keeps its keywords and rating and records Open Photo AI
- **WHEN** a JPEG exported by Lightroom, whose XMP holds keywords, a rating, a document identity, an edit history and
  Camera Raw settings, is enhanced and exported as JPEG
- **THEN** the exported XMP holds the same keywords and rating, a new document identity derived from the source's, the
  source's history followed by an Open Photo AI event, the export time as its modification date, and no Camera Raw
  settings

#### Scenario: A format change is recorded
- **WHEN** a JPEG is exported as PNG
- **THEN** the history event records a conversion from JPEG to PNG

#### Scenario: A source without XMP gains a history
- **WHEN** a JPEG with EXIF naming its camera's software but no XMP is exported
- **THEN** the exported XMP names that software as the creating tool, and its history records the creation by that
  software and the export by Open Photo AI

#### Scenario: Malformed XMP is left out
- **WHEN** a JPEG whose XMP is not well-formed is exported
- **THEN** the exported file carries no XMP, and a warning is logged

### Requirement: IPTC reaches formats without IPTC-IIM through XMP

An export to a format that cannot hold IPTC-IIM but can hold XMP (PNG, WebP) SHALL carry the source's IPTC Core fields
--- title, keywords, creator and their position, description and its writer, headline, rights, credit, source,
instructions, location, city, state, country and its code, transmission reference, date created --- as the XMP
properties the IPTC Photo Metadata Standard maps them to, wherever the source's XMP does not already hold that
property. A value the source's XMP already holds SHALL win.

#### Scenario: Keywords held only in IPTC reach a PNG
- **WHEN** a JPEG whose keywords and caption are only in its IPTC-IIM is exported as PNG
- **THEN** the PNG's XMP holds those keywords and that caption

#### Scenario: XMP wins over IPTC
- **WHEN** a JPEG whose IPTC and XMP hold different captions is exported as WebP
- **THEN** the WebP's XMP holds the XMP caption

### Requirement: A preview carries no descriptive metadata

A preview rendition SHALL never contain the picture's EXIF, XMP or IPTC, so that a viewer cannot turn the preview away
from the frame the picture's pixels are in.

#### Scenario: A preview carries no descriptive metadata
- **WHEN** a preview rendition is produced for a picture whose source carries EXIF with an orientation other than 1
- **THEN** the rendition contains no EXIF, XMP or IPTC

## REMOVED Requirements

### Requirement: Carried XMP is not yet written

**Reason**: Replaced by "An export writes the source's XMP, true of the result" and "A preview carries no descriptive
metadata": this slice writes the XMP, and only the preview rule remains.

**Migration**: None for users; exports gain XMP.
