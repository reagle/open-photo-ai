## ADDED Requirements

### Requirement: Grain on the command line

The command SHALL accept `--grain [MODEL]` (`--gr`), whose one model is `sensor`, with `--gr-amount <PERCENT>`, a whole
number from 0 to 100 (default 100), and `--gr-add`, each only with `--grain`. It SHALL run the grain last, matching the
photograph's grain unless `--gr-add` is given. `--autopilot` SHALL refuse it, and `--gui` SHALL list it among the
ignored options.

#### Scenario: Match the source's grain after a restore
- **WHEN** `opai photo.jpg --rs --gr -o out.jpg` is run
- **THEN** the restored photograph has its source's noise back where the restore smoothed it

#### Scenario: Add grain on purpose
- **WHEN** `opai photo.jpg --gr --gr-add --gr-amount 30 -o out.jpg` is run
- **THEN** 30 percent of a sensor's high-ISO grain is added everywhere

#### Scenario: Without a grain
- **WHEN** `--gr-amount 50` is given without `--grain`
- **THEN** the command refuses to start
