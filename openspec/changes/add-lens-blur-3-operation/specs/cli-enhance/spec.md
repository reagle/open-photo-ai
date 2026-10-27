## ADDED Requirements

### Requirement: A lens blur is requested with its own flags

The command SHALL accept `--lens-blur`, alias `--lb`, with an optional model, `lisbon`, which it SHALL mean when none is
given; `--lb-f <F>`, the 35 mm-equivalent f-number to simulate, a number from 1.0 to 32 inclusive, written either bare
(`1.4`) or with the `f/` prefix (`f/1.4`), defaulting to 2.8; and `--lb-focus <X,Y>`, the focus point as two fractions
of the photograph's width and height from 0 to 1 inclusive, defaulting to `0.5,0.5`. `--lb-f` and `--lb-focus` SHALL be
accepted only alongside `--lens-blur`. A lens blur SHALL count as a requested enhancement, SHALL NOT be combined with
`--autopilot`, and SHALL be applied after every other requested enhancement.

#### Scenario: A bare lens blur
- **WHEN** `opai photo.jpg --lb -o out.jpg` is run
- **THEN** the photograph is blurred with Lisbon at f/2.8, focused at its centre

#### Scenario: Focus and aperture given
- **WHEN** `opai photo.jpg --lens-blur --lb-f f/1.4 --lb-focus 0.3,0.6 -o out.jpg` is run
- **THEN** the photograph is blurred at f/1.4, focused at the point 30 percent across and 60 percent down

#### Scenario: An option without its enhancement
- **WHEN** `--lb-f 2` is given without `--lens-blur`
- **THEN** the command refuses to start and names `--lens-blur` as the missing argument

#### Scenario: Out of range
- **WHEN** `--lb-f 0.5` or `--lb-focus 1.2,0.5` is given
- **THEN** the command refuses to start and states the accepted range, naming the axis for a focus point (`focus_x must be between 0 and 1 inclusive, but 1.2 was supplied`)

#### Scenario: A negative focus
- **WHEN** `--lb-focus -0.1,0.5` is given
- **THEN** the command refuses to start with the range message, not as an unexpected argument

#### Scenario: Either quality
- **WHEN** `-q hd` or `-q sd` is given with `--lens-blur`
- **THEN** Lisbon runs at FP16 in both cases
