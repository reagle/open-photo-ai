## ADDED Requirements

### Requirement: A loaded image keeps its colour profile

When an image file embeds an ICC colour profile, loading it SHALL report the profile's exact bytes together with its
pixels, for every format that can embed one: JPEG, PNG, TIFF, WebP, AVIF and HEIF. A file that embeds no profile, or
describes its colour only by coded values rather than an ICC profile, SHALL be reported with no profile. A developed
camera RAW SHALL be reported with no profile, its pixels being sRGB.

A profile whose colour model does not describe the decoded pixels (a CMYK profile on a JPEG decoded to RGB) SHALL NOT
be reported, and its absence SHALL be logged.

The pixels SHALL be the same as when no profile is read: loading converts no colour values.

Any picture derived from a loaded one - an enhancement result, whether computed or served from the cache, or a crop -
SHALL carry the same profile as the picture it was derived from.

#### Scenario: A Display P3 JPEG
- **WHEN** a JPEG embedding a Display P3 ICC profile is loaded
- **THEN** the profile reported is byte for byte the one in the file, and the pixels equal those decoded without it

#### Scenario: A file without a profile
- **WHEN** a PNG with no `iCCP` chunk is loaded
- **THEN** no profile is reported

#### Scenario: A camera RAW
- **WHEN** a camera RAW file is loaded
- **THEN** no profile is reported

#### Scenario: An enhancement keeps the profile
- **WHEN** a picture with a profile is enhanced, once computed and once served from the cache
- **THEN** both results carry the source's profile

### Requirement: Writing embeds the colour profile where the format can hold it

When a picture carrying an ICC profile is written as JPEG, PNG, TIFF or WebP, the written file SHALL embed that
profile byte for byte, at every bit depth the format writes and with or without an alpha channel.

When it is written as BMP, GIF, AVIF or HEIF, the write SHALL still succeed, without the profile, and a warning naming
the target format and the profile's description SHALL be logged.

A picture with no profile SHALL be written with none, exactly as before.

#### Scenario: The profile survives each capable format
- **WHEN** a picture carrying a Display P3 profile is written as JPEG, PNG, TIFF and WebP and each file is loaded
  again
- **THEN** each reports the same profile bytes

#### Scenario: A large profile in JPEG
- **WHEN** a picture carrying a profile larger than 64 KiB is written as JPEG and loaded again
- **THEN** the same profile bytes are reported

#### Scenario: A format that cannot hold it yet
- **WHEN** a picture carrying a profile is written as AVIF
- **THEN** the file is written, carries no ICC profile, and a warning naming AVIF is logged
