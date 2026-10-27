## ADDED Requirements

### Requirement: The mask panel says what the mask is used for under a lens blur

When the photograph's stack holds a lens blur, the mask panel SHALL offer two switches, both on by default: limit the
enhancements to the mask, and keep the mask in focus. When the lens blur is the only enhancement, only the second SHALL
be shown. Changing either SHALL re-run the preview without estimating the depth again, and SHALL be undoable.

#### Scenario: Focus only from the panel
- **WHEN** a photograph with denoise and a lens blur has a painted subject and the user switches off "Limit the
  enhancements to the mask"
- **THEN** the preview shows the whole photograph denoised, the subject in focus and the background blurred
