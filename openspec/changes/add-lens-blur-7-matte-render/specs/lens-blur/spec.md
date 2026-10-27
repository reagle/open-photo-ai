## ADDED Requirements

### Requirement: The render composites a matted subject over the blurred background

The defocus render SHALL accept an optional subject: its own colour, its coverage (alpha) and one radius. With a
subject, the render SHALL blur the background as given and SHALL composite the subject as one layer "over" it at the
subject's radius, so that a pixel of coverage `a` shows `a` of the subject's colour and `1 - a` of the blurred
background, and layers nearer than the subject cover it. Without a subject the render SHALL be unchanged.

#### Scenario: A half-covered edge
- **WHEN** a sharp subject with alpha 0.5 along its edge is composited over a background blurred with radius 20 px
- **THEN** each edge pixel equals half the subject's colour plus half the blurred background, within one level

#### Scenario: No subject
- **WHEN** the render is called without a subject and with the default settings
- **THEN** its result is identical to the render before this change

### Requirement: Holes behind the subject are filled from their surroundings

A fill SHALL replace values where a weight is 0 by a smooth continuation of the values where it is 1, without seams, and
SHALL leave values with weight 1 unchanged.

#### Scenario: A hole in a gradient
- **WHEN** a horizontal gradient with a 100 px square of weight 0 is filled
- **THEN** the filled square differs from the gradient by less than 5 percent of its range, and every weight-1 value is
  unchanged

### Requirement: The subject's own colour is separated from the background

Given a photograph and an alpha, a foreground estimate SHALL return, for each pixel with alpha above 0, a colour from
which the background's contribution has been removed.

#### Scenario: A red strand over green
- **WHEN** a composite of a red subject over a green background with alpha ramping from 1 to 0 across its edge is
  separated
- **THEN** the estimated foreground in the ramp is within 10 percent of pure red

### Requirement: Layers can be spaced geometrically

The render SHALL offer layer radii spaced evenly (the default) or geometrically from a first radius by a ratio on each
side of focus.

#### Scenario: Geometric spacing near focus
- **WHEN** radii from -100 to 40 px are rendered with geometric spacing from 1 px by 1.4
- **THEN** a pixel at radius 2 px is rendered from layers no more than 1 px from its radius
