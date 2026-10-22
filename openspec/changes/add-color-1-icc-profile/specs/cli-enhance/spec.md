## ADDED Requirements

### Requirement: A written result carries the input's colour profile

Each result the command writes SHALL embed the ICC colour profile of the input it was made from, in every output
format that can hold one (JPEG, PNG, TIFF, WebP), whatever enhancements were applied and whether the format was
preserved or chosen with `--format`. In a format that cannot yet hold one, the result SHALL still be written, without
it. An input with no profile SHALL produce a result with none.

#### Scenario: A Display P3 photograph upscaled to TIFF
- **WHEN** `opai p3.jpg --up --format tiff -o out` is run on a JPEG embedding a Display P3 profile
- **THEN** `out/p3.tiff` embeds the same profile
