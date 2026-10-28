## ADDED Requirements

### Requirement: On macOS the File menu holds Open Recent

On macOS the File menu SHALL hold Open Recent below Open..., a submenu listing the recently opened photographs by file
name, newest first, with the parent folder's name added where two share a file name, followed by Clear Menu. Clear
Menu SHALL be unavailable while the list is empty. The submenu's title and Clear Menu SHALL follow the interface
language.

#### Scenario: The submenu lists what was opened

- **WHEN** `beach.jpg` and then `dune.png` have been opened
- **THEN** File > Open Recent lists `dune.png` above `beach.jpg`, then Clear Menu

#### Scenario: Clearing the list

- **WHEN** the user chooses Clear Menu
- **THEN** the list is empty, Clear Menu is unavailable, and the next launch shows an empty list too

### Requirement: On Windows and Linux Open Recent is beside Add images

On Windows and Linux, which have no menu bar, the recent photographs and Clear Menu SHALL be offered by an Open Recent
button beside the drawer's Add images, unavailable while the list is empty. On macOS the button SHALL NOT be shown.

#### Scenario: Windows and Linux

- **WHEN** the application runs on Windows or Linux and photographs have been opened
- **THEN** the drawer's Open Recent button lists them, and choosing one opens it as the macOS menu would

### Requirement: Choosing a recent photograph opens it

Choosing a photograph from Open Recent SHALL make it the current image if it is open, and otherwise open it as Browse
images would, adding it to the gallery and making it current. While a dialog or menu is open over the window, choosing
an entry from the macOS menu SHALL do nothing.

#### Scenario: Opening a recent photograph

- **WHEN** the user chooses `beach.jpg` from Open Recent and it is not open
- **THEN** it is added to the gallery and becomes the current image

#### Scenario: A recent photograph that is already open

- **WHEN** the user chooses a photograph from Open Recent that is open but not current
- **THEN** it becomes the current image, the gallery is unchanged, and it moves to the top of the list

### Requirement: A recent photograph that is gone is not opened

A photograph no longer at its path SHALL NOT be opened: the window SHALL say it could not be found, and the entry SHALL
leave the list.

#### Scenario: A recent photograph that has moved

- **WHEN** the user chooses a photograph whose file has been moved or deleted
- **THEN** nothing is opened, a notice says it could not be found, and it is no longer in the list
