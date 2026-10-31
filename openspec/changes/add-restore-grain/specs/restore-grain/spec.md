## ADDED Requirements

### Requirement: Restore can put back the noise it removed

A restore run SHALL take a grain amount from 0 to 1, 0 by default. At 0 the result SHALL equal a restore run without
grain, pixel for pixel, under the same cache key as before grain existed. Above 0, noise SHALL be added to the result
whose fine residual, per luminance band and channel, makes up the given share (in standard deviation) of what the
result lacks against the photograph the restore was given; nothing SHALL be added where the photograph had no more fine
residual than the result.

#### Scenario: Grain off
- **WHEN** a photograph is restored with grain 0
- **THEN** the result and its cache key equal those of a restore run before grain existed

#### Scenario: Match a grainy source
- **WHEN** a photograph with sensor noise is restored at 1x with grain 1
- **THEN** the result's fine residual in each luminance band is within 10 percent of the photograph's

#### Scenario: A clean source
- **WHEN** a photograph without noise is restored with grain 1
- **THEN** no noise is added

### Requirement: Grain is deterministic and at the output's pixel size

The noise SHALL depend only on the result's pixel coordinates and on the measured figures, so the same run SHALL give
the same pixels every time. Under an enlarging restore the noise SHALL be drawn per output pixel, at the per-pixel level
measured on the photograph before it was enlarged.

#### Scenario: Two runs
- **WHEN** the same photograph is restored twice with grain 0.5 and no run cache
- **THEN** the two results are identical

#### Scenario: An enlarging restore
- **WHEN** a photograph with sensor noise is restored at 2x with grain 1
- **THEN** the result's fine residual per output pixel is within 15 percent of the photograph's per source pixel

### Requirement: Grain leaves the gain map alone

A restore with grain SHALL hand on the gain map it was given unchanged.

#### Scenario: Ultra HDR
- **WHEN** an Ultra HDR JPEG is restored with grain 1 and exported
- **THEN** its gain map equals the one exported from the same restore with grain 0
