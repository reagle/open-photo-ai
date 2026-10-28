## ADDED Requirements

### Requirement: A removal is painted over the preview

When a photograph is open, the window SHALL offer Remove as an enhancement. While its region is being edited, a
primary-button drag over the photograph SHALL paint (or, with Option/Alt held, erase) a stroke of the chosen size and
softness, with the same pan, zoom, brush ring, undo and redo as the mask brush, and a tint of its own colour SHALL show
what will be removed. When the button is released the preview SHALL show the painted part removed.

#### Scenario: Paint over a tag
- **WHEN** the user adds Remove and drags across a tag
- **THEN** a tint covers the dragged path, and when the button is released the preview shows the tag removed

#### Scenario: Remove with nothing painted
- **WHEN** the user adds Remove and paints nothing
- **THEN** the preview is unchanged and no removal model is opened

### Requirement: A painted removal goes with the export

The removal strokes SHALL go with the photograph's export, and the exported file SHALL show the removal the preview
showed.

#### Scenario: Export
- **WHEN** the user exports a photograph with a painted removal
- **THEN** the exported file has the painted part removed as the preview showed it

### Requirement: A removal's region is its own

The area painted for Remove SHALL be kept apart from the limiting mask: each SHALL have its own strokes and its own undo,
only one SHALL be edited at a time, the removal SHALL offer no Invert, and taking Remove off the stack SHALL forget its
strokes. A change of crop SHALL clear the removal's strokes as it clears the limiting mask's, with one notice whose Undo
restores the crop and what it cleared.

#### Scenario: Undo a removal stroke
- **WHEN** the user has painted a limiting mask and two removal strokes, and presses Cmd+Z while painting the removal
- **THEN** the second removal stroke is gone and the limiting mask is unchanged

#### Scenario: A crop change
- **WHEN** the user applies a different crop to a photograph with a painted removal
- **THEN** the removal's strokes are cleared, a notice says so, and its Undo brings back the crop and the strokes
