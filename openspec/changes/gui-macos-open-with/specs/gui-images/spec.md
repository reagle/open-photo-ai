## ADDED Requirements

### Requirement: Files the platform asks the application to open are opened

The application SHALL declare the image formats it opens to the platform, so that it is offered in the file manager's
Open With menu for those files without becoming their default application.

Files the platform asks the application to open - from Open With, by dropping onto its icon, or named on its command
line - SHALL be added to the window's open images, whether the application was already running or was launched to open
them. Files that arrive before the window is ready SHALL be added once it is. They SHALL be described exactly as the
picker describes them, and files of a format the application does not open SHALL be named and not opened, as for a
drop. The window SHALL be shown and brought to the front.

#### Scenario: Open With while running

- **WHEN** the application is running and the user chooses Open With > Open Photo AI for two photographs
- **THEN** both are added to the open images and the window comes to the front

#### Scenario: Open With at launch

- **WHEN** the application is not running and the user opens a photograph with it
- **THEN** the application starts and, once its window is ready, that photograph is open

#### Scenario: The application is offered, not imposed

- **WHEN** the application is installed
- **THEN** it appears in Open With for a JPEG, and the JPEG's default application is unchanged

#### Scenario: An unsupported file

- **WHEN** the platform asks the application to open a text file and a photograph together
- **THEN** the photograph is opened and the text file is named as not opened
