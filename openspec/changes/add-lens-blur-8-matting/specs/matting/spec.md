## ADDED Requirements

### Requirement: A matte is estimated from a photograph and a trimap

The library SHALL estimate a matte, a coverage from 0 to 1 per pixel, from a photograph and a trimap that marks each
pixel as sure background (0), sure foreground (1) or unknown (0.5). Known pixels SHALL keep the trimap's value; unknown
pixels SHALL take the model's estimate. The model SHALL run only over the parts of the photograph that hold unknown
pixels.

#### Scenario: Known pixels
- **WHEN** a photograph is matted with a trimap
- **THEN** every pixel marked 0 or 1 has exactly that alpha

#### Scenario: Only where needed
- **WHEN** a trimap's unknown pixels all lie in one corner of a 6000 x 4000 photograph
- **THEN** the model runs on the tiles covering that corner only

### Requirement: A matte is cached by photograph and trimap

A matte SHALL be stored in the run cache keyed by the photograph and the trimap, and a second request for the same pair
SHALL be served without running the model, returning the same values.

#### Scenario: The same request twice
- **WHEN** the same photograph is matted twice with the same trimap
- **THEN** the second request runs no model and returns an identical matte
