## ADDED Requirements

### Requirement: How a lens blur treats the subject's edges

The command SHALL accept `--lb-edges <matte|depth>`, only alongside `--lens-blur`, defaulting to `matte`. `matte` SHALL
matte the in-focus subject and composite it over the blurred background; `depth` SHALL render from the depth alone, as
before matting existed.

#### Scenario: The depth-only render for comparison
- **WHEN** `opai photo.jpg --lb --lb-f 1.4 --lb-edges depth -o out.jpg` is run
- **THEN** the result equals the lens blur's render before matting existed

#### Scenario: Without a lens blur
- **WHEN** `--lb-edges matte` is given without `--lens-blur`
- **THEN** the command refuses to start and names `--lens-blur` as the missing argument
