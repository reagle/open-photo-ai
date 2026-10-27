## ADDED Requirements

### Requirement: A gain map is blurred with the photograph

When the photograph being blurred carries a gain map whose base rendition is SDR, the lens blur SHALL reconstruct the
HDR rendition at the gain map's resolution from the gain map's own parameters, blur it and the SDR base with the same
radii (scaled to that resolution), and replace the gain map with one encoding the ratio of the two blurred renditions
under the same parameters. The result SHALL carry the replacement, also when served from the run cache. The gain map's parameters SHALL be read from its ISO 21496-1 metadata where it has any, else from its `hdrgm` XMP. A gain map that cannot be rewritten SHALL be dropped from the result with a logged warning ("HDR gain map dropped"), rather than kept to lift the old highlights over the blurred base, and the blur SHALL still succeed; this covers parameters that cannot be read, a base rendition that is HDR, a gain map larger than its photograph, and a three-channel gain map whose base-colour-space flag is clear. Enhancements other than the lens blur SHALL leave the gain map as they do today.

#### Scenario: An Ultra HDR JPEG blurred and exported as JPEG
- **WHEN** the bonsai photograph (an Ultra HDR JPEG) is blurred at f/1.4 and written as JPEG
- **THEN** the file is an Ultra HDR JPEG whose gain map differs from the source's in the blurred background and matches
  it within rounding where the photograph is in focus

#### Scenario: A highlight blooms
- **WHEN** a photograph whose gain map lifts a small highlight is blurred so that the highlight's radius is 10 gain-map
  pixels
- **THEN** the replacement gain map lifts a disc of about that radius around the highlight, less than the original peak
  at its centre

#### Scenario: Unreadable parameters
- **WHEN** a photograph's gain map carries parameters that cannot be read and it is blurred
- **THEN** the result carries no gain map, a warning says it was dropped, and the blurred photograph is returned

#### Scenario: An HDR base rendition
- **WHEN** a photograph's gain map declares an HDR base rendition and it is blurred
- **THEN** the result carries no gain map, a warning says it was dropped, and the blurred photograph is returned

#### Scenario: A gain map smaller than the photograph
- **WHEN** the gain map is a quarter of the photograph's size
- **THEN** its replacement is at that same size and the blur radii are a quarter of the photograph's

#### Scenario: Nothing blurred
- **WHEN** a lens blur adds no blur (at or above the photograph's own f-number)
- **THEN** the result carries the source's gain map unchanged, the same one

#### Scenario: Served from the cache
- **WHEN** the same blur is run twice with the cache enabled
- **THEN** the second result carries the same replacement gain map as the first

#### Scenario: A cache entry without a gain map
- **WHEN** a lens blur's pixels are in the cache without a gain map stored beside them
- **THEN** the step is run again rather than served, so that the result carries the replacement

#### Scenario: Other enhancements
- **WHEN** a denoise alone is run on an Ultra HDR JPEG
- **THEN** the result carries the source's gain map unchanged
