## Purpose

Keyboard shortcuts in the GUI: which actions can be performed from the keyboard, the keys they are bound to by default
on each platform, when a key press is left to the control that has focus, and what the press-and-hold comparison
draws.

## ADDED Requirements

### Requirement: The current image can be zoomed and fitted from the keyboard

The window SHALL offer three keyboard actions on the current image, bound by default as follows, where Mod is Command
on macOS and Control on Windows and Linux:

| Action | Default binding |
|---|---|
| Zoom in | Mod with `=` (the same key with Shift, and the numeric keypad's `+`, SHALL also count) |
| Zoom out | Mod with `-` (the numeric keypad's `-` SHALL also count) |
| Fit to window | Mod with `0` |

Zoom in and zoom out SHALL change the current image's magnification exactly as the zoom control's step buttons do: by
the same step, about the middle of the pane, holding at either end of the range. Fit to window SHALL draw the current
image whole and centred, at the size that fits. With no image open, these actions SHALL do nothing.

A binding that names Mod SHALL NOT match while the other platform's modifier is held instead, so Control with `=` does
not zoom on macOS.

#### Scenario: Zooming in from the keyboard

- **WHEN** an image is open and the user presses Mod with `=`
- **THEN** the current image's magnification increases by the step the zoom control's plus button uses, about the
  middle of the pane
- **AND** the zoom slider reports the new magnification

#### Scenario: Zooming out past the fitted size

- **WHEN** the current image is at the size that fits and the user presses Mod with `-`
- **THEN** it stays at the size that fits

#### Scenario: Fitting a magnified image

- **WHEN** the current image is magnified and panned, and the user presses Mod with `0`
- **THEN** the current image is drawn whole and centred at the size that fits

#### Scenario: The wrong modifier on macOS

- **WHEN** the application runs on macOS and the user presses Control with `=`
- **THEN** the magnification does not change

#### Scenario: No image open

- **WHEN** no image is open and the user presses Mod with `=`
- **THEN** nothing changes

### Requirement: Holding a key shows the original in place of the result

The window SHALL offer a keyboard action that, for as long as its key is held, draws the original image in every pane
that otherwise draws the enhanced image, and labels those panes as the original. It SHALL be bound by default to `\`,
and SHALL NOT be bound to `Space` by default, because a menu's trigger button gets the focus back when the menu closes
and operates on `Space`. Releasing the key SHALL draw the result again. If the window loses focus while the key is held,
the result SHALL be drawn again, as if the key had been released.

The action SHALL apply in all three comparison modes, and SHALL NOT change the magnification, the position, the
split's divider or any enhancement being run. Holding the key down long enough to repeat SHALL have no further effect.

#### Scenario: Holding backslash in the full comparison

- **WHEN** the current image has an enhanced result, the full comparison is chosen, and the user holds `\`
- **THEN** the canvas draws the original image, labelled as the original
- **AND** when the user releases `\` the canvas draws the enhanced result again, labelled as enhanced

#### Scenario: Space is not a default

- **WHEN** no binding has been changed, the Add enhancement menu has just closed with the focus back on its button, and
  the user presses `Space`
- **THEN** the original is not shown in place of the result, and the key is left to the button

#### Scenario: Holding the key side by side

- **WHEN** the side-by-side comparison is chosen and the user holds `\`
- **THEN** both panes draw the original image at the same magnification and position as before

#### Scenario: Focus lost while held

- **WHEN** the user holds `\` and switches to another application before releasing it
- **THEN** on returning, the canvas draws the enhanced result

### Requirement: A shortcut never takes a key from typing, a dialog or a focused control

A key press SHALL be left to the control it was meant for, and no shortcut SHALL act, when:

- the focus is in a field the user types into, or in an editable region;
- a dialog or a menu is open;
- the key is `Space`, which the user may bind to an action, and the focus is on a button, checkbox, slider, tab or menu
  item, where `Space` operates that control.

When a shortcut does act, the key press SHALL have no other effect in the window, such as scrolling or zooming the
page itself.

#### Scenario: Typing in a number field

- **WHEN** the focus is in a number field in the settings dialog and the user presses `-` with Mod, or `\`
- **THEN** the current image's magnification and comparison do not change

#### Scenario: Space on a focused button

- **WHEN** the user has bound `Space` to show the original, the drawer's zoom-in button has focus, and the user presses
  `Space`
- **THEN** the button is activated, and the original is not shown in place of the result

#### Scenario: Backslash on a focused button

- **WHEN** the drawer's zoom-in button has focus and the user holds `\`
- **THEN** the original is shown in place of the result until the key is released

#### Scenario: A dialog is open

- **WHEN** the crop dialog is open and the user presses Mod with `0`
- **THEN** the current image's magnification does not change

### Requirement: The zoom controls state their shortcuts to assistive technology

The zoom control's step buttons SHALL expose their keyboard shortcuts to assistive technology, in the form the
platform's modifier takes, so a screen reader can report them.

#### Scenario: The zoom-in button on macOS

- **WHEN** the application runs on macOS
- **THEN** the zoom-in button declares the shortcut Command with `=` to assistive technology
