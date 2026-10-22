## ADDED Requirements

### Requirement: The CLI can set the output quality

The CLI SHALL accept `--output-quality` with a number from 1 to 100, applied to whichever lossy format is written, or
`source`, which writes a JPEG output from a JPEG input at the input's estimated quality and anything else at the
format's default. Without the option, outputs SHALL be written at each format's default quality, as before. The option
SHALL have no short form, since `-q` selects the model precision.

#### Scenario: A numeric quality

- **WHEN** the user runs the CLI on a photograph with `--output-quality 70 --format jpeg`
- **THEN** the output is a JPEG written at quality 70

#### Scenario: Matching the source

- **WHEN** the user runs the CLI on a quality-85 JPEG with `--output-quality source` and JPEG output
- **THEN** the output is written at quality 85

#### Scenario: An out-of-range value

- **WHEN** the user passes `--output-quality 0` or `--output-quality high`
- **THEN** the CLI refuses the option with a message naming the accepted values
