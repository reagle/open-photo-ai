## ADDED Requirements

### Requirement: A mask image limits the enhancements

The command SHALL accept `--mask <FILE>`, an image in any format it reads, whose luminance is the mask's weight: white
applies the requested enhancements, black keeps the photograph, grey mixes them. The mask SHALL be resized to each
photograph's size. A photograph whose aspect ratio differs from the mask's by more than 1 percent SHALL be refused with
an error naming both ratios, and the other photographs SHALL still be processed. `--mask` SHALL be accepted only with at
least one enhancement or with `--autopilot`.

#### Scenario: Sharpen the subject only
- **WHEN** `opai photo.jpg --sharpen --mask subject.png -o out.jpg` is run with a mask that is white over the subject
- **THEN** the subject is sharpened and the rest of the photograph is unchanged

#### Scenario: A mask of the wrong shape
- **WHEN** a 3:2 mask is given with a 4:3 photograph
- **THEN** that photograph is refused with an error stating both aspect ratios

#### Scenario: A mask without an enhancement
- **WHEN** `--mask subject.png` is given with no enhancement and without `--autopilot`
- **THEN** the command refuses to start
