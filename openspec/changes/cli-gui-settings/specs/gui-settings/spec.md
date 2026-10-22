## ADDED Requirements

### Requirement: The settings the command line can follow are published for it

The application SHALL publish, in its configuration directory, a versioned settings file holding the saved preferences
the command line can follow: the processor, the enhancements Autopilot may not suggest, and each enhancement's chosen
default model and quality level, for the enhancements the user chose one for. It SHALL write the file once at launch,
from the preferences in force, and again whenever a Save changes any of these three. A Save that changes none of them,
and a draft that is cancelled, SHALL NOT write it.

The file SHALL be written so that a reader never sees it half-written, and a write that would not change it SHALL be
skipped.

The application SHALL NOT read the file. The preferences it remembers, and everything it does with them, SHALL be
exactly what they are without the file; the file only follows them.

A write that fails SHALL be recorded in the application's log file and SHALL NOT be shown to the user as an error, and
SHALL NOT change any preference.

#### Scenario: Published at launch

- **WHEN** the application starts with a processor, two default models and one enhancement excluded from Autopilot
  saved
- **THEN** the settings file names that processor, those two models with their quality levels, and that exclusion

#### Scenario: Published on Save

- **WHEN** the user switches colorization off for Autopilot and saves
- **THEN** the settings file excludes colorization
- **AND WHEN** the user changes only the background and saves
- **THEN** the settings file is not written

#### Scenario: Cancelled

- **WHEN** the user changes the processor and cancels
- **THEN** the settings file still names the processor in force

#### Scenario: The file is not read back

- **WHEN** the settings file is edited by hand or deleted while the application is closed, and the application starts
- **THEN** the preferences the application shows and uses are the ones it remembered, and the file is written again
  from them
