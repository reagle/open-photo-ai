## 1. Slice 1: Open Recent (`add-gui-open-recent`)

- [x] 1.1 Add `src/recent.rs`: the `RecentList` (D2: ten, newest first, by path, a batch in its order), the state file (D1: `session.json`, version 1, atomic write, absent/corrupt/other version read as empty and logged), application state loaded at setup, and the commands `recent_images`, `note_recent`, `check_recent`, `clear_recent`; add tests for the round trip, a corrupt file, another version, a missing photograph, the cap and order, and an unchanged write skipped; verify `cargo test -p gui recent` is green
- [x] 1.2 In `src/menu.rs` add File > Open Recent (D4): the submenu, its entries rebuilt from the list, file-name labels disambiguated by folder, Clear Menu unavailable while empty, `recent:<n>` resolved to its path and forwarded as `open-recent`, Clear Menu acted on in Rust, `recent-changed` on every change, and the two labels under `openRecent` and `clearRecent`; add tests for the labels, the forwarding and the plan; verify `cargo test -p gui menu` is green
- [x] 1.3 Frontend: `ipc/recent.ts`, `stores/recent.ts`, `lib/recent.ts` (`watchRecent` noting what enters the gallery - D3 - and `openRecent` - D5), `open-recent` in `hooks/useAppActions.ts`, `"recent"` as a `files_added` source; add tests; verify `pnpm test` is green
- [x] 1.4 Windows and Linux: the Open Recent button beside Add images (D7); add tests for both platforms; verify `pnpm test features/drawer` is green
- [x] 1.5 Add the i18n keys (two menu labels, the button, its empty state, the missing notice) to all 14 locale files; verify the locale parity test is green
- [x] 1.6 Run `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p gui`, and in `crates/gui` `pnpm test`, `pnpm typecheck`, `pnpm lint`; verify all are green
- [ ] 1.7 Verify by hand on macOS (`~/dev/opai-notes/session-restore/README.md`)

## 2. Slice 2: Reopen last session (proposal only, pending Joseph's decision)

- [ ] 2.1 The "before closing" hook in `stores/files.ts` and the capture of a closing photograph's edits into its recent entry (D9)
- [ ] 2.2 The `session` member (D8), its debounced and closing saves, and the Settings switch with i18n in 14 locales
- [ ] 2.3 `restore_session` (D10): missing photographs counted, identity mismatch dropping the edits, the window writing the four stores; Open Recent applying a matching entry's edits
- [ ] 2.4 Tests: Rust (missing, identity mismatch, session round trip) and vitest (restore populates the gallery and stacks, only the selected photograph runs, the switch)
- [ ] 2.5 Verify by hand

## Implementation notes and deviations

Slice 1 implemented on bookmark `add-gui-open-recent` (on `integration`: this proposal, then one feature commit).

- The commands are `recent_images`, `note_recent`, `check_recent` and `clear_recent` (task 1.1 named them before they were written); the macOS Clear Menu is acted on in Rust (`recent::clear`), the button's through `clear_recent`.
- No identity-mismatch test yet: slice 1 records the identity and never compares it (D6); the test belongs to slice 2 (task 2.4).
- Not bundled: `just build gui` copies the bundle out of this workspace's own `target/`, so it needs a release build there, which the agent was asked not to make (disk); checked by `cargo test`, clippy and the frontend suite only. Joseph's `just build-fork gui` in the `integration` workspace bundles it once this bookmark is merged there.
