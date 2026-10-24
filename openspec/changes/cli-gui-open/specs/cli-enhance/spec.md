## ADDED Requirements

### Requirement: `--gui` opens the inputs in the GUI instead of enhancing them

With `--gui`, the command SHALL NOT enhance anything. It SHALL expand and check its inputs as a run does, `--recursive`
included, and hand every image found to the graphical application, which SHALL open them whether or not it was running.
Neither `--output` nor an enhancement SHALL be required. It SHALL then print `🖼️ Opening <count> images in Open Photo AI`
and exit with status 0; refused inputs SHALL fail as a run's do (status 2), and a failure to start the application with
status 1.

#### Scenario: A directory tree, GUI closed

- **WHEN** the GUI is not running and the user runs `opai photos -r --gui`, where `photos` holds twelve images in
  subfolders
- **THEN** the GUI starts and opens the twelve images
- **AND** the command prints `🖼️ Opening 12 images in Open Photo AI` and exits with status 0

#### Scenario: A missing input

- **WHEN** the user runs `opai missing.jpg --gui`
- **THEN** the command fails with status 2 naming `missing.jpg`, and the GUI is not started

### Requirement: `--gui` names the options it ignores

Under `--gui`, every option given on the command line apart from the inputs and `--recursive` SHALL be ignored, and the
command SHALL print one warning on stderr naming each of them by its long flag and saying to set them in the GUI. An
option not given on the command line SHALL NOT be named.

#### Scenario: Other options are ignored with a warning

- **WHEN** the user runs `opai beach.jpg --up osaka -q sd -o out --gui`
- **THEN** the command prints `⚠️ --upscale, --quality and --output are ignored with --gui; set them in Open Photo AI
  instead`, opens `beach.jpg` in the GUI, and writes nothing

#### Scenario: Recursion is not an ignored option

- **WHEN** the user runs `opai photos -r --gui`
- **THEN** no warning is printed

### Requirement: `--gui` works while the GUI is running

The command SHALL hand the images over before it starts telemetry and before it initializes the library, so under
`--gui` it SHALL NOT be refused while the GUI holds the single-instance claim and SHALL NOT send usage data.

#### Scenario: GUI already open

- **WHEN** the GUI is running and the user runs `opai beach.jpg --gui`
- **THEN** `beach.jpg` is added to the running window's open images, and no second window opens
- **AND** the command is not refused for another instance holding the claim
