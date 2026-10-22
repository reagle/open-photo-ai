## ADDED Requirements

### Requirement: An export can match its source's JPEG quality

Besides a fixed quality per format, the user SHALL be able to choose that exports match the source's quality. In that
mode a JPEG export of a photograph loaded from a JPEG SHALL be written at the source's estimated quality; any other
combination - a source that is not a JPEG, an export that is not a JPEG, a source with no estimate - SHALL be written
at that format's set quality. Fixed SHALL be the mode until the user chooses otherwise.

The gain map of an Ultra HDR export SHALL still be written at the primary image's quality or 90, whichever is higher.

#### Scenario: A JPEG source exported as JPEG

- **WHEN** the mode is Match source and a photograph loaded from a quality-80 JPEG is exported as JPEG
- **THEN** the file is written at quality 80

#### Scenario: A PNG source

- **WHEN** the mode is Match source, the JPEG quality is set to 90, and a PNG is exported as JPEG
- **THEN** the file is written at quality 90

#### Scenario: A JPEG source exported as WebP

- **WHEN** the mode is Match source and a JPEG is exported as WebP
- **THEN** the file is written at WebP's set quality

### Requirement: An export reports the quality it used

Each written lossy file SHALL report the quality it was encoded at and whether that came from the setting or from the
source, in the export queue and in the log.

#### Scenario: The queue row after writing

- **WHEN** a JPEG export written in Match source mode at quality 82 finishes
- **THEN** its queue row says it was written at quality 82, from the source
