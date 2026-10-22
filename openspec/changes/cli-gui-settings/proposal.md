> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

A user who has set the GUI up the way they like it (SD models, a preferred upscale model, colour balance and
colorization switched off for Autopilot, a processor) gets none of that from `opai` on the command line: every run
starts from the built-in defaults, and the only ways to change them are flags on every call or the seven `OPAI_AP_*`
variables in the shell. The GUI's preferences live only in the webview's `localStorage`, which no other process can
read, and `opai` has no settings store. The `add-cli-autopilot` change named that gap explicitly (its design.md D1
keeps the model choice in `cli` "until `opai` has a settings store").

This change gives the command line one read-only view of the GUI's saved choices, limited to settings the command line
already has flags or variables for, so the two front ends agree without either one gaining a new setting.

## What Changes

Task estimate: **13**. One change, not a series. It stacks on `cli-report-enhancements` (it extends the `--verbose`
output that change adds) and on `cli-output-default`.

**Cross-front-end behaviour that upstream should agree to before a PR.** This is the first time a front end publishes
its preferences for the other one. The draft issue is `opai-notes/issue-cli-gui-settings.md`.

**In `opai`:**

- A small, versioned settings file, `gui-settings.json`, in `opai`'s configuration directory (beside `analytics.json`
  and the log): the processor, the families Autopilot may not suggest, and each family's chosen model and precision.
  The library owns its format, its writer (atomic, skipped when unchanged) and its reader. The reader never fails: a
  missing file is "no settings", and an unreadable file, an unknown version, or an entry naming something the library
  does not publish is ignored with a log warning, entry by entry where it can be.

**In `gui`:**

- The window mirrors the three preferences into the file through a new Tauri command, once at launch and on every Save
  that changes one of them, exactly as it mirrors the Analytics choice today. **The GUI never reads the file**:
  `localStorage` stays its only source of truth, so nothing the GUI shows or does changes. A failed write is logged
  and reported like the analytics mirror's, never shown as an error dialog.

**In `cli`, observable on the command line:**

- **The command follows the GUI's saved settings.** Precedence, highest first: a command-line flag, an `OPAI_AP_*`
  variable, the GUI's settings file, the built-in default.
  - **Processor:** `-p` > GUI > `auto`. A processor from the file that this build cannot name, or this machine does
    not offer, falls back to `auto` with a log warning instead of refusing the run.
  - **Quality, per family:** `-q` (every family) > the precision of the family's model chosen in the GUI (its HD or SD
    tier) > HD. The GUI chooses a precision per family, so the families of one run can differ; the processor line then
    reads `quality mixed`.
  - **Autopilot's model per family, and families switched off:** `OPAI_AP_*` > GUI (its model, or off where the family
    is excluded from Autopilot) > the bare flag's default model.
- **`--no-gui-settings`** ignores the file for one run.
- **Under `--verbose`**, the processor line names where the processor and the quality came from (for example
  `quality SD (from GUI settings)`), and under `--ap` a second line names each family's model or `off` and its source.
- **Docs:** `docs/CLI_REFERENCE.md` gains a "GUI settings" section and the new flag.

**Unchanged:** a run on a machine where the GUI has never run (or has not run since this change) behaves exactly as
today. The bare enhancement flags (`--up`, `--dn`, …) keep their built-in default models; only `--ap` takes models
from the GUI. No setting is added to the GUI.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `cli-enhance`: a new requirement, "The command follows the GUI's saved settings" (the file, the precedence, the
  fallbacks, `--no-gui-settings`); "Quality selects the precision each model runs at" becomes per family when the file
  says so; "The processor is chosen on the command line" adds the file as a source; "Autopilot chooses each image's
  enhancements" adds the GUI's models and exclusions below the variables; "Progress is shown while the command works"
  adds `mixed` and the `--verbose` sources. The last two are modified on top of `cli-report-enhancements`' versions.
- `gui-settings`: a new requirement, "The settings the command line can follow are published for it".

## Impact

**Crates touched:** `opai` (a new `settings` module and its exports), `gui` (one Tauri command, a frontend mirror and
its IPC binding), `cli` (`args.rs`, `autopilot.rs`, `chain.rs`, `look.rs`, `run.rs`, a new `settings.rs`), and
`docs/CLI_REFERENCE.md`.

**Why the file's logic is in `opai`:** both front ends need it (the GUI writes, the CLI reads), so the format, the
resolution of a stored `<codename>_<precision>` against the catalogue, and the lenient reader live in one place. What
stays in each front end is front-end vocabulary: the GUI decides *when* to mirror (TypeScript, beside the analytics
mirror), and the CLI decides precedence against its own flags and variables.

**`rust-sak`:** `fs::user_config_dir` resolves the file's path without creating anything, and `fs::mk_temp_file_in`
gives the atomic write, as the analytics mirror already does. Nothing is missing.

**Third-party crates introduced:** none (`serde_json` is already a dependency of `opai`).

**Breaking:** none for a user without the file. A user who has saved settings in the GUI since this change will see
the CLI follow them; `--no-gui-settings` restores the built-in defaults for one run.

**Merge points with the shortcuts series** (`add-gui-shortcuts-*`, not in this stack): it adds a field to
`SettingsData` in `stores/settings.ts` and a line to `main.tsx`; this change only reads the store and adds one line to
`main.tsx`, so the expected conflict is that one import/call pair.
