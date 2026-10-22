## ADDED Requirements

### Requirement: A framing applies to an HDR photograph's gain map too

When a framing is applied to a photograph that carries a gain map, the same flips, turn and rectangle SHALL be applied
to the gain map at its own resolution, so that the gain map of the framed photograph covers exactly what its pixels
cover, and an export of it carries a gain map in register with the written image. A framing that changes nothing SHALL
leave the gain map as it was.

#### Scenario: A cropped and turned Ultra HDR photograph is exported as JPEG
- **WHEN** a gain-map JPEG is turned 5°, mirrored left-to-right and cropped to its left half, then exported as JPEG
- **THEN** the exported gain map shows the same part of the scene, mirrored and turned the same way, at the written
image's size

#### Scenario: A quarter-resolution gain map
- **WHEN** a photograph whose gain map is a quarter of its width and height is cropped
- **THEN** the gain map is cropped to the same region measured in its own pixels, within one gain-map pixel
