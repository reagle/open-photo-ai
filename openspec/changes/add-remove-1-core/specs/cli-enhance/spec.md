## ADDED Requirements

### Requirement: A mask image marks what to remove

The command SHALL accept `--remove <FILE>` (alias `--rm`), an image in any format it reads, whose luminance marks what
to remove: white is removed and filled from its surroundings, black is kept. `--remove` SHALL count as an enhancement
on its own and SHALL combine with the enhancement flags, `--mask` and `--autopilot`.

#### Scenario: Remove a tag
- **WHEN** `opai photo.jpg --rm tag.png -o out.jpg` is run with a mask white over a tag
- **THEN** the tag is removed and the rest of the photograph is unchanged

#### Scenario: Remove and enhance
- **WHEN** `opai photo.jpg --rm tag.png --sh -o out.jpg` is run
- **THEN** the tag is removed first and the result is sharpened

### Requirement: A removal mask fits each photograph

The removal mask SHALL be resized to each photograph's size. A photograph whose aspect ratio differs from the mask's by
more than 1 percent SHALL be refused with an error naming both ratios, and the other photographs SHALL still be
processed.

#### Scenario: A removal mask of the wrong shape
- **WHEN** a 3:2 removal mask is given with a 4:3 photograph
- **THEN** that photograph is refused with an error stating both aspect ratios
