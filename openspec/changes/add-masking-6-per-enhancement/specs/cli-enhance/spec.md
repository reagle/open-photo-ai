## ADDED Requirements

### Requirement: A mask image per enhancement

The command SHALL accept `--dn-mask`, `--cl-mask`, `--la-mask`, `--cb-mask`, `--sh-mask`, `--rs-mask` and `--gr-mask`, each taking
an image whose luminance is that enhancement's mask (white applies it, black leaves the step's input), each accepted
only with its family's flag. Each SHALL be read once per command, resized to each photograph, and refused for a
photograph whose aspect ratio differs by more than 1 percent, the other photographs still being processed. `--rs-mask`
SHALL be refused with `--rs-scale` above 1. With `--autopilot`, a family mask whose family Autopilot did not choose
SHALL be reported once as unused, and the photograph SHALL still be processed.

#### Scenario: Two masks
- **WHEN** `opai photo.jpg --dn --dn-mask background.png --sh --sh-mask subject.png -o out.jpg` is run
- **THEN** the background is denoised and the subject is sharpened

#### Scenario: A mask without its enhancement
- **WHEN** `--sh-mask subject.png` is given without `--sharpen`
- **THEN** the command refuses to start

#### Scenario: Restore mask at 2x
- **WHEN** `--rs --rs-scale 2 --rs-mask subject.png` is given
- **THEN** the command refuses to start, saying a restore that enlarges takes no mask
