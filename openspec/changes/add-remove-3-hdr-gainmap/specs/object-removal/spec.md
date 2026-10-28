## ADDED Requirements

### Requirement: Remove fills an Ultra HDR gain map under the removal from its surroundings

When the photograph carries an HDR gain map, a Remove operation SHALL hand back a gain map in which the cells under the
hole (any pixel under them within 16 px of a mark) are filled smoothly from the map's own cells outside it, each cell
taking that fill by the mean over its pixels of the colour paste's feather, and every cell with no pixel within 16 px of
a mark is unchanged. A gain map smaller than the photograph SHALL be rewritten at its own resolution.

#### Scenario: A bright object on a full-resolution gain map
- **WHEN** a photograph whose gain map holds a bright object's gain is run through Remove over that object
- **THEN** the gain under the grown marks is that of the map around it, and every cell 16 px or more from a mark equals
  the source map's

#### Scenario: A quarter-resolution gain map
- **WHEN** the photograph's gain map is a quarter of its size on each axis
- **THEN** the map keeps its size, the cells wholly under the grown marks hold the gain around them, and the cells whose
  pixels are all 16 px or more from a mark are unchanged

### Requirement: A removal's rewritten gain map is served from the run cache

The gain map a Remove operation rewrote SHALL be stored beside the step's pixels, and a run served from the run cache
SHALL carry the same gain map.

#### Scenario: Served from the store
- **WHEN** the same removal is run again on the same photograph with a run cache
- **THEN** the model is not run and the result carries the same rewritten gain map

### Requirement: A gain map a removal cannot rewrite is dropped with a warning

When a photograph's gain map cannot be laid over it (larger than the photograph on either axis, or not 8-bit grayscale or
RGB), Remove SHALL still remove the marked part, the result SHALL carry no gain map, the run SHALL log "HDR gain map
dropped: it could not be carried", and an export of the result SHALL report the HDR as not carried. A run served from the
run cache SHALL do the same.

#### Scenario: A gain map wider than the photograph
- **WHEN** a photograph carrying a gain map wider than itself is run through Remove
- **THEN** the result shows the removal, carries no gain map, and a JPEG export of it reports "HDR gain map dropped: it
  could not be carried"
