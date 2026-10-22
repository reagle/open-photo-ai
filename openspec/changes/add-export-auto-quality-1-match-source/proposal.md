> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

Every lossy export is written at one fixed number per format (JPEG 90, WebP 75, AVIF and HEIC 60 unless the user
changed them), whatever the photograph is. A JPEG that arrived at quality 75 is re-encoded at 90 and grows for no
visible gain; one that arrived at 98 is degraded. The CLI cannot choose an encoding quality at all. Joseph asked for a
quality the application works out from the image itself.

## What Changes

Task estimate for the whole feature: **about 25**, a series of two slices (see design.md - Slices). This change is the
first, `add-export-auto-quality-1-match-source`, estimated at **12 tasks**.

- **A quality mode beside the per-format quality: Fixed (today's behaviour, the default) or Match source.** Match
  source writes a JPEG export at the quality the source JPEG was saved at, estimated from its quantisation tables the
  way ImageMagick and libjpeg-based tools do. A source that is not a JPEG, an export that is not a JPEG, and tables
  the estimate cannot place fall back to the format's set quality.
- The quality actually used is reported for each written file (the export queue row; the CLI's `-v` report; the log).
- **CLI: a new `--output-quality <1-100|source>` flag** (long only - `-q` is already the HD/SD precision). Without it
  the CLI writes as today.
- The Ultra HDR gain-map floor is unchanged: the gain map is encoded at the primary's quality or 90, whichever is
  higher.
- Slice 2 (`add-export-auto-quality-2-perceptual`, pending) adds an Automatic mode that searches for the smallest file
  whose perceptual score against the enhanced image stays above a target, for JPEG, WebP, AVIF and HEIC.

## Capabilities

### New Capabilities

### Modified Capabilities

- `image-io`: the source JPEG's estimated quality is part of what loading a JPEG records.
- `gui-export`: an export can match its source's JPEG quality, and reports the quality it used.
- `cli-enhance`: the CLI can set the output quality, numerically or from the source.

## Impact

- Crates: `rust-sak` (fork; new `image` function), `opai` (capture at load, the quality choice in
  `export::WriteOptions`), `cli` (flag), `gui` (command argument, settings store, Settings > Export page, export
  dialog, 14 locales). The decision of which quality to use is `opai`'s, because both front ends need it.
- `rust-sak`: `image` already walks a JPEG's marker segments up to the first scan (`blocks/jpeg.rs`, `segments`) for
  metadata and gain maps; it has no quantisation-table reader. Extending it there is general-purpose (any image tool
  wants "what quality was this saved at"), so it goes into `rust-sak` - its own tasks, a fork tag, and Joseph's
  go-ahead before pushing.
- No new third-party crates in this slice.
- Base: the Ultra HDR stack (bookmark `uhdr`, `add-color-4-ultra-hdr-gainmap`), which carries the gain-map floor and
  the `rust-sak` fork tags this extends.
