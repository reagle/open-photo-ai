## 1. The shared file (`opai`)

- [x] 1.1 Add `crates/opai/src/settings.rs`: `SharedSettings` (processor, Autopilot exclusions, per-family `ModelChoice { codename, precision, position }`), `SharedSettings::from_gui` resolving `<codename>_<precision>` against the catalogue without splitting (D1), `FILE`/`VERSION`, `path()` via `rust_sak::fs::user_config_dir`, and `write_at` (temp file + persist, skipped when unchanged), entries kept in apply order; export from `lib.rs`; verify `cargo test -p opai settings` covers round trip, skip-unchanged and `from_gui` dropping unknown models
- [x] 1.2 Add `read_at(path) -> Loaded` (D2): absent is silent, unreadable/non-object/unknown version is `Ignored` with a `warn`, unknown keys skipped, an unresolvable processor/family/model/precision dropped with a `warn` while the rest is kept; verify tests for each case pass

## 2. The GUI mirror (`gui`)

- [x] 2.1 Add `crates/gui/src/gui_settings.rs` with `#[tauri::command] set_gui_settings` (traced with `command_span!`, IO on `task::spawn_blocking`, error tagged like `AnalyticsError`), register it in `lib.rs`; verify `cargo test -p gui` including `every_registered_command_opens_its_span`
- [x] 2.2 Add `frontend/ipc/guiSettings.ts` (`setGuiSettings`) with a test pinning the command name and argument shape; verify `pnpm test ipc/guiSettings`
- [x] 2.3 Add `frontend/lib/guiSettings.ts` `mirrorGuiSettings()` (boot + subscription on processor/models/autopilotExcluded, D3), call it from `main.tsx` and mock it in `main.test.tsx`; add tests: sent at boot, sent on a Save that changes one of the three, not sent for a background-only Save, rejection reported; verify `pnpm test lib/guiSettings main`

## 3. The command line follows it (`cli`)

- [x] 3.1 In `args.rs`, add `--no-gui-settings`, make `-q`/`-p` optional, and add `crates/cli/src/settings.rs` with `Source`, `Sourced`, `Qualities` and `Settings::resolve` (D4); `parse()` reads the file unless the flag is given, `parse_from` resolves with none; verify `cargo test -p cli args settings` covers each precedence row
- [x] 3.2 In `autopilot.rs`, make `AutopilotModels::read` take the file as the layer below the variables (exclusion → off, model → that model) and record each family's source; verify tests: file off, file model, variable over file, `off` variable over file model
- [x] 3.3 In `chain.rs` and `run.rs`, plan with per-family `Qualities`, and fall back to `auto` with a `warn` when a processor from the file is not offered (D4); verify `cargo test -p cli chain run`
- [x] 3.4 In `look.rs` and `run.rs`, print `mixed` and, under `--verbose`, the sources on the processor line and the autopilot line (D5); log which file was read; verify exact-text tests in `look`

## 4. Documentation

- [x] 4.1 In `docs/CLI_REFERENCE.md`, add `--no-gui-settings`, a "GUI settings" section (file, location per OS, precedence table, fallbacks), and note it under `-q`, `-p` and "Autopilot"; verify `cargo test -p cli every_example_in_the_reference_parses`

## 5. Verification

- [x] 5.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; verify all three are green
- [x] 5.2 Run `pnpm check`, `pnpm typecheck` and `pnpm test` in `crates/gui`; verify all are green
- [x] 5.3 Verify by hand on macOS, under a temporary `HOME` so the real configuration directory is untouched: write the file through `opai::settings` as the GUI would (SD everywhere, colour balance and colorization off), run `--ap --verbose` and see `quality SD (from GUI settings)` and `Color Balance off (from GUI settings)`; `-q hd` and `OPAI_AP_COLOR=rio` override; `--no-gui-settings` and a corrupt file give the defaults with a log warning
