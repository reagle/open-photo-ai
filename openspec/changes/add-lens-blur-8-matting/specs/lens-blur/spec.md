## ADDED Requirements

### Requirement: The in-focus subject is matted and composited over the blurred background

When the blur is large enough to matter, the lens blur SHALL build a trimap from the depth around the focus distance,
matte it, fill the background behind the subject, and composite the subject as one layer over the blurred background, so
that fine detail at the subject's edge (hair, fur, leaves) stays sharp while the background seen between it is blurred.
The matte SHALL be computed on the chain's input, so enhancements before the blur and changes of the f-number SHALL NOT
compute it again; a change of focus point SHALL.

#### Scenario: Fur against a street
- **WHEN** a photograph of a person in a fur hood in front of a street is lens-blurred at f/1.4, focused on the face
- **THEN** the hood's strands are sharp, the street between and beyond them is blurred, and no dark haze spreads from
  the hood into the street

#### Scenario: Changing the aperture
- **WHEN** a matted lens blur is run again at a different f-number above 3 px of blur
- **THEN** neither the depth nor the matte is computed again

#### Scenario: A small blur
- **WHEN** the largest blur radius in the photograph is under 3 px
- **THEN** no matte is computed and the render is the depth-only one
