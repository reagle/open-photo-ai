## ADDED Requirements

### Requirement: The current image can be shown at its actual pixels

The window SHALL offer a keyboard action, bound by default to Mod with `1`, that magnifies the current image so that
one of its pixels is drawn on one pixel of the display, about the middle of the pane. Where that magnification lies
outside the range the zoom control allows, the image SHALL be drawn at the nearest end of that range.

The window SHALL also offer a toggle, bound by default to `Z`: when the current image is at the size that fits, it
SHALL show the image at its actual pixels, and at any other magnification it SHALL fit the image.

With no image open, both actions SHALL do nothing.

#### Scenario: Actual pixels on a large photograph

- **WHEN** a photograph four times wider than the pane is open at the size that fits, on a display with one display
  pixel per layout pixel, and the user presses Mod with `1`
- **THEN** the photograph is magnified four times, about the middle of the pane

#### Scenario: Toggling from fitted

- **WHEN** the current image is at the size that fits and the user presses `Z`
- **THEN** it is shown at its actual pixels

#### Scenario: Toggling back

- **WHEN** the current image is magnified and the user presses `Z`
- **THEN** it is drawn whole and centred at the size that fits

### Requirement: The previous or next image can be chosen from the keyboard

The window SHALL offer two keyboard actions, bound by default to `←` and `→`, that make the image before or after the
current one, in the order the drawer lists them, the current image. At the first image the previous action, and at the
last image the next action, SHALL do nothing. The drawer SHALL scroll, when needed, so that the current image is in
view.

#### Scenario: Moving to the next image

- **WHEN** three images are open, the first is current, and the user presses `→`
- **THEN** the second image is current and drawn on the canvas

#### Scenario: At the last image

- **WHEN** the last image is current and the user presses `→`
- **THEN** it stays current

### Requirement: Arrow keys stay with the control that is operated by them

An arrow key, `Home`, `End`, `Page Up` or `Page Down` SHALL be left to the control that has the focus, and no shortcut
SHALL act on it, when that control is operated with those keys: a slider, a group of options such as the comparison
toggles, the split comparison's divider, tabs, a list, a menu or a toolbar.

#### Scenario: The zoom slider has the focus

- **WHEN** the drawer's zoom slider has the focus and the user presses `→`
- **THEN** the slider moves, and the current image does not change

### Requirement: The shortcuts can be seen from the window

The window SHALL offer a keyboard action, bound by default to `?`, that opens a list of every keyboard action with the
keys it is bound to now, shown as the user presses them on this platform, and saying which actions have none. The list
SHALL say where the bindings are changed. It SHALL close with `Escape`.

The zoom control's step buttons SHALL show, in their tooltips, their name and the first key they are bound to now, or
their name alone when they have none. The Keyboard settings page SHALL say which key opens the list, when one does.

#### Scenario: Opening the list

- **WHEN** the user has bound zoom in to `Shift` with `Z` and presses `?`
- **THEN** a list opens showing zoom in bound to `Shift` with `Z`, and every other action with its keys

#### Scenario: The zoom-in tooltip on macOS

- **WHEN** the application runs on macOS with the default bindings and the pointer rests on the zoom-in button
- **THEN** its tooltip reads its name followed by `⌘ =`
