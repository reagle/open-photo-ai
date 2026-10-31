## ADDED Requirements

### Requirement: The General page offers Scroll wheel zooms

The settings surface's General page SHALL offer a switch, **Scroll wheel zooms**, off by default, with a description
saying that a pinch, or a scroll with the platform's Command or Control held, always zooms. Like every preference it
SHALL be a draft until Save, SHALL be put back by Reset to defaults, and SHALL be remembered across restarts; a stored
value that is not a boolean, or none, SHALL read as off. Its label and description SHALL be translated in every
catalogue the application ships.

#### Scenario: Turning it on

- **WHEN** the user turns on Scroll wheel zooms and saves
- **THEN** a wheel event with no modifier over the preview zooms by the fixed step instead of panning, from the next
  event

#### Scenario: Cancelled

- **WHEN** the user turns it on and cancels the dialog
- **THEN** a plain wheel event still pans
