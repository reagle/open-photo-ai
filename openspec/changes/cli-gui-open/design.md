## Context

See proposal.md - Why. These facts were checked in the code and shape the design:

- **The single-instance claim is shared.** `opai::instance` refuses a run while the GUI holds it, and `Opai::initialize`
  takes it. A handover that went through `run::run` would be refused exactly when it is most useful.
- **Telemetry starts in `main` right after parsing** (`analytics::begin`). A handover is not a run and should not count
  as one.
- **`gui-macos-open-with` already receives files by every route**: `RunEvent::Opened` on macOS, at launch and while
  running; the first launch's `argv`; and a second launch's arguments, resolved against its working directory, through
  `tauri-plugin-single-instance`. It accepts only existing files (`opened::paths_from_args`), so directories must be
  expanded by the CLI.
- **The bundle is `io.vinicius.opai`**, `Open Photo AI.app`, main binary `OpenPhotoAI` (`crates/gui/tauri.conf.json`).
- **`inputs::expand` is what a run uses** to turn inputs into images, with `-r`, the extension filter and the probe; it
  refuses an unreadable input and an empty result with a usage error (exit 2).
- **The CLI's messages are English strings** in `look.rs` and `error.rs`; nothing is localised.

## Goals / Non-Goals

**Goals:**

- `opai photos -r --gui` opens every image under `photos` in the window, whether or not it is running.
- The same inputs a run would enhance, no more and no fewer.
- An option the user gave is never silently dropped.

**Non-Goals:**

- Carrying settings to the GUI (future work, see proposal.md).
- Waiting for the GUI to finish opening the photographs, or reporting which it refused.
- A separate `opai gui` subcommand.

## Decisions

### D1: A flag, handled in `main` before telemetry and the library

`args.gui` is checked right after `args::parse()`, before `analytics::begin` and the runtime; `gui::open` is
synchronous and needs neither. Alternative: a branch in `run::run`, rejected because `run` is built around one `Opai`
handle and the update check.

### D2: macOS hands over through Launch Services, `open -b io.vinicius.opai <paths>`

The bundle identifier, not a path, finds the application wherever it is installed. `open` sends an "open documents"
Apple Event, which `RunEvent::Opened` receives both when the application is launched for it and when it is already
running. No `--args`: those reach only a fresh launch, and would duplicate the event's files. `open` returns once the
event is sent; its exit status and stderr are the failure message. The command is built by a pure function so its
arguments are tested without launching anything.

### D3: Windows and Linux start the binary detached

The binary `OpenPhotoAI` (`.exe` on Windows) is looked for beside the running `opai`, then on `PATH`; not found is a
clear error naming both places. It is spawned with the absolute paths as arguments and null standard streams, in its
own process group (Unix) or detached without a console (Windows), and not waited for: a running instance's
single-instance plugin receives the arguments and the new process exits; otherwise the new process opens them. Only
compiled here (no Windows or Linux machine), so it stays small and `cfg`-gated.

### D4: "Given" means written on the command line

The ignored options are read from clap's `ArgMatches`: every argument whose `value_source` is `CommandLine`, except the
inputs, `-r` and `--gui` itself, named by its long flag, in the order given. A default or an environment variable is
not something the user asked this command for. `parse_from` therefore parses through `try_get_matches_from` and
`from_arg_matches` instead of `try_parse_from`, and stores the list in `Args::ignored`.

### D5: Clap's own conflicts still apply

`--ap` with an enhancement flag, or `--scale` without `--up`, is still refused under `--gui`: clap checks them before
the code sees `--gui`, and relaxing them would need a second parse. Rare, and the refusal is accurate.

### D6: The warning is printed before the inputs are expanded

So a user who mistyped an input still learns that the options would have been ignored. It is a new `⚠️` line in
`look.rs`, yellow and not bold, under the same `NO_COLOR` rule as every other line.

## Risks / Trade-offs

- **The GUI is not installed or never opened** on macOS: `open` fails with "Unable to find application with bundle
  identifier", passed through. Mitigation: the message asks whether it is installed.
- **An application built from source but not registered** with Launch Services is not found by `-b`. Opening it once
  registers it.
- **The confirmation means "handed over", not "opened"**: an image the GUI refuses is reported by the GUI, not here.
- **`--output` becomes an `Option`**: every reader goes through `Args::output()`, which clap guarantees outside
  `--gui`.
