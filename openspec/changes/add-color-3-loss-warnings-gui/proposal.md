> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

Slice 2 (`add-color-2-loss-warnings`) makes every export assess what it loses against its source - HDR, bit depth, the
colour profile - and the CLI shows it. The GUI still exports an Ultra HDR photograph as SDR, or a 16-bit TIFF as an
8-bit JPEG, without a word. Joseph's requirement ("if HDR, bit depth, color profile is changed, the user should be
warned") is about the user deciding before the file is written, so the export dialog should say so while the format
can still be changed, and the queue should say which written files lost what.

## What Changes

Task estimate: **8**. Slice 3 of the `add-color` series (design.md - Slices); depends on slice 2.

- **`opai` predicts the losses of a file not yet loaded**: `export::predict_blocking(path, format)` reads the file's
  bytes (not its pixels) and answers the same `Loss` list `export::losses` would for that source written as that
  format at its own depth. One rule, two entry points: both reduce to slice 2's assessment.
- **A new command `export_losses(paths, naming)`** answers, for each queued path, the losses its chosen format will
  cause. The dialog asks again when the queue or the format choice changes, not on every keystroke of a prefix.
- **The export dialog shows a notice before export** when any queued image will lose something, naming how many and
  which kinds, and each affected row shows a warning icon whose tooltip lists the losses in words.
- **After export, each written row with losses shows the warning** from the `export` command's answer (the actual
  assessment), in place of the prediction.
- **i18n keys in every locale file** (14), without plural forms so every language has the same keys.

Crates touched: `opai` (the prediction entry point), `gui` (one command, the `Exported` payload, the React dialog and
row). The front end only renders `Loss` values; the rule stays in `opai`.

`rust-sak` coverage: nothing new; the prediction uses `probe_bytes`, `metadata_bytes` and slice 2's `hdr_bytes`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `gui-export`: the dialog warns before export when the chosen format will lose a queued image's HDR, bit depth or
  colour profile, and each written file that lost something says what.

## Impact

- `crates/opai/src/export/loss.rs` (`predict_blocking`), `crates/opai/src/export/mod.rs`.
- `crates/gui/src/export/` (`mod.rs`, `run.rs`, `names.rs` or a new `losses.rs`), `crates/gui/src/lib.rs` (handler).
- `crates/gui/frontend/ipc/export.ts`, `stores/exportBatch`, `features/export/` (`ExportDialog`, `ExportRow`, a new
  hook and notice), `i18n/locales/*.json`.
