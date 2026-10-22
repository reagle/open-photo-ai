## ADDED Requirements

### Requirement: An export carries the source photograph's colour profile

An exported file SHALL embed the ICC colour profile of the photograph it was made from, whether or not enhancements
were applied and whether or not it was cropped, in every export format that can hold a profile (JPEG, PNG, TIFF,
WebP). In a format that cannot yet hold one, the export SHALL still be written, without it. A photograph with no
profile SHALL be exported with none.

#### Scenario: A wide-gamut photograph enhanced and exported
- **WHEN** a Display P3 JPEG is cropped, denoised and exported as PNG
- **THEN** the PNG embeds the source's Display P3 profile

#### Scenario: Preserve over a profiled source
- **WHEN** a Display P3 JPEG is exported under Preserve with no enhancements
- **THEN** the JPEG written embeds the source's profile
