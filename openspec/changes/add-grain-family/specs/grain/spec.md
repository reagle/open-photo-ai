## ADDED Requirements

### Requirement: Grain is an enhancement of its own, applied last

The library SHALL offer a Grain family whose operation runs after every other enhancement of a chain, a lens blur
included, whatever order the chain is given in. It SHALL run no model, need no download and open no session. Autopilot
SHALL never suggest it.

#### Scenario: Last in the chain
- **WHEN** a chain of Grain, Lens Blur, Restore and Denoise is processed in that order
- **THEN** it runs Denoise, Restore, Lens Blur and then Grain

### Requirement: Matching the source puts back only the grain the result lacks

A grain run SHALL take an amount from 0 to 1 and a mode, matching the source by default. Matching, the noise added
SHALL make up the given share (in standard deviation) of what the result lacks, at each place and brightness, of the
photograph's own fine residual, measured per luminance band on the photograph at its own size; nothing SHALL be added
where the result has the photograph's residual or more, or where the photograph has none.

#### Scenario: A result that lost its grain
- **WHEN** a noisy photograph is smoothed by an enhancement and grained at amount 1
- **THEN** the result's fine residual per luminance band is within 15 percent of the photograph's

#### Scenario: A lens blur's subject and background
- **WHEN** a lens blur's result is grained at amount 1
- **THEN** the blurred background gets the photograph's grain back and the part in focus, which kept its own, gets
  next to none

#### Scenario: A clean photograph
- **WHEN** a photograph without noise is grained, matching
- **THEN** no noise is added

### Requirement: A denoise is not undone

Where the chain contains a denoise, the photograph a grain matches SHALL be what the last denoise made, whether that
denoise was computed or served from the run cache.

#### Scenario: Denoise, then grain
- **WHEN** a chain denoises a noisy photograph and then grains it, matching, at amount 1
- **THEN** the grain adds nothing the denoise removed

### Requirement: Adding grain on purpose

With the mode switched to adding, the grain SHALL add the given share of a sensor's grain (a fixed residual at
mid-grey, shaped by brightness) everywhere, whatever noise the result has.

#### Scenario: Add
- **WHEN** a clean photograph is grained, adding, at amount 0.5
- **THEN** it has half the sensor grain's residual at mid-grey

### Requirement: Grain is deterministic, at the output's pixels, and leaves the gain map alone

The noise SHALL depend only on the result's pixel coordinates and the measured figures, so the same chain SHALL give
the same pixels every time and the run cache is stable. Under an enlargement the noise SHALL be drawn per output pixel
at the photograph's per-pixel level. A grain SHALL hand on the gain map it was given unchanged.

#### Scenario: Two runs
- **WHEN** the same chain with a grain is processed twice with no run cache
- **THEN** the two results are identical

#### Scenario: An enlarged result
- **WHEN** a noisy photograph is enlarged 2x and grained, matching, at amount 1
- **THEN** the result's fine residual per output pixel is within 15 percent of the photograph's per source pixel

#### Scenario: Ultra HDR
- **WHEN** an Ultra HDR photograph is grained
- **THEN** its gain map is the one it was handed

### Requirement: A limiting mask does not limit the grain

A chain with a limiting mask SHALL blend before its trailing lens blurs and grain, and the grain SHALL run over the
blended picture.

#### Scenario: A mask over half the photograph
- **WHEN** a smoothing enhancement limited to the left half is followed by a grain, matching
- **THEN** the left half gets the photograph's grain back and the right half, the photograph's own, gets next to none
