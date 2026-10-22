## MODIFIED Requirements

### Requirement: A second launch of the graphical application raises the window that already exists

Launching the graphical application while an instance of it is already running for the same user SHALL NOT open a
second window and SHALL NOT report an error. The second launch SHALL end successfully, and the running instance's
window SHALL be made visible, restored if it was minimised, and given keyboard focus.

Raising the window SHALL NOT change its size or position. A window the user had maximised SHALL still be maximised
afterwards, since the user asked to see the application rather than to resize it.

Files named on the second launch's command line, resolved against the directory it was launched from, SHALL be opened
in the running instance as files the platform asks it to open. Anything else supplied on the second launch SHALL NOT
alter the running application's state.

#### Scenario: The running window is minimised

- **WHEN** the graphical application is launched while an instance of it is already running for the same user and that
  instance's window is minimised
- **THEN** the window is restored and given focus
- **AND** no second window is opened

#### Scenario: The running window is maximised

- **WHEN** the graphical application is launched while an instance of it is already running for the same user and that
  instance's window is maximised
- **THEN** the window is given focus and is still maximised

#### Scenario: The second launch is not a failure

- **WHEN** the graphical application is launched while an instance of it is already running for the same user
- **THEN** the second launch ends successfully, reporting no error to the user and to whatever started it

#### Scenario: Arguments on the second launch change nothing

- **WHEN** the graphical application is launched a second time with command-line arguments that name no file
- **THEN** the running instance is raised and is otherwise unchanged

#### Scenario: A second launch naming photographs

- **WHEN** the graphical application is launched a second time with two photographs' relative paths
- **THEN** the running instance opens both, resolved against the second launch's directory, and is raised
