## 1. Arguments

- [x] 1.1 Add `--gui` to `Args` after `--output`, make `--output` an `Option<PathBuf>` with `required_unless_present = "gui"` read through `Args::output()`, add a second usage line `opai --gui <INPUT>... [-r]`, and skip the at-least-one-enhancement check under `--gui`; verify `opai --gui a.jpg` parses and `opai a.jpg --up` still requires `-o`
- [x] 1.2 Parse through `ArgMatches` and collect into `Args::ignored` every option whose value source is the command line, except the inputs, `-r` and `--gui`, by long flag in the order given (design.md - D4); add tests for the collected list, `-r` not counted, and nothing collected without `--gui`; verify `cargo test -p cli args` is green

## 2. Handover

- [x] 2.1 Add `src/gui.rs`: warning, expansion through `inputs::expand`, absolute paths, the macOS `open -b` command from a pure function (D2), the detached binary on Windows and Linux (D3), the confirmation; add tests for the warning text, the confirmation, the macOS command line, the binary search and the bundle id and binary name against `tauri.conf.json`; verify `cargo test -p cli gui` is green
- [x] 2.2 Call it from `main` before telemetry and the runtime (D1), with a `CliError::Launch` (exit 1) and a `⚠️` warning line in `look.rs` (D6); verify a missing input under `--gui` prints the warning, then the failure, and exits 2

## 3. Documentation and checks

- [x] 3.1 Document `--gui` in `docs/CLI_REFERENCE.md` (synopsis, rules, quick start, its own section, one instance at a time, exit codes, common errors); verify `every_example_in_the_reference_parses` is green
- [x] 3.2 Run `cargo fmt --check`, `cargo clippy -p cli --all-targets -- -D warnings` and `cargo test -p cli`; verify all are green
- [ ] 3.3 Verify by hand on macOS with the GUI installed: GUI closed, `opai photos -r --gui` starts it and opens every image; GUI open, `opai a.jpg --gui` adds it to the window without a second instance; with extra options the warning names them; record which of Windows and Linux were tried
- [ ] 3.4 Add the by-hand cases to `~/dev/opai-notes/TESTING.md`; verify the checklist lists them

## Implementation notes

Bookmark `cli-gui-open`, one commit on `gui-macos-open-with`, not pushed.

- 2.1: the Windows and Linux branch could not be cross-compiled here (C dependencies need a cross toolchain); its
  functions were checked by compiling them alone with `rustc --target` for `x86_64-unknown-linux-gnu` and
  `x86_64-pc-windows-msvc`.
- 2.2: refused inputs keep a run's message, `no image to enhance in …`, rather than a `--gui` wording.
- 3.2: `cargo test -p cli` 197 passed.
