## ADDED Requirements

### Requirement: The export dialog warns before an export loses HDR, bit depth or a colour profile

While the export dialog is open and nothing is running, for every queued image whose chosen format will lose something
against its source - HDR, bits per channel, or its colour profile, as the library predicts from the file - the row
SHALL show a warning icon whose tooltip lists each loss in words, and the dialog SHALL show a notice naming how many
of the queued images will lose something. The notice SHALL follow a change of the format choice. Save SHALL NOT be
blocked by it.

#### Scenario: An Ultra HDR photograph queued as JPEG
- **WHEN** an Ultra HDR JPEG is queued with the format JPEG
- **THEN** its row shows the warning, whose tooltip reads that the HDR gain map is dropped because JPEG output is SDR
  only for now, and the notice counts one image

#### Scenario: Changing the format
- **WHEN** a queued Display P3 JPEG's format is changed from JPEG to AVIF
- **THEN** its row's warning appears, naming the Display P3 profile not embedded in AVIF

#### Scenario: Nothing to lose
- **WHEN** only 8-bit sRGB JPEGs are queued as JPEG
- **THEN** no row shows the warning and no notice is shown

#### Scenario: Plain sRGB queued as AVIF
- **WHEN** a queued 8-bit JPEG with the `sRGB IEC61966-2.1` profile's format is changed to AVIF or HEIC
- **THEN** its row shows no warning, since those formats are read as sRGB without a profile

#### Scenario: Stepping through the formats
- **WHEN** the format choice is changed several times while the first prediction is still being answered
- **THEN** only the first and the last choices are asked about, the first answer is not shown, and the last one is

#### Scenario: A large JPEG queued
- **WHEN** a 30 MB JPEG is queued and the format choice changes
- **THEN** the prediction reads its header, not the whole file, and predicts what reading the whole file would

### Requirement: A written file that lost something says what

When an export is written, the `export` command SHALL answer the losses the library assessed for it, and the file's
row SHALL show the warning with those losses in place of the prediction.

#### Scenario: Written with a loss
- **WHEN** a 16-bit PNG is exported as JPEG
- **THEN** the written row shows the warning reading `16-bit → 8-bit in JPEG`

#### Scenario: A profile the encoder could not write
- **WHEN** a PNG embedding a 16,740,000-byte sRGB profile is exported as JPEG, whose prediction showed no warning
- **THEN** the written row shows the warning reading
  `sRGB IEC61966-2.1 profile not embedded in JPEG: the encoder could not write it`
