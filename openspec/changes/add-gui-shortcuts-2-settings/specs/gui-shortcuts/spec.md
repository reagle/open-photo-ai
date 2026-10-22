## ADDED Requirements

### Requirement: The user can change the keys an action is bound to

The settings surface's Keyboard page SHALL list every keyboard action the window offers, each with the keys it is
bound to, shown as the user presses them on this platform (Command as `⌘` on macOS, Control as `Ctrl` elsewhere). The
user SHALL be able to remove any binding, and to add one by pressing the key combination it is to be. An action MAY
have several bindings, or none; an action with none SHALL say so and SHALL do nothing from the keyboard.

Bindings SHALL be drafts like every other preference: they SHALL take effect when the user saves, at once and without
a restart, and SHALL be discarded if the user cancels.

While a binding is being recorded, `Escape` SHALL end the recording without closing the settings surface, and moving
the focus away, including with `Tab`, SHALL end it as well. Pressing a modifier on its own SHALL NOT end it.

Every control on the page that names a key SHALL also expose the action it belongs to, so that a screen reader
announces, for example, "Remove ⌘ =, Zoom in" rather than only the key.

#### Scenario: Rebinding zoom in

- **WHEN** the user removes zoom in's binding, adds `Shift` with `Z` in its place and saves
- **THEN** pressing `Shift` with `Z` zooms in, and Mod with `=` no longer does
- **AND** the zoom-in button declares `Shift+z` to assistive technology

#### Scenario: Cancelling a rebinding

- **WHEN** the user adds a binding to fit and then cancels the settings surface
- **THEN** fit is bound to the keys it had before

#### Scenario: An action with no binding

- **WHEN** the user removes show original's binding and saves
- **THEN** the Keyboard page shows show original as having no shortcut
- **AND** holding `\` does not show the original

#### Scenario: Escape while recording

- **WHEN** the user starts adding a binding and presses `Escape`
- **THEN** the recording ends with nothing added
- **AND** the settings surface stays open

### Requirement: A binding held by another action is refused, with an offer to move it

When the key combination the user records would also be matched by a binding of another action, the binding SHALL NOT
be added. The page SHALL say which action already uses it, and SHALL offer one control that moves it: removing it from
that action and adding it to the one being edited. Nothing SHALL be moved without that control. Recording a
combination the action being edited already has SHALL add nothing.

Two bindings SHALL count as the same when some key press would match both on this platform, so `Mod` with `=` and
Command with `=` are one binding on macOS, and `?` and `Shift` with `?` are one binding everywhere.

#### Scenario: Recording a key another action holds

- **WHEN** the user adds Mod with `0` to zoom in while fit is bound to it
- **THEN** zoom in's bindings are unchanged
- **AND** the page says that fit already uses it, and offers to use it for zoom in instead

#### Scenario: Moving a binding

- **WHEN** the page has said that fit already uses Mod with `0`, and the user chooses to use it for zoom in instead
- **THEN** zoom in is bound to Mod with `0` and fit is no longer bound to it

### Requirement: Keys that would trap the user cannot be bound

The page SHALL refuse, with a message saying the key is kept for moving around the window, a binding of `Escape` or
`Tab` with any modifiers, and `Enter` with no modifier other than `Shift`: taking them would leave a keyboard user
unable to close a dialog, move the focus or press a button. It SHALL also refuse, with a message saying the system
uses it, a combination the platform takes before the window can see it or that closes, hides or minimises the window:
Command with `Q`, `W`, `H` or `M` on macOS, and `Alt` with `F4` on Windows and Linux.

#### Scenario: Binding Escape

- **WHEN** the user starts adding a binding and presses `Shift` with `Escape`
- **THEN** nothing is added and the page says that key is kept for moving around the window

#### Scenario: Binding Tab

- **WHEN** the user starts adding a binding and presses `Tab`
- **THEN** nothing is added and the focus moves to the next control

#### Scenario: Binding Command-Q on macOS

- **WHEN** the application runs on macOS and the user presses Command with `Q` while recording
- **THEN** nothing is added and the page says the system uses that key

### Requirement: The bindings are remembered, and repaired when they cannot be honoured

Bindings the user saved SHALL survive a restart. Only actions whose bindings the user changed SHALL be remembered as
such; every other action SHALL read its default bindings, including actions a later version of the application adds.

On start, a remembered binding SHALL be dropped, and the rest kept, when it names no action this application offers,
is not a key combination, or is one the user could not have bound. When a remembered binding of one action would match
the same key press as a default binding of another, the remembered one SHALL win and the default SHALL be dropped from
the other action, so no key press is ever claimed by two actions.

#### Scenario: A binding survives a restart

- **WHEN** the user binds fit to Mod with `9`, saves and restarts the application
- **THEN** fit is bound to Mod with `9`
- **AND** zoom in, zoom out and show original read their default bindings

#### Scenario: A remembered binding the application cannot honour

- **WHEN** the application starts and the remembered bindings name an action it does not offer, or bind `Escape`
- **THEN** those bindings are dropped
- **AND** every other remembered binding is kept

#### Scenario: A remembered binding meets a default

- **WHEN** the remembered bindings give zoom in Mod with `0`, which is fit's default
- **THEN** Mod with `0` zooms in, and fit has no binding from its defaults that zoom in holds
