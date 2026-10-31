## ADDED Requirements

### Requirement: Restore's grain on the command line

The command SHALL accept `--rs-grain <PERCENT>`, a whole number from 0 to 100, only with `--restore`, and SHALL run the
restore with that share of the removed noise put back (0 is off, the default).

#### Scenario: Match the source's grain
- **WHEN** `opai photo.jpg --rs --rs-grain 100 -o out.jpg` is run
- **THEN** the restored photograph has its source's noise back

#### Scenario: Without a restore
- **WHEN** `--rs-grain 50` is given without `--restore`
- **THEN** the command refuses to start
