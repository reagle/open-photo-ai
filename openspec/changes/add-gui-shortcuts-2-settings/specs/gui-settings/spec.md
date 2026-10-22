## MODIFIED Requirements

### Requirement: The settings surface is divided into pages, one shown at a time

The settings surface SHALL divide its preferences into five pages - General, Performance, Enhancements, Export and
Keyboard - and SHALL show one page at a time. A list of the five pages SHALL always be visible beside it, and choosing
a page from that list SHALL show that page in place of the one before it.

Each page SHALL open with its own name and one line saying what the page covers, so a user who arrives on a page can
tell what it is for without reading its controls.

Every preference the surface holds SHALL be on exactly one page. A preference on no page is one the user cannot reach;
a preference on two is one that can be changed in one place and read back in another.

Changing pages SHALL NOT discard, apply or reset anything. Every preference is a draft until Save wherever it was
changed, so a change made on one page SHALL still be there after moving to another page and back, and SHALL be applied
or discarded with the rest.

The surface SHALL open on the General page.

#### Scenario: Opening the settings surface

- **WHEN** the user opens the settings surface
- **THEN** the General page is shown, with its name and its one-line description
- **AND** the list of the five pages is shown beside it, with General marked as the current page

#### Scenario: Moving to another page

- **WHEN** the user chooses Export from the list of pages
- **THEN** the Export page is shown in place of the page before it
- **AND** Export is marked as the current page

#### Scenario: Moving to the Keyboard page

- **WHEN** the user chooses Keyboard from the list of pages
- **THEN** the Keyboard page is shown, listing every keyboard action with the keys it is bound to

#### Scenario: A change survives moving between pages

- **WHEN** the user changes the language, moves to the Export page, then back to General
- **THEN** the language still reads as the user changed it
- **AND** nothing has been applied

#### Scenario: Every preference has a page

- **WHEN** the pages are compared with the preferences the surface holds
- **THEN** every preference is on exactly one page

### Requirement: A page's settings can be reset to their defaults

The settings surface SHALL offer a way to put the settings on the page being viewed back to their defaults. Only that
page's settings SHALL change; the settings on the other pages SHALL be left as they are in the draft.

A reset SHALL change the draft and nothing else. Nothing SHALL be applied until the user saves, and cancelling after a
reset SHALL leave every preference as it was when the surface was opened.

A setting's default SHALL be the value it reads as for a user who has never chosen: the language the application would
start in on a first launch, the particle field, analytics on, the automatic processor, each enhancement's first
catalogue model at its first quality level, every enhancement allowed for Autopilot, each export format's starting
quality, and every keyboard action bound to its default keys.

#### Scenario: Resetting the Export page

- **WHEN** the user changes the JPEG quality and the language, then resets the Export page
- **THEN** every export quality reads its starting value
- **AND** the language still reads as the user changed it

#### Scenario: A reset is a draft

- **WHEN** the user resets a page and then cancels
- **THEN** every preference is what it was before the surface was opened

#### Scenario: A reset that is saved

- **WHEN** the user resets the Enhancements page and saves
- **THEN** every enhancement defaults to its first catalogue model at its first quality level
- **AND** Autopilot may suggest every enhancement

#### Scenario: Resetting the Keyboard page

- **WHEN** the user has bound zoom in to another key, resets the Keyboard page and saves
- **THEN** every keyboard action is bound to its default keys
