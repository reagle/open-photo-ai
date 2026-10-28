## ADDED Requirements

### Requirement: The application remembers the ten photographs most recently opened

The application SHALL keep, across launches, the ten photographs most recently opened, newest first, by path. A
photograph SHALL count as opened when it enters the gallery, whatever opened it, and when it is chosen from Open Recent
while already open. A photograph opened again SHALL move to the top rather than be listed twice, an eleventh SHALL push
the oldest out, and a batch SHALL be listed in its own order with its first photograph newest.

#### Scenario: The list is capped and ordered

- **WHEN** twelve photographs are opened one after another
- **THEN** the list holds the last ten, the twelfth first

#### Scenario: Opened again

- **WHEN** a photograph third in the list is opened again
- **THEN** it is first, and is listed once

### Requirement: What the application remembers is kept in one versioned state file

The list SHALL be kept in a state file holding a schema version, in the application's configuration directory beside
the settings the command line follows, written so that a reader sees the previous contents or the new ones and never a
partial file.

#### Scenario: The list survives a restart

- **WHEN** photographs have been opened and the application is quit and started again
- **THEN** Open Recent lists the same photographs in the same order

### Requirement: A damaged or foreign state file is an empty one

A state file that is missing, unreadable, not valid, or of another version SHALL be read as empty and recorded in the
log, and SHALL NOT stop the application from starting.

#### Scenario: A damaged state file

- **WHEN** the state file holds text that is not valid
- **THEN** the application starts with an empty list, records why in its log, and replaces the file at the next change

#### Scenario: A state file of another version

- **WHEN** the state file's version is not one this build reads
- **THEN** the list is empty, as for a damaged file

### Requirement: The last session can be reopened at launch (slice 2, not built)

A Reopen last session switch, on by default, SHALL be offered in Settings. While it is on, the application SHALL reopen
at launch the photographs open when it last closed, in the same order, with the same current and picked photographs,
each with its enhancement stack and parameters, framing, limiting mask, removal strokes and face choices.

#### Scenario: Reopening the last session

- **WHEN** the application is quit with three photographs open, the second current with a denoise and a crop, and is
  started again
- **THEN** the same three are open in the same order, and the second is current with its denoise and crop

### Requirement: Restoring a session runs nothing it was not asked to (slice 2, not built)

Restoring SHALL NOT store or restore result pixels, and SHALL NOT run any photograph's enhancements at launch other
than the current one's, which runs as opening it would - from the run cache where it can.

#### Scenario: Only the current photograph runs

- **WHEN** a session of twenty photographs with stacks is restored
- **THEN** only the current photograph's enhancements are run, and each other runs when it is selected

### Requirement: A restored session skips what has gone or changed (slice 2, not built)

Photographs missing from disk SHALL be skipped with one notice naming how many. A photograph whose file now has
different content SHALL be reopened without its saved state.

#### Scenario: A photograph changed since

- **WHEN** a photograph of the last session was edited by another application before the launch
- **THEN** it is reopened with no enhancements, framing or mask

#### Scenario: Photographs missing

- **WHEN** two photographs of the last session have been deleted
- **THEN** the rest are reopened and one notice says two could not be found
