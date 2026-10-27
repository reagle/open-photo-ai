## ADDED Requirements

### Requirement: The defocus render follows a per-pixel radius in linear light

The render SHALL take a photograph in linear light and a signed blur radius per pixel (negative behind the focal plane,
positive in front, zero in focus) and SHALL blur each pixel as an anti-aliased disc of that radius would, grading
smoothly with the radius rather than in visible steps. Pixels with a radius under half a pixel SHALL be returned
unchanged. Blurring SHALL happen in linear light, so a bright point spreads into a bright disc rather than a grey smear.

#### Scenario: Radius zero everywhere
- **WHEN** a photograph is rendered with a radius of zero at every pixel
- **THEN** the result equals the photograph, after the round trip through linear light

#### Scenario: A point of light
- **WHEN** a single bright pixel on black is rendered with a radius of 10 px everywhere
- **THEN** the result is a disc of radius about 10 px, of even brightness inside and anti-aliased at the rim, holding
  the same total light

#### Scenario: A blur at the frame's edge
- **WHEN** a uniform photograph is rendered with a radius of 15 px everywhere
- **THEN** the result is the same uniform photograph: the blur neither darkens at the frame's edge nor lets a farther layer show through

#### Scenario: A radius that is not a number
- **WHEN** a pixel's radius is not finite, or a pixel's value is not finite
- **THEN** the render still returns, treating the radius as zero and the value as black, and no other pixel is affected

### Requirement: Nearer layers cover farther ones, and colours do not bleed across depth

The render SHALL composite depth layers from far to near, so that a sharp subject in front of a blurred background stays
sharp at its edge and its colour does not spread into the background, and a blurred foreground spreads over what is
behind it.

#### Scenario: A sharp square in front of a blurred background
- **WHEN** a red square at radius 0 sits on a green background at radius -20 px
- **THEN** the square's pixels are unchanged, and no background pixel farther than 1 px from the square is more than 2
  percent red

### Requirement: Depth edges follow the photograph

Upsampling a low-resolution plane to a photograph's size SHALL use the photograph as a guide, so that an edge in the
plane lands on the photograph's own edge; and a dilation that keeps the larger (nearer) value SHALL be available to keep
pixels that straddle an edge with the nearer side.

#### Scenario: A step edge
- **WHEN** a 4x-reduced plane with a blurred step is upsampled with a photograph that has a sharp edge at the same place
- **THEN** the upsampled step lies within 2 px of the photograph's edge

### Requirement: A keep-sharp weight limits the blur

The render SHALL accept an optional weight plane of any resolution, with values from 0 to 1, that scales each pixel's
radius by one minus the weight, so that a weight of 1 keeps a pixel's radius at zero.

#### Scenario: A fully weighted region
- **WHEN** a photograph is rendered with a radius of 15 px everywhere and a keep-sharp weight of 1 over its left half
- **THEN** the left half, away from the boundary by more than 15 px, is unchanged and the right half is blurred
