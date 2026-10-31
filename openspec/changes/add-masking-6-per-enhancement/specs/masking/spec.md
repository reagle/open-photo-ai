## ADDED Requirements

### Requirement: An enhancement can be limited by a mask of its own

A run SHALL accept at most one step mask per family for Denoise, Colorization, Light Adjustment, Color Balance, Sharpen,
and Restore at a scale of 1. A step given a mask SHALL produce, per pixel and per channel, `prev + w (step(prev) -
prev)`, where `prev` is the picture the step ran on and `w` the mask's weight, and the next step SHALL run on that
result. A step mask given for Remove, Face Recovery, Upscale, Restore above a scale of 1 or Lens Blur SHALL be refused
before any model runs. A run with no step masks, or only empty ones, SHALL produce the same pixels and identity as
before step masks existed.

#### Scenario: Sharpen the subject, denoise the background
- **WHEN** a photograph is denoised with a mask that is white over the background and sharpened with a mask that is
  white over the subject, both with hard edges
- **THEN** away from the edges, the background equals the denoised photograph, and the subject equals the photograph
  sharpened without denoising

#### Scenario: A step mask on an upscale
- **WHEN** a run is given a step mask for its Upscale
- **THEN** the run is refused before any model runs, with an error naming the family

#### Scenario: A step mask on a later step at a larger size
- **WHEN** a photograph is upscaled 2x and restored at 1x with a Restore mask white on its left half
- **THEN** the result's left half is restored and its right half equals the upscale's

### Requirement: The limiting mask and step masks combine

When a run has a limiting mask and step masks, the step masks SHALL apply inside the chain and the limiting mask SHALL
blend the chain's result with the photograph after its removals and before its lens blurs, as without step masks.
Changing only the limiting mask SHALL run no model again.

#### Scenario: Both masks
- **WHEN** a photograph is sharpened with a step mask white on its left half and a limiting mask white on its top half
- **THEN** only the top-left quarter is sharpened, and the rest equals the photograph

### Requirement: A step mask edit does not run that step's model again

A masked step's unmasked result SHALL be stored under the key it has without a mask, and its blended result under a key
that also depends on the mask. Every later step's key SHALL depend on the mask. A run that differs from an earlier one
only in one step's mask SHALL serve that step's model and every earlier step from the run cache.

#### Scenario: A stroke on the Denoise mask
- **WHEN** a chain of denoise and sharpen with a Denoise mask is run, and run again with one more stroke on that mask
- **THEN** the second run does not run the denoise model, runs the sharpen model, and its result has a new identity

### Requirement: Step masks are painted in the photograph's own coordinates

A step mask SHALL be expressed in fractions of the photograph as loaded, before any framing, and SHALL be mapped
through the framing of each run before it is rasterised, so the same strokes mark the same part of the photograph under
any crop, turn or flip. A step mask SHALL be rasterised at the chain input's size and read at the size of the step it
limits.

#### Scenario: A crop after painting
- **WHEN** a Sharpen mask is painted over a face, and the photograph is then cropped so the face moves to the frame's
  left edge
- **THEN** the sharpened area is still the face

### Requirement: A masked step leaves the gain map as it was

A masked step SHALL hand on the gain map it was given unchanged.

#### Scenario: Ultra HDR with a step mask
- **WHEN** an Ultra HDR JPEG is sharpened with a step mask and exported
- **THEN** the exported gain map equals the one exported from the same run without the step mask
