## ADDED Requirements

### Requirement: A Grain slider in the Restore panel

The Restore panel SHALL show a Grain slider from 0 to 100 percent under Strength, at 0 for a new restore, with a hint
that it puts back the photograph's own noise. Moving it SHALL re-run the preview with that grain, and export and batch
runs SHALL use it.

#### Scenario: Add grain back
- **WHEN** the user sets Restore's Grain to 100 percent
- **THEN** the preview re-runs and the restored photograph shows the source's noise again
