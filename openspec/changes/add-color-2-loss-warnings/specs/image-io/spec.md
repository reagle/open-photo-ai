## ADDED Requirements

### Requirement: A loaded image records what an export can lose

Loading an image SHALL record, beside its pixels, what the source file held that a written file can lose: its true
bits per channel (10 or 12 for such an AVIF or HEIF, not the 16 it decodes to), whether its samples are floating
point, whether it is HDR, and the description of a colour profile the load discarded because it did not describe the
decoded pixels. Every picture derived from it - a crop, an enhancement, a cached result - SHALL carry the same record.

A file SHALL be recorded as HDR when it is a JPEG with a gain map (an ISO 21496-1 `APP2` marker, an MPF entry of type
Gain Map, or XMP declaring the `hdrgm` namespace), or declares a PQ or HLG transfer through CICP (AVIF or HEIF `nclx`,
PNG `cICP`) or an ICC profile's `cicp` tag. Reading this SHALL NOT fail a load: anything malformed is read as not HDR.
A developed camera RAW SHALL be recorded as 16-bit and not HDR.

#### Scenario: An Ultra HDR JPEG
- **WHEN** a JPEG with an ISO 21496-1 marker, `hdrgm` XMP and an MPF gain map is loaded
- **THEN** it is recorded as HDR with a gain map, and 8 bits per channel

#### Scenario: A 16-bit PNG
- **WHEN** a 16-bit PNG is loaded
- **THEN** it is recorded as 16 bits per channel, not float, not HDR

#### Scenario: A truncated marker
- **WHEN** a JPEG whose MPF segment is cut short is loaded
- **THEN** the load succeeds and the image is recorded as not HDR

### Requirement: An export reports what it lost against its source

Writing an export SHALL assess what the written file loses against the picture's recorded source, before it is written
and reconciled with what the encoder reports it embedded, and SHALL report each loss with the result and log it as a
warning:

- **HDR**: an HDR source is written as SDR, in every format, until a later slice writes HDR.
- **Bit depth**: the file holds fewer bits per channel than the source, or integer samples where the source had float.
- **Colour profile**: the source's profile is not embedded, because the format cannot hold one (BMP, GIF, AVIF, HEIF
  today), because it is for another colour model than the file is written in, because the load discarded it, or
  because the encoder could not write it (a profile too large for the format). A plain sRGB profile left out of an
  AVIF or HEIF SHALL NOT be reported: those formats are read as sRGB without one.

The assessment SHALL agree with what is actually written: a block the encoder leaves out at write time SHALL be
reported even though it was not foreseen. A source with nothing to lose, or written where nothing is lost, SHALL
report no loss. A loss SHALL NOT make the export fail.

#### Scenario: An Ultra HDR JPEG written as JPEG
- **WHEN** the Ultra HDR JPEG above, with a Display P3 profile, is exported as JPEG
- **THEN** exactly one loss is reported: `HDR gain map dropped: JPEG output is SDR only for now`

#### Scenario: The same photograph written as AVIF
- **WHEN** it is exported as AVIF
- **THEN** the HDR loss and `Display P3 profile not embedded in AVIF` are reported

#### Scenario: A 16-bit TIFF written as JPEG
- **WHEN** a 16-bit TIFF is exported as JPEG
- **THEN** `16-bit → 8-bit in JPEG` is reported

#### Scenario: Nothing lost
- **WHEN** an 8-bit sRGB JPEG with an sRGB profile is exported as JPEG or PNG
- **THEN** no loss is reported

#### Scenario: A profile too large for JPEG
- **WHEN** a PNG embedding a 16,740,000-byte sRGB profile is exported as JPEG, which cannot hold a profile that large
- **THEN** the file is written without it and
  `sRGB IEC61966-2.1 profile not embedded in JPEG: the encoder could not write it` is reported

#### Scenario: Plain sRGB written as AVIF
- **WHEN** an 8-bit JPEG with the `sRGB IEC61966-2.1` profile is exported as AVIF or HEIF
- **THEN** no loss is reported, while a Display P3 one exported as AVIF still reports its profile not embedded

#### Scenario: An export of an export is not HDR
- **WHEN** a JPEG whose XMP declares the `hdrgm` namespace on `rdf:RDF`, with no gain map to carry, is exported as
  JPEG and the result is exported again
- **THEN** the result's XMP no longer declares the `hdrgm` namespace anywhere, and the second export reports no HDR
  loss
