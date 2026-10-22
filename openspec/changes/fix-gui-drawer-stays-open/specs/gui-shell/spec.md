## MODIFIED Requirements

### Requirement: Clicking away folds the drawer

A press on the preview canvas outside the unfolded drawer SHALL fold it, so that the part of the canvas the drawer is
covering is reachable again by the same gesture that moves attention back to it.

A press anywhere else - the sidebar, the navbar, a dialog, a menu or a notice - SHALL NOT fold the drawer: a user
adding an enhancement or exporting is working on the images the strip shows, not reaching past it.

A menu opened from within the drawer is part of the drawer for this purpose, however far outside the drawer's own
bounds it is drawn. Operating it SHALL NOT fold the drawer, because a user reaching into a menu the drawer itself
opened is not reaching past the drawer.

#### Scenario: A click on the canvas

- **WHEN** the drawer is unfolded and the user clicks the preview canvas above it
- **THEN** the drawer folds

#### Scenario: A click on the sidebar or navbar

- **WHEN** the drawer is unfolded and the user presses Add enhancement in the sidebar, or Export in the navbar
- **THEN** the drawer stays unfolded

#### Scenario: A click inside the drawer

- **WHEN** the drawer is unfolded and the user clicks one of its thumbnails or controls
- **THEN** the drawer stays unfolded

#### Scenario: A click inside a menu the drawer opened

- **WHEN** the user opens a thumbnail's file options menu and chooses one of its actions
- **THEN** the action is taken and the drawer does not fold on account of that click

#### Scenario: Dismissing that menu

- **WHEN** the user opens a thumbnail's file options menu and then clicks the preview canvas
- **THEN** the menu closes and the drawer folds
