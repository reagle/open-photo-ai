## ADDED Requirements

### Requirement: Served renditions carry the source photograph's colour profile

A preview or thumbnail served to the interface for an opened photograph, an enhanced result or a cropped one SHALL
embed the ICC colour profile of the photograph it was made from, so that the window can display it in its own colours.
A photograph with no profile SHALL be served with none.

#### Scenario: A Display P3 photograph previewed
- **WHEN** the interface asks for a canvas-sized rendition of an opened Display P3 JPEG, and of its enhanced result
- **THEN** both served images embed the source's Display P3 profile
