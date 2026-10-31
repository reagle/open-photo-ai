## ADDED Requirements

### Requirement: Paint a mask for one enhancement

Each row of the enhancement list for Denoise, Colorization, Light Adjustment, Color Balance, Sharpen, Restore and Grain SHALL
offer Paint mask, which opens the mask brush on that enhancement's own mask. A row with a painted mask SHALL show a
badge and offer Edit mask. While a row's mask is edited, the preview SHALL tint that mask only. The masks SHALL be sent
with every preview, export and batch run, and SHALL be forgotten when their row is taken off the stack or the file is
closed. A crop change SHALL keep them. Restore's Paint mask SHALL be unavailable while its scale is above 1, its strokes
kept. The photograph-wide mask SHALL be labelled as the mask for all enhancements.

#### Scenario: Sharpen only the subject
- **WHEN** the user adds Sharpen, presses its row's Paint mask, paints over the subject and presses Done
- **THEN** the Sharpen row shows a mask badge, and the preview and the export sharpen only the subject

#### Scenario: Crop after painting
- **WHEN** a Sharpen mask is painted and the photograph is then cropped
- **THEN** the Sharpen mask is kept and still covers the same part of the photograph

#### Scenario: A slow step follows
- **WHEN** a Denoise mask is edited in a stack that also holds Restore
- **THEN** the brush panel says that Restore runs again after each stroke
