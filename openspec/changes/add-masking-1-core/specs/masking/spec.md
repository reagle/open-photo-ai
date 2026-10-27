## ADDED Requirements

### Requirement: A mask limits where the enhancements apply

When a run is given a mask, the result SHALL be the chain's result where the mask's weight is 1, the photograph the
chain started from where it is 0, and the mix `source + w (result - source)` in between, per pixel and per channel. The
chain's models SHALL run over the whole frame as they do without a mask. The result SHALL keep the bit depth it would
have had without a mask.

#### Scenario: A black mask
- **WHEN** a photograph is sharpened with a mask whose weight is 0 everywhere
- **THEN** the result's pixels equal the photograph's

#### Scenario: A white mask
- **WHEN** a photograph is sharpened with a mask whose weight is 1 everywhere
- **THEN** the result's pixels equal those of the same run without a mask

#### Scenario: Half and half
- **WHEN** a photograph is denoised with a mask that is white on the left half and black on the right half, with a hard
  edge
- **THEN** the left half equals the unmasked result and the right half equals the photograph, except within 2 px of the
  edge

### Requirement: A resized result shows an enlargement of the photograph where it is masked out

When the chain changes the photograph's size, the photograph SHALL be enlarged to the result's size with a Lanczos3
filter before the blend, and the mask SHALL be read at the result's size.

#### Scenario: A masked 2x upscale
- **WHEN** a photograph is upscaled 2x with a mask that is black on its right half
- **THEN** the result is twice the photograph's size, and its right half equals a Lanczos3 enlargement of the
  photograph's right half

### Requirement: A mask is brush strokes over an optional base, rasterised at any size

A mask SHALL consist of an optional base weight image, a list of strokes, and whether it is inverted. A stroke SHALL
have a mode (paint or erase), a radius as a fraction of the framed photograph's longer side, a softness from 0 to 1, and
points as fractions of the framed photograph's width and height. Rasterising SHALL start from the base (or from 0),
apply the strokes in order (a paint stroke raising the weight to the stroke's own, an erase stroke lowering it to one
minus the stroke's own), and invert the result if asked. The stroke's own weight SHALL be 1 within its hard core, 0
beyond its radius, and fall smoothly between, with an edge at least one pixel wide. The same mask rasterised at two
sizes SHALL give the same weight at corresponding points.

#### Scenario: Resolution independence
- **WHEN** a mask with one soft stroke is rasterised at 1000 x 750 and at 4000 x 3000
- **THEN** at every point of a 10 x 10 grid of fractions, the two weights differ by less than 0.02

#### Scenario: Erase undoes paint
- **WHEN** a mask has a paint stroke and then an erase stroke along the same points with the same radius and softness
- **THEN** its weight is 0 everywhere

#### Scenario: Painting twice does not change a soft edge
- **WHEN** the same paint stroke is applied twice
- **THEN** the weight equals that of applying it once

#### Scenario: Inverted
- **WHEN** a mask is inverted
- **THEN** its weight at each pixel is one minus the weight it has when not inverted

### Requirement: Changing the mask does not run a model again

A mask SHALL NOT be part of any step's cache key. A run that differs from an earlier one only in its mask SHALL serve
every step from the run cache and SHALL redo only the blend. The result's identity SHALL depend on the mask: two runs
with different masks SHALL produce different identities, and a run with no mask or an empty mask SHALL produce the same
identity as before masking existed.

#### Scenario: A mask edit
- **WHEN** a chain of denoise and sharpen is run with a mask, and then again with one more stroke added
- **THEN** the second run executes no model, and the two results have different identities

#### Scenario: An empty mask
- **WHEN** a run is given a mask with no base and no strokes
- **THEN** its result and identity equal those of the same run without a mask
