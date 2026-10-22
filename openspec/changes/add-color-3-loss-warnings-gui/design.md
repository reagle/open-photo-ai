## Context

See proposal.md - Why, and `add-color-2-loss-warnings`' design for the assessment itself: `opai::export::Loss` (`hdr`,
`depth`, `profile`, serialized with a `kind` tag), `export::losses(picture, format)`, and `Written::losses` from
`export::write`. The GUI's export runs each queued file through the `export` command at the source's own depth
(`OutputDepth::Source`), one at a time, and the dialog's rows are named by `export_names`, which is pure and asked on
every keystroke.

## Goals / Non-Goals

**Goals:**

- Before Save: the dialog says, while the format can still be changed, that the chosen format will lose something for
  some of the queued images, and each such row says what.
- After Save: each written row with a loss says what was lost, from what was actually written.
- No rule in the front end: the window renders `Loss` values from Rust.

**Non-Goals:**

- Blocking or confirming the export: a loss is a warning; Save stays one click.
- A depth or HDR choice (slices 5 and 8).

## Slices

The `add-color` series, in order. Each is its own change.

| # | Change | Delivers | Depends on | Status |
|---|---|---|---|---|
| 1 | `add-color-1-icc-profile` | Source ICC profile read at load, carried on `Picture` through crop, enhancement and the run cache, embedded on export (JPEG, PNG, TIFF, WebP) and in GUI previews; `rust-sak` ICC read/embed | - | done |
| 2 | `add-color-2-loss-warnings` | `rust-sak` `hdr_bytes`; source fidelity (depth, float, HDR, discarded profile) read at load and carried on `Picture`; the per-export loss assessment in `opai`; CLI warning lines and batch count | 1 | done (bookmark `add-color-2-loss-warnings`) |
| 3 | `add-color-3-loss-warnings-gui` | GUI: the export dialog's notice before export and a per-image warning after it, from slice 2's assessment; i18n in every locale | 2 | **current** |
| 4 | `add-color-4-avif-heif` | ICC and CICP (`nclx`) colour descriptions embedded in AVIF and HEIF, CICP read at load (AVIF/HEIF `nclx`, PNG `cICP`), 10/12-bit AVIF and HEIF encode; extends `avif-rs`/`heif-rs` and `rust-sak` | 1 | pending |
| 5 | `add-color-5-output-depth` | A chosen export depth (Source / 8 / 16 / 32-bit float) in `opai`, the GUI export dialog (all locales) and a CLI `--depth` flag; a float path out of `imaging`; float TIFF; optional OpenEXR as a new `rust-sak` format | 1 | pending |
| 6 | `add-color-6-working-space` | Colour conversion with `moxcms`: source profile to the models' sRGB working space before inference and back afterwards (or to a chosen output profile), with gamut handling; a "convert to sRGB on export" option; RAW developed to a wide-gamut space | 1, 5 | pending |
| 7 | `add-color-7-jpeg-xl` | JPEG XL decode and encode (8/16/float, ICC and CICP, PQ/HLG tagging) as a new `rust-sak` format and an export format in GUI and CLI | 4, 5 | pending |
| 8 | `add-color-8-ultra-hdr` | Ultra HDR / ISO 21496-1 gain-map JPEG: an input's gain map preserved through crop and upscale, and HDR output from HDR-capable sources | 5, 6 | pending |

## How the work divides

- **`opai`**: `export::predict_blocking(path, format) -> Result<Vec<Loss>, ImageIoError>`: the source's `Fidelity` and
  profile from its bytes (no pixel decode; a RAW by extension, without reading it), then slice 2's assessment with the
  written depth the source's own depth gives in that format. It shares `Fidelity::read` with `load`.
- **`gui` (Rust)**: `export_losses(paths, naming)` on a blocking thread, answering `{ path, losses }` per path in the
  order asked (an unreadable file answers none: the export will report its own failure; a path this application never
  opened, by `Opened::knows`, answers none without being read); `Exported::Exported` gains `losses`.
- **`gui` (React)**: `exportLosses` in `ipc/export.ts`; a `useExportLosses(paths, format)` hook keyed on the paths and
  the format choice only; a notice above the queue; a warning icon on a row, whose tooltip lists the losses, fed by
  the prediction until the row is written and by `Exported.losses` after; a `formatLoss(t, loss)` helper.

## Decisions

### D1. Predict from the file, not from opened pictures

The queued images are not all opened (decoded) when the dialog opens, and decoding them to warn would cost seconds.
The prediction reads bytes and headers only, the same facts `load` records, through the same `Fidelity::read`. It is a
lower bound: it may miss a loss but never names one the write will not have. An enhancement that narrows depth (none
does at the GUI's `Source` depth) or changes the colour model (a colourised grayscale photograph loses its `GRAY`
profile to the colour model) is not foreseen, nor a profile the encoder cannot write; the after-export indicator is
the truth. (Review fix: the code and this decision first called it an upper bound, which it is not.)

### D2. A separate command from `export_names`

`export_names` is pure and runs on every keystroke; reading files there would make typing a prefix do I/O. The losses
depend only on the paths and the format choice, so they are asked when those change.

### D3. Words without plurals

The notice reads `Images that will lose quality in this format: {{count}} of {{total}}` style text, so every locale
has the same keys (`parity.test.ts`) without per-language plural categories. Format names are not translated.

### D4. Only the header is read, and one question is in flight

Review finding: `predict_blocking` read every queued file whole (`std::fs::read`) on every format change, and the hook
sent a new question per change without dropping the ones it superseded, so stepping through five formats with forty 30
MB photographs queued would read 6 GB. `rust-sak`'s `probe_file` and `icc_profile_file` choose the format by
extension, while `load` and the prediction go by content, so they would not give identical answers for a misnamed
file; instead `loss::header` reads a bounded prefix by content: a JPEG through its first `SOS` segment (every `APPn`
block a prediction reads - ICC, XMP, ISO 21496-1, MPF - lies before it, and `hdr_bytes` reads nothing after it), a PNG
through its first `IDAT` chunk header (no `iCCP` or `cICP` may follow it), in 64 KiB steps, reading on where a segment
or chunk runs past the step (a 16 MB profile is still read whole); any other format, and a JPEG or PNG whose structure
cannot be followed, whole, as before. In the Ultra HDR change a gain-map JPEG asked about as JPEG is read whole, since
its gain map lies after the primary. A test holds the bounded and the whole-file predictions equal for large and small
JPEGs and PNGs, a TIFF and a WebP, and the reviewer's inputs agree too.

In the window, `useExportLosses` keeps one question in flight: a question asked while one is unanswered waits, a later
one replaces it, and it is asked when the one in flight settles (answered or failed); a waiting question is dropped
when the dialog closes. A Tauri `invoke` cannot be cancelled once sent, so dropping what has not been sent is the
cancellation available; only the latest answer is shown, as before.

### D5. A profile the encoder could not write has its own words

`add-color-2-loss-warnings` D6 adds `ProfileCause::NotWritten`, known only after the write: it reaches a written row
through `Exported.losses`, never a prediction. `formatLoss` words it with `export.loss.profileNotWritten`
(`<profile> profile not embedded in <format>: the encoder could not write it`), added to all 14 locales. Plain sRGB
left out of AVIF or HEIF is no longer a loss (its D7), so the dialog no longer warns on every sRGB photograph queued
as AVIF or HEIC.

## Risks / Trade-offs

- [A slow disk makes the notice late] → it appears when the answer arrives; Save is not blocked.
- [Prediction and result disagree] → the row switches to the result after writing; both come from one assessment.

## Platforms

Validated by unit and component tests (Vitest, jsdom) and `cargo test` on macOS. **Not checked in a real window** in
this change: the notice and the icon's tooltip need a manual look in the Tauri app on macOS, Windows and Linux.
