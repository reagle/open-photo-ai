## ADDED Requirements

### Requirement: Lens Blur is an enhancement applied last

The library SHALL offer a Lens Blur enhancement (artifact prefix `lb`) with one model, Lisbon, whose required artifacts
are the depth model's graphs. A chain SHALL apply it after every other enhancement, including Upscale. Autopilot SHALL
never suggest it.

#### Scenario: Placed after an upscale
- **WHEN** a chain holds an upscale and a lens blur, in either order
- **THEN** the upscale runs first and the lens blur last

### Requirement: The blur follows a real lens, adding only what the photograph lacks

The lens blur SHALL take an f-number to simulate, in 35 mm full-frame-equivalent terms (1.0 to 32, default 2.8), and a
focus point given as fractions of the framed photograph (default the centre). It SHALL focus at the depth estimated at
that point, and SHALL blur every other pixel by the thin-lens circle of confusion for the photograph's 35 mm-equivalent
focal length at that f-number, in pixels of the output, reduced by the blur the photograph already has at its own
equivalent f-number (blur adding in quadrature). At or above the photograph's own equivalent f-number it SHALL leave the
photograph unchanged. Blur radii SHALL scale with the output's size.

#### Scenario: The photograph's own aperture
- **WHEN** a photograph taken at an equivalent f/6 is blurred at f/8
- **THEN** the result equals the input

#### Scenario: A wider aperture
- **WHEN** the bonsai photograph (24 mm, f/4, crop 1.5) is blurred at f/1.4 with the focus on the tree
- **THEN** the tree stays sharp and the background is blurred, more than at f/2.8

#### Scenario: Larger output, larger discs
- **WHEN** the same photograph is blurred with the same settings after a 2x upscale
- **THEN** every blur radius is twice what it is without the upscale

### Requirement: The lens comes from EXIF, with stated fallbacks

The photograph's 35 mm-equivalent focal length and f-number SHALL be read from its EXIF (focal length, 35 mm-equivalent
focal length, f-number, focal-plane resolution). Where the focal length cannot be established from EXIF, the depth
model's focal estimate SHALL be used; where the f-number or the crop factor is missing, the photograph's own aperture
SHALL be taken as f/8 equivalent. The crop factor read from the focal-plane resolution SHALL come from the sensor's diagonal (the EXIF's pixel dimensions and both focal-plane resolutions), not from an assumed aspect, and SHALL be ignored where the EXIF's image size contradicts the file's own by more than 1 percent. Missing or implausible EXIF SHALL never fail a run.

#### Scenario: No EXIF
- **WHEN** a photograph with no EXIF is blurred at f/2
- **THEN** the run succeeds, using the depth model's focal estimate and an own aperture of f/8 equivalent

#### Scenario: A resized file with a stale resolution
- **WHEN** a photograph's EXIF image size is half its file's
- **THEN** its focal-plane resolution is not used for the crop factor, and the depth model's estimate is

### Requirement: A crop is a smaller sensor behind the same lens

Where the photograph being blurred is smaller across its diagonal than the file it came from (a crop), the lens read from the file's EXIF SHALL be scaled to the crop: its focal length and its f-number each multiplied by the ratio of the diagonals, so that the same physical aperture blurs the same scene by the same number of pixels and the photograph's own equivalent f-number, above which nothing is added, rises with the crop. A lens estimated by the depth model, which sees the crop itself, SHALL NOT be scaled again.

#### Scenario: A 2x crop
- **WHEN** a photograph is blurred at f/2.8, and its 2x crop at f/5.6
- **THEN** the blur radii of the crop equal the whole frame's within 3 percent, and a cutoff at f/6 on the whole frame is at f/12 on the crop

### Requirement: Depth is estimated once; the blur re-renders

A chain holding a lens blur SHALL obtain the depth of its input picture through the cached depth estimate, and SHALL NOT
re-estimate it when only the lens blur's parameters, or any other enhancement's, change. The lens blur's result SHALL be
cached under its parameters, the depth map and the lens it used.

#### Scenario: Nothing to add, nothing estimated
- **WHEN** a photograph whose EXIF alone gives its focal length, f-number and crop factor is blurred at or above its own equivalent f-number
- **THEN** the depth model does not run, and the result equals the input

#### Scenario: A crop changes the depth
- **WHEN** a lens blur is run on a photograph and then on a crop of it
- **THEN** the depth is estimated for each

#### Scenario: Not injected
- **WHEN** a chain with a lens blur that has no depth and adds something is planned
- **THEN** it is refused before anything is installed, as uninjected

#### Scenario: Moving the focus point
- **WHEN** a lens blur is run, then run again with a different focus point
- **THEN** the depth model runs once, and the second result differs from the first

#### Scenario: Changing an earlier enhancement
- **WHEN** a chain of denoise and lens blur is run, then run again with a different denoise strength
- **THEN** the depth model does not run again
