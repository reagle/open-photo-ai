## ADDED Requirements

### Requirement: The window reopens where the user left it

The application SHALL remember the main window's size, position and whether it was maximised when the window closes or
the application quits, and SHALL restore them at the next launch. Native full-screen SHALL NOT be restored.

A remembered position that no connected display shows any part of SHALL NOT be restored; the platform SHALL place the
window instead, at its remembered size. With nothing remembered, the window SHALL open at its default size, centred,
and not maximised.

Restoring the frame SHALL NOT make the window visible before the interface has drawn itself.

#### Scenario: A resized window is restored

- **WHEN** the user resizes and moves the window, quits, and launches the application again
- **THEN** the window opens at the same size and position

#### Scenario: A maximised window stays maximised

- **WHEN** the user maximises the window and quits
- **THEN** the next launch opens maximised, and un-maximising returns the window to its earlier size

#### Scenario: The display it was on is gone

- **WHEN** the window was last on a second display that is no longer connected
- **THEN** the next launch opens the window on a connected display at its remembered size

#### Scenario: First launch

- **WHEN** the application is launched for the first time for this user
- **THEN** the window opens centred at its default size and is not maximised
