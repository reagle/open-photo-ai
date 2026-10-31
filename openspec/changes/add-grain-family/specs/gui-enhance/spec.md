## ADDED Requirements

### Requirement: A Grain enhancement with an Amount and a Match source switch

The Add enhancement menu SHALL offer Grain last, after Lens Blur, in the Effects group. Its panel SHALL show the model,
an Amount slider from 0 to 100 percent (100 for a new grain) and a Match source switch (on for a new grain), with a line
saying what the switch's state does. The row SHALL read the model, the amount and whether it is matched or added.
Autopilot SHALL never add it. Moving either control SHALL re-run the preview, and export and batch runs SHALL use it.

#### Scenario: Add a grain
- **WHEN** the user adds Grain from the menu
- **THEN** it is added at 100 percent, matching the source, and the preview shows the photograph's own noise back
  where the enhancements smoothed it away

#### Scenario: Add grain on purpose
- **WHEN** the user switches Match source off
- **THEN** the preview re-runs with a sensor's grain added everywhere at the amount
