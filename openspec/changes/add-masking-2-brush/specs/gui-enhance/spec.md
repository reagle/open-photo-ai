## ADDED Requirements

### Requirement: A mask is painted over the preview

When a photograph with at least one enhancement is open, the window SHALL offer to paint a mask for it. While the mask
is being edited, a primary-button drag over the photograph SHALL paint (or, with the Erase tool or Option/Alt held,
erase) a stroke of the chosen size and softness, a drag with Space held SHALL pan, and the wheel SHALL zoom. A ring
under the pointer SHALL show the brush's size at the current zoom. A tint SHALL show the painted area exactly as the run
will weight it. Undo and Redo SHALL step through the strokes, Invert and Clear included; Escape or Done SHALL leave
editing.

#### Scenario: Paint a stroke
- **WHEN** the user edits the mask and drags across the subject
- **THEN** a tint covers the dragged path at the brush's size, and when the button is released the preview shows the
  enhancements applied there only

#### Scenario: Erase with Option held
- **WHEN** the user holds Option and drags over part of a painted area
- **THEN** that part is removed from the mask and the enhancements no longer apply there

#### Scenario: Pan while editing
- **WHEN** the user holds Space and drags while editing the mask
- **THEN** the photograph pans and nothing is painted

#### Scenario: Undo
- **WHEN** the user paints two strokes and presses Cmd+Z
- **THEN** the second stroke is gone from the tint and from the result, and Shift+Cmd+Z brings it back

### Requirement: The mask applies to the preview and the export alike

A photograph's mask SHALL be sent with every preview run and every export of that photograph. A change to the mask SHALL
re-run the preview when a stroke ends, without running a model again. Changing the crop SHALL clear the mask, after the
crop panel has said that it will, and SHALL offer to undo both together.

#### Scenario: Export matches the preview
- **WHEN** a photograph with a painted mask is exported
- **THEN** the exported image shows the enhancements only where the preview does

#### Scenario: A stroke does not re-run the models
- **WHEN** a stroke is added to a photograph that has already been enhanced
- **THEN** the preview updates without any model running, and the progress shows no model step

#### Scenario: A crop change
- **WHEN** the crop of a photograph with a mask is changed
- **THEN** the mask is cleared and a notice offers Undo, which restores both the previous crop and the mask
