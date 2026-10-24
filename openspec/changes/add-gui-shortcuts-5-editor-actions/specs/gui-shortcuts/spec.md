## ADDED Requirements

### Requirement: The editor's controls have keyboard actions

The window SHALL offer these keyboard actions, each doing what its control does, with these default bindings: turn
Autopilot on or off (`A`); open the Add enhancement menu (`E`); open Crop/Rotate (`C`); choose the next comparison, in
the order full, side by side, split and then full again (`V`); show or hide the drawer's images (Mod with `\`); and
show the current image in the platform's file manager (Mod with Shift and `R`).

Each SHALL do nothing whenever its control is unavailable or absent: Add enhancement and show in the file manager with
no image open, Crop/Rotate with no image whose pixels can be drawn, and the comparison and the drawer with no image
open. Autopilot SHALL be available whether or not an image is open.

The Add enhancement menu opened from the keyboard SHALL have the focus on its first available entry, so that the arrow
keys and Enter choose one.

Like every other keyboard action, each SHALL be listed on the Keyboard settings page and in the shortcuts list, SHALL
be rebindable there, and SHALL NOT act while the focus is in a text field, while a dialog or a menu is open, or when
the focused control is operated with that key.

#### Scenario: Autopilot from the keyboard

- **WHEN** Autopilot is on and the user presses `A`
- **THEN** Autopilot is off, and its switch shows it

#### Scenario: Adding an enhancement without the pointer

- **WHEN** an image is open and the user presses `E`, then the down arrow and Enter
- **THEN** the menu opens with its first entry focused, and the second entry is added to the image

#### Scenario: Add enhancement with no image

- **WHEN** no image is open and the user presses `E`
- **THEN** no menu opens

#### Scenario: Cycling the comparison

- **WHEN** an image is open, the comparison is side by side, and the user presses `V` twice
- **THEN** the comparison is split after the first press and full after the second

#### Scenario: Showing and hiding the drawer

- **WHEN** an image is open, the drawer's images are hidden, and the user presses Mod with `\`
- **THEN** the drawer's images are shown

#### Scenario: In a text field

- **WHEN** the focus is in the export name field and the user types `a`, `e`, `c` or `v`
- **THEN** the letter is typed, and no action is performed

### Requirement: The enhancements on the current image can be opened and removed from the keyboard

The window SHALL offer seven keyboard actions, bound by default to `1` to `7`, that open the options of the enhancement
in that place in the current image's list, counting from the top. An action for a place past the end of the list SHALL
do nothing, as SHALL all seven while the list is not drawn. When the options are closed, the focus SHALL be on that
enhancement's row.

The window SHALL offer a keyboard action, bound by default to both `Delete` and `Backspace`, that removes the
enhancement whose row has the focus, as that row's remove control does, and then moves the focus to the next row, or
to the previous row when there is no next one, or to the Add enhancement control when no row is left. With the focus
outside the enhancement list, the action SHALL do nothing and SHALL leave the key to whatever else handles it. Holding
the key down SHALL remove one enhancement.

#### Scenario: Opening the second enhancement's options

- **WHEN** the current image has three enhancements and the user presses `2`
- **THEN** the second enhancement's options open, and after Escape the focus is on its row

#### Scenario: A place past the end

- **WHEN** the current image has three enhancements and the user presses `4`
- **THEN** nothing opens

#### Scenario: Removing the focused enhancement

- **WHEN** the focus is on the first of three enhancement rows and the user presses `Delete`
- **THEN** that enhancement is removed and the focus is on the row that is now first

#### Scenario: Delete elsewhere

- **WHEN** the focus is on the canvas and the user presses `Delete`
- **THEN** no enhancement is removed

### Requirement: A magnified image can be panned from the keyboard

The window SHALL offer four keyboard actions, bound by default to Mod with each arrow key, that move the view of a
magnified current image a tenth of the pane in the direction of the arrow, holding the image inside the pane as a drag
does. Holding the key down SHALL keep moving it. While the image is drawn at the size that fits, or with no image open,
they SHALL do nothing. In side by side, both panes SHALL move together.

#### Scenario: Panning right

- **WHEN** the current image is magnified, with more of it to the right of the pane, and the user presses Mod with `→`
- **THEN** the view moves right by a tenth of the pane's width

#### Scenario: At the edge

- **WHEN** the right edge of the magnified image is at the pane's right edge and the user presses Mod with `→`
- **THEN** the image does not move

#### Scenario: Fitted

- **WHEN** the current image is drawn at the size that fits and the user presses Mod with an arrow
- **THEN** nothing moves

### Requirement: Controls with a keyboard shortcut reveal it

Every on-screen control that has a keyboard shortcut SHALL reveal it on hover and on focus, in the application's tooltip
or, for a menu item, at the item's trailing edge. The key shown SHALL be the first the action is bound to now, written
as the platform writes it, and SHALL be absent when the action has no binding. A control SHALL also carry
`aria-keyshortcuts` naming every binding.

#### Scenario: A hint on this platform

- **WHEN** the user focuses the Export button on macOS
- **THEN** its tooltip reads the button's name with `⌘ S`, and with `Ctrl+S` elsewhere

#### Scenario: A rebound action

- **WHEN** the user has bound Autopilot to `P` and hovers the Autopilot switch
- **THEN** its tooltip names `P`, not `A`

#### Scenario: An unbound action

- **WHEN** the user has removed every binding of Show in Finder and opens the file menu
- **THEN** its item shows no key and carries no `aria-keyshortcuts`

#### Scenario: A menu item

- **WHEN** the user opens the file menu on Windows
- **THEN** Close image shows `Ctrl+W` at the item's trailing edge
