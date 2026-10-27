## ADDED Requirements

### Requirement: A lens blur in the list reports its aperture

A lens blur in an image's enhancement list SHALL show its model and its f-number. It SHALL be offered in the menu of
enhancements that can be added, after every other enhancement, and SHALL never be suggested by Autopilot.

#### Scenario: A lens blur at f/2
- **WHEN** a lens blur set to f/2 is in the list
- **THEN** its row reads the model's name and f/2

### Requirement: A lens blur's aperture can be changed from its options

A lens blur's options SHALL offer its f-number as a slider in third stops, in 35 mm full-frame-equivalent terms, from f/1.0 to the first stop at or above the photograph's own equivalent f-number as the photograph is framed, which SHALL be the slider's starting value and SHALL add no blur. The slider SHALL have an accessible name and SHALL be read out as the f-number at its position, naming the right end as the photograph as taken. When the photograph carries no lens information, the options SHALL say that its own f-number is assumed.

#### Scenario: Adding a lens blur
- **WHEN** a lens blur is added to a photograph taken at an equivalent f/6
- **THEN** its slider stands at about f/6, the enhanced view equals the photograph, and moving the slider to f/2 blurs
  it

#### Scenario: An f-number between two stops
- **WHEN** a lens blur is added to a photograph taken at an equivalent f/5.91
- **THEN** the slider ends at f/6.3, not f/5.6, and adds no blur at its starting value

#### Scenario: No lens information
- **WHEN** a lens blur is added to a photograph with no EXIF
- **THEN** the options say that f/8 is assumed, and the slider ends at f/8

### Requirement: The focus point is set by clicking the photograph

A lens blur's options SHALL offer to set its focus point. Choosing it SHALL close the options and arm the preview, which
SHALL then take the next click on the photograph that is not a drag as the focus point, in fractions of the photograph
as framed, mark it, and leave the armed state. Escape SHALL leave the armed state without a change. While armed,
dragging SHALL still pan and the wheel SHALL still zoom.

#### Scenario: Focusing on the subject
- **WHEN** the user chooses Set focus and clicks a point 30 percent across and 60 percent down the photograph
- **THEN** the lens blur's focus becomes 0.3, 0.6 and a new run starts

#### Scenario: Escape
- **WHEN** the preview is armed and Escape is pressed with no dialog or menu open over the photograph
- **THEN** the preview is no longer armed and the focus has not changed

#### Scenario: Escape that closes something else
- **WHEN** the preview is armed and Escape is pressed to close a menu or dialog open over the photograph
- **THEN** the menu or dialog closes and the preview stays armed

#### Scenario: A drag is not a click
- **WHEN** the preview is armed and the user drags the photograph
- **THEN** the photograph pans, the focus does not change, and the preview stays armed

### Requirement: A crop carries a lens blur with it

When a photograph's framing is changed, a lens blur in its list SHALL be carried to the new framing: a lens blur at its photograph's own f-number SHALL stay at the new framing's own f-number, any other f-number SHALL be kept and held no higher than the new framing's own, and the focus point SHALL be moved to the same place on the subject, or to the nearest edge of the new frame where the new frame leaves that place out.

#### Scenario: Cropping a lens blur that adds nothing
- **WHEN** a lens blur at its photograph's own f-number is cropped to a quarter of the photograph's width
- **THEN** its slider ends higher, the lens blur stays at the end, and the preview stays unblurred

#### Scenario: Cropping with the focus on the subject
- **WHEN** a photograph is focused 0.6 across and the crop begins at 0.5 across and is 0.25 of the width
- **THEN** the focus becomes 0.4 across

#### Scenario: Cropping the subject out
- **WHEN** the crop leaves the focus point outside the new frame
- **THEN** the focus becomes the nearest point on the new frame's edge, not its centre
