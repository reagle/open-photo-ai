## ADDED Requirements

### Requirement: A lens blur keeps the mask in focus

A mask SHALL have two uses, each on unless switched off: limiting the enhancements, and keeping the masked part in focus
under a lens blur. When the chain holds a lens blur and the mask keeps the subject in focus, the lens blur SHALL take
the mask's weight as the share of each pixel held at radius zero. When the chain holds a lens blur and the mask limits
the enhancements, the blend with the photograph SHALL happen before the lens blur, and the lens blur SHALL run over the
blended picture. Changing the mask SHALL NOT estimate the depth again.

#### Scenario: Hold the subject sharp
- **WHEN** a photograph is lens-blurred at f/1.4 with a mask painted over the subject
- **THEN** the painted area is as sharp as the photograph and the rest is blurred as it would be without a mask

#### Scenario: A mask with denoise and a lens blur
- **WHEN** a photograph is denoised and lens-blurred with a mask over the subject, both uses on
- **THEN** the subject is denoised and sharp, and the background is not denoised and is blurred

#### Scenario: Focus only
- **WHEN** the same stack runs with the mask's enhancement limit switched off
- **THEN** the whole photograph is denoised, the subject stays sharp, and the background is blurred

#### Scenario: A mask edit with a lens blur
- **WHEN** a stroke is added to the mask of a photograph that has been lens-blurred
- **THEN** the depth is not estimated again and no model before the lens blur runs again
