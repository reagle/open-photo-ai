## ADDED Requirements

### Requirement: Every surface over the photograph is a draft dialog or a live surface, and answers the keys of its kind

The GUI SHALL treat every surface drawn over the photograph as one of two kinds, and a new surface SHALL be one or the
other.

A **draft dialog** holds changes that are not kept until its primary button is pressed --- the settings dialog, the
export dialog, Crop/Rotate and Select faces. While one is open it SHALL take every key. Escape SHALL be Cancel,
discarding the draft. Return SHALL press the primary button (Save, Apply), keeping the draft, unless the focus is on a
control that Return operates itself (a button, a link, a text area, a select, a combobox, a listbox or its option, a
menu or its item, a switch, a checkbox or a radio), an input method is composing, Return is pressed with a modifier, or
the press has already been handled (as by the Keyboard page while it records a binding). Return in a single-line field
SHALL commit what was typed and then press the primary button, which sees the committed value. A disabled primary button
SHALL NOT be pressed, and a dialog with no primary button in its current state SHALL do nothing on Return. A press
outside a draft dialog SHALL NOT dismiss it.

A **live surface** changes the photograph as it is used and has nothing to cancel --- an enhancement's options panel,
the Remove, Refine and limiting-mask brushes, and Set focus. Escape and Return SHALL both mean Done, closing it and
keeping what was done, with Return left to a focused control that Return operates as above. Where a live surface is open
over another (an options panel while a brush is up), the one on top SHALL take the key.

#### Scenario: Return saves the settings

- **WHEN** the settings dialog is open with a change drafted and the focus is not on a button, and the user presses
  Return
- **THEN** the change is saved and the dialog closes, as pressing Save does

#### Scenario: Return in a quality box

- **WHEN** the user types a quality into the Export page's box and presses Return
- **THEN** the typed quality is committed and the dialog is saved with it

#### Scenario: Return on a focused Cancel

- **WHEN** a draft dialog is open with the focus on its Cancel button, and the user presses Return
- **THEN** the dialog is cancelled, not saved

#### Scenario: Return while recording a binding

- **WHEN** the Keyboard page is recording a binding and the user presses Return
- **THEN** the dialog stays open and is not saved

#### Scenario: Return closes an options panel

- **WHEN** an enhancement's options panel is open and the user presses Return with the focus on its slider, or outside
  the panel
- **THEN** the panel closes and every value it set is kept

#### Scenario: Return finishes a brush

- **WHEN** a brush is up and no button has the focus, and the user presses Return
- **THEN** the brush is done, and what was painted is kept

### Requirement: The photograph stays operable under a live surface

While every surface open over the window is a live surface, the window SHALL still perform its view actions from the
keyboard --- zoom in, zoom out, fit, actual pixels, toggle zoom, the four pans and show original --- under the same
rules that leave a key to a field or a control as everywhere else, and SHALL leave every other window action to wait
until the surface is closed. While a dialog, an alert, a menu or a listbox is open, no window key SHALL act. The wheel,
the pinch, a drag and Space with a drag SHALL keep working on the preview. A press on the preview, or on the drawer's
zoom control, SHALL NOT close an enhancement's options panel; a press anywhere else SHALL.

#### Scenario: Zooming with a panel open

- **WHEN** an enhancement's options panel is open and the user presses Mod with `=`
- **THEN** the current image's magnification increases, and the panel stays open

#### Scenario: An editor key with a panel open

- **WHEN** an enhancement's options panel is open and the user presses `C`
- **THEN** Crop/Rotate does not open

#### Scenario: A dialog over the panel

- **WHEN** a dialog is open over an options panel and the user presses Mod with `=`
- **THEN** the magnification does not change

#### Scenario: Panning the photograph beside a panel

- **WHEN** an enhancement's options panel is open and the user drags on the preview, or presses the drawer's zoom
  buttons
- **THEN** the photograph pans or zooms, and the panel stays open

#### Scenario: Clicking the sidebar closes the panel

- **WHEN** an enhancement's options panel is open and the user presses on the sidebar outside it
- **THEN** the panel closes

### Requirement: A scroll pans and a pinch zooms

Over the preview, a pinch on a trackpad --- delivered by the webview as a wheel event with Control held --- and a wheel
event with Command or Control held SHALL zoom about the point under the pointer, by an amount proportional to the
event's delta, clamped per event so one notch of a mouse's wheel is a bounded step, and held inside the zoom range. A
wheel event with no modifier, or with Shift alone, SHALL pan the photograph by its horizontal and vertical deltas, the
view moving the way the scroll goes, counted in pixels whatever unit the event reports; Shift with a wheel that reports
only a vertical delta SHALL pan sideways. A photograph drawn whole SHALL not move. A preference, **Scroll wheel zooms**,
off by default, SHALL make a wheel event with no modifier zoom by the fixed step of the reference instead of panning.
Every pane of the side-by-side and split comparisons SHALL show the same part of the photograph after any of these.

#### Scenario: Two-finger scroll

- **WHEN** the photograph is magnified and the user scrolls with two fingers on a trackpad
- **THEN** the photograph pans and its magnification does not change

#### Scenario: Pinch

- **WHEN** the user pinches outwards over the preview
- **THEN** the photograph is magnified about the point under the pointer, by more for a wider pinch

#### Scenario: The reference's wheel

- **WHEN** Scroll wheel zooms is on and the user turns a mouse's wheel over the preview
- **THEN** each event changes the magnification by the fixed step

### Requirement: Space held pans everywhere on the preview

While Space is held, a press and drag on the preview SHALL pan the photograph, in place of a brush's stroke and in place
of Set focus's click, and the pointer SHALL be drawn as a hand. Space SHALL be left to a focused button, switch, radio
or link, which it presses, and to a text field, and SHALL NOT be taken while a dialog, a menu or a listbox is open.
Where the user has bound a bare Space to a window action, Space SHALL be that action, except while a brush is up. Space
SHALL remain unbound by default.

#### Scenario: Space and a drag with no brush

- **WHEN** Set focus is armed, the photograph is magnified, and the user holds Space and drags on it
- **THEN** the photograph pans, no focus point is set, and Set focus is still armed after Space is released

#### Scenario: Space on a focused button

- **WHEN** a button has the focus and the user presses Space
- **THEN** the button is pressed and nothing pans

### Requirement: The shortcuts list states the input rules

The shortcuts list SHALL end with a fixed section naming Return (accept a dialog, finish a panel or a brush), Escape
(cancel a dialog, finish a panel or a brush), the zoom gestures (pinch, the modifier with a scroll, and a scroll where
Scroll wheel zooms is on) and the pan gestures (a scroll where it is off, a drag, and Space with a drag), as this
platform writes the keys.

#### Scenario: The list under the default

- **WHEN** Scroll wheel zooms is off and the user opens the shortcuts list
- **THEN** its last section lists a scroll under panning and not under zooming
