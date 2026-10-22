## ADDED Requirements

### Requirement: The application's own actions have keys on every platform

The application SHALL offer six application-level actions with fixed keys: open images (Command-O on macOS, Ctrl+O
elsewhere), export (Command-S and Shift-Command-E, Ctrl+S and Ctrl+Shift+E), open Settings (Command-comma,
Ctrl+comma), close the current image (Command-W, Ctrl+W), close all images (Shift-Command-W, Ctrl+Shift+W), and quit
(Command-Q on macOS, Ctrl+Q on Linux; on Windows the system's Alt+F4).

Open images SHALL show the same picker as Browse images and add what is chosen. Export SHALL open the Export dialog as
the sidebar's Export button does, and SHALL do nothing whenever that button is unavailable (no image picked). Close
image and close all SHALL act as the file options menu's Close image and Close all images do, and SHALL NOT close the
window or quit. With no image open, the two close actions SHALL do nothing. While a dialog or menu is open over the
window, none of the actions except quit SHALL take effect, and none SHALL take a key pressed in a text field.

#### Scenario: Closing an image from the keyboard

- **WHEN** three images are open and the user presses Command-W on macOS or Ctrl+W elsewhere
- **THEN** the current image is closed, two remain, and the application is still running with its window shown

#### Scenario: Closing all images

- **WHEN** images are open and the user presses Shift-Command-W or Ctrl+Shift+W
- **THEN** every image is closed and the window shows the empty canvas

#### Scenario: Opening images

- **WHEN** the user presses Command-O or Ctrl+O
- **THEN** the platform's file picker opens, and the files chosen are added to the open images

#### Scenario: Opening Settings

- **WHEN** the user presses Command-comma or Ctrl+comma
- **THEN** the Settings dialog opens

#### Scenario: Exporting from the keyboard

- **WHEN** an image is open and picked and the user presses Command-S or Shift-Command-E on macOS, or Ctrl+S or
  Ctrl+Shift+E elsewhere
- **THEN** the Export dialog opens with the picked images queued

#### Scenario: Nothing to export

- **WHEN** no image is picked and the user presses Command-S or Ctrl+S
- **THEN** nothing happens

#### Scenario: Typing in a text field

- **WHEN** the focus is in a text field and the user presses Shift-Command-E or Ctrl+S
- **THEN** the Export dialog does not open

#### Scenario: A dialog is open

- **WHEN** the export dialog is open and the user presses Command-W
- **THEN** no image is closed

### Requirement: On macOS the application has a menu of its own

On macOS the menu bar SHALL carry the application's menu (About, Settings, Services, Hide, Hide Others, Show All,
Quit), a File menu (Open, Export, Close Image, Close All Images), the standard Edit menu, a View menu (Enter Full
Screen), a Window menu (Minimize, Zoom, Bring All to Front) and a Help menu, with the keys of the requirement above
shown beside their items. No menu item SHALL close the window with Command-W. Its labels SHALL follow the interface
language, and Close Image and Close All Images SHALL be unavailable while no image is open, and Export whenever the
sidebar's Export button is.

On Windows and Linux the window SHALL NOT gain a menu bar.

#### Scenario: Choosing Close Image from the File menu

- **WHEN** an image is open and the user chooses File > Close Image
- **THEN** the current image is closed and the application keeps running

#### Scenario: Export is unavailable with nothing to export

- **WHEN** no image is open
- **THEN** File > Export is greyed out

#### Scenario: Menu labels follow the language

- **WHEN** the user switches the interface language to French
- **THEN** the menu's labels are in French

### Requirement: Closing the window quits the application

On every platform, closing the main window with its close button SHALL quit the application, as quitting does. No
keyboard key of the application's own SHALL close the window: on macOS Command-W closes an image, not the window.

#### Scenario: Close button

- **WHEN** images are open and the user clicks the window's close button
- **THEN** the application quits

#### Scenario: Command-W with no image open

- **WHEN** no image is open and the user presses Command-W on macOS
- **THEN** nothing happens and the window stays open

### Requirement: Fixed application keys are listed and cannot be taken

The Keyboard settings page and the shortcuts list SHALL show the application-level actions and their keys as fixed,
read-only entries. Recording one of those keys for a rebindable action SHALL be refused with the reason that the
application uses it.

#### Scenario: Recording Command-O for zoom in

- **WHEN** on macOS the user records Command-O as a binding for Zoom in
- **THEN** the binding is refused and the page says the application uses that key

#### Scenario: Export's two keys are listed

- **WHEN** the user opens the shortcuts list or the Keyboard settings page
- **THEN** Export is listed as fixed with both of its keys

### Requirement: The shortcuts list opens with Command-slash as well as ?

The shortcuts list SHALL open, by default, with Command-slash on macOS and Ctrl+slash elsewhere as well as with `?`.
Both are default bindings of the list's rebindable action, listed and changed on the Keyboard settings page like its
other keys.

#### Scenario: Opening the list with Command-slash

- **WHEN** no dialog is open, the focus is not in a text field, and the user presses Command-slash on macOS or
  Ctrl+slash elsewhere
- **THEN** the keyboard shortcuts list opens
