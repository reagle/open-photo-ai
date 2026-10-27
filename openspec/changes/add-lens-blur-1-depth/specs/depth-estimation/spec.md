## ADDED Requirements

### Requirement: A depth estimate is a data operation of its own family

The library SHALL offer depth estimation as a data family, Depth (artifact prefix `dp`), whose result is not an image
and which is never placed in an enhancement chain, with one model, Lisbon (MoGe-2 ViT-B). Its operation SHALL be run
through the data-run path, named in progress reports, required-artifact lists and cache keys as an analysis, and reached
publicly through a depth entry point beside the existing analysis entry point, whose own signature SHALL NOT change.

#### Scenario: Depth is never in a chain
- **WHEN** the order in which a chain applies families is read
- **THEN** neither Detection nor Depth is in it, and every other family keeps its position relative to the others

#### Scenario: A depth run names its model
- **WHEN** a depth estimate is run on a photograph
- **THEN** its progress reports name Lisbon and its required artifacts are Lisbon's graphs

### Requirement: Lisbon runs at a fixed size per orientation, stretched to fill it

Lisbon SHALL have two graphs, landscape (1280 x 1024) and portrait (1024 x 1280), FP16. A photograph wider than tall, or square, SHALL use the landscape graph; any other the portrait one. The photograph SHALL be resized to exactly the graph's size, filling it (stretched, not padded), so that no pixel of the result is padding, and the result SHALL be resampled back to the photograph's own aspect, 1280 pixels on its long side and never larger than the photograph.

#### Scenario: A portrait photograph
- **WHEN** a 3064 x 4080 photograph is estimated
- **THEN** the portrait graph is used and the depth map is about 961 x 1280, covering the whole photograph and nothing else

#### Scenario: A 3:2 photograph
- **WHEN** a 5562 x 3708 photograph is estimated
- **THEN** the landscape graph is used, the photograph fills it, and the depth map is 1280 x 853

#### Scenario: A photograph smaller than the graph
- **WHEN** a 640 x 480 photograph is estimated
- **THEN** the depth map is no larger than 640 x 480

### Requirement: The depth map is metric, with a focal-length estimate

The depth map SHALL hold, for every pixel of the photograph at the map's resolution, a distance in metres: the model's affine-invariant points with the shift along the optical axis recovered by least squares against the image plane of the graph, as the model's reference implementation recovers it, multiplied by the model's metric scale, and resampled to the photograph's aspect in disparity so that nothing nearer than a metre is flattened. Pixels the model marks invalid, and any whose distance is not positive, SHALL take the largest valid distance. The map SHALL carry the 35 mm-equivalent focal length implied by the recovered focal, measured against the graph's diagonal. A run with too few valid pixels to recover the shift SHALL fail with an error (unmeasurable) rather than return a flat map.

#### Scenario: Recovery of a known geometry
- **WHEN** a synthetic point map made from a known focal length and shift is recovered
- **THEN** the shift and the focal length come back within 0.1 percent

#### Scenario: Sky
- **WHEN** part of the photograph is marked invalid by the model
- **THEN** those pixels hold the farthest valid distance in the map

#### Scenario: Too few valid pixels
- **WHEN** the model marks every pixel invalid
- **THEN** the run fails as unmeasurable, naming the operation and the number of valid samples

#### Scenario: A near subject
- **WHEN** a point 0.25 m from the camera is resampled to the photograph's aspect
- **THEN** it still reads 0.25 m, not 1 m

#### Scenario: Focus distance at a point
- **WHEN** the distance at a point given as fractions of the frame is asked for
- **THEN** the answer is the median distance in a small window around that point

### Requirement: A depth estimate is cached per photograph and framing

A depth estimate SHALL be stored in the run cache under the photograph's identity (which a crop changes) and the
operation, and a second estimate of the same photograph, framing and model SHALL be served from the cache without
running the model. Changing anything other than the photograph, its framing or the model SHALL NOT invalidate it. A map served from the cache SHALL equal the one computed.

#### Scenario: The second request is served from the cache
- **WHEN** the depth of the same framed photograph is estimated twice with the cache enabled
- **THEN** the model runs once, and both results are equal
