## ADDED Requirements

### Requirement: What a mask image is used for

The command SHALL accept `--mask-use <both|enhancements|lens-blur>`, defaulting to `both`, only alongside `--mask`.
`enhancements` SHALL limit the enhancements to the mask without keeping it in focus under a lens blur; `lens-blur` SHALL
keep the mask in focus under a lens blur and apply every other enhancement to the whole photograph. `lens-blur` without
`--lens-blur` SHALL be refused.

#### Scenario: Focus only
- **WHEN** `opai photo.jpg --denoise --lb --lb-f 1.4 --mask subject.png --mask-use lens-blur -o out.jpg` is run
- **THEN** the whole photograph is denoised and the subject stays in focus while the rest is blurred

#### Scenario: Nothing to keep in focus
- **WHEN** `--mask-use lens-blur` is given without `--lens-blur`
- **THEN** the command refuses to start and says the mask would have no effect
