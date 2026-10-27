## ADDED Requirements

### Requirement: Remove fills the marked part of the photograph from its surroundings

A Remove operation SHALL carry a mask whose weight, rasterised at the chain input's size, marks the pixels to remove
where it is at least 0.5. The result SHALL replace every pixel within 8 px of a marked pixel with a fill generated from
the photograph around it, mix the fill and the photograph between 8 and 16 px, and leave every pixel 16 px or more from
a mark unchanged. The result SHALL have the input's size and channel depth.

#### Scenario: A small object
- **WHEN** a photograph is run through Remove with a mask white over a tag and black elsewhere
- **THEN** the tag is replaced by the surface around it, and every pixel more than 16 px from the white part equals the
  photograph's

#### Scenario: Nothing marked
- **WHEN** a Remove operation's mask is below 0.5 everywhere
- **THEN** the result equals the photograph and no model runs

#### Scenario: A 16-bit photograph
- **WHEN** a 16-bit photograph is run through Remove with 16-bit output
- **THEN** the result is 16 bits per channel and the unmarked part equals the photograph exactly

### Requirement: Each region is inferred with context of its own

The marked pixels SHALL be split into connected regions. Each region SHALL be inferred over a crop of the photograph
around it, with context on every side of 0.6 times the region's longer side plus 64 px, clamped to the frame, resampled
so that its longer side is 512 px. Regions whose crops overlap SHALL be inferred together over one crop.

#### Scenario: Two objects far apart
- **WHEN** a mask marks two small objects at opposite corners of a large photograph
- **THEN** each is inferred over its own crop, and the rest of the photograph is unchanged

#### Scenario: Two objects close together
- **WHEN** a mask marks two objects whose crops would overlap
- **THEN** both are filled by one inference over a crop containing both

### Requirement: Remove runs before every other enhancement

A chain holding a Remove operation SHALL run it first, on the photograph as it was given, whatever order the operations
were listed in. A Remove operation with an empty mask SHALL be left out of the chain, and the result SHALL carry the
identity of the chain without it.

#### Scenario: Remove with a sharpen
- **WHEN** a chain lists a sharpen and then a Remove
- **THEN** the Remove runs first and the sharpen runs over its result

#### Scenario: Remove added before anything is painted
- **WHEN** a chain holds a Remove operation whose mask has no strokes and no base, and a sharpen
- **THEN** no removal model is installed or run, and the result's identity is that of the sharpen alone

### Requirement: A removal is cached by its mask

A Remove operation's run cache tag SHALL include its mask's signature. A run that differs from an earlier one only in an
operation after the removal SHALL serve the removal from the run cache.

#### Scenario: Change the sharpen strength
- **WHEN** a photograph is run through Remove and a sharpen, then again with another sharpen strength
- **THEN** the second run serves the removal from the cache and runs only the sharpen

#### Scenario: Change the region
- **WHEN** the same chain is run again with one more stroke in the removal mask
- **THEN** the removal is computed again and the result has a different identity

### Requirement: A limiting mask does not bring back what was removed

When a run with a Remove operation is also given a limiting mask, the masked blend SHALL mix the chain's result with the
photograph after its removals rather than with the photograph as it was given.

#### Scenario: Remove outside the masked subject
- **WHEN** a photograph is run through Remove and a sharpen with a limiting mask white over the subject only, and the
  removed object is outside the subject
- **THEN** the object stays removed and only the subject is sharpened

### Requirement: Faces in the removed region are not restored

A face found on the source for a face recovery in the same chain SHALL NOT be restored when the centre of its box is
marked for removal.

#### Scenario: Remove a passer-by
- **WHEN** a chain holds a Remove over a passer-by and a face recovery whose faces are detected on the photograph
- **THEN** the passer-by's face is not restored over the fill, and the other faces are

### Requirement: Remove prefers the CPU under Auto

Under the `Auto` provider the removal model SHALL be opened on the CPU. An explicitly requested provider SHALL be used
as requested.

#### Scenario: Auto on a Mac
- **WHEN** Remove runs under `Auto` on a Mac with Core ML
- **THEN** the run reports that its model ran on the CPU

### Requirement: The catalogue publishes Remove and the mask it takes

The catalogue SHALL list the Remove family, first in the apply order, with the variant Lima at FP32 and one parameter,
`mask`, of a kind that tells a front end the value is a painted region rather than a control.

#### Scenario: A front end reads the catalogue
- **WHEN** a front end reads the catalogue's Remove entry
- **THEN** it finds order 0, the variant `lima` published at FP32, and a parameter named `mask` of kind `mask`
