> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I have not yet tested its application in
> practice.

## Why

A denoise, a generative restore (Kazan, Osaka) and a lens blur each leave smoother pixels than any capture of the same
scene: the photograph's own sensor noise or film grain is gone, so skin, sky and walls read as "plastic", and a
synthetic blur reads as painted where a real lens's blurred background still has its sensor's noise. Every major tool
that ships an AI restorer now offers grain back for this reason: Topaz Photo AI has a Grain enhancement described as a
finishing step after denoise, sharpening, upscaling or face recovery; Gigapixel added a Grain toggle under every model;
Magnific has "Smart Grain"; Krea's enhancer has grain strength and size. Lightroom's Grain is a separate manual Amount.
(Sources and the rest of the survey: `~/dev/opai-notes/research/restore-grain-2026-10-31.md`.)

What none of them does by default is **put back the grain the photograph had**, and only where it was taken away. A
fixed amount is too much on a clean photograph and too little on a grainy one; measuring the photograph's own noise per
brightness, and adding back only what the result lacks, tile by tile, is the realistic version, and it needs no model.

The first version of this change (`add-restore-grain`, 2026-10-30) made grain a parameter of Restore. Joseph asked for it
to be an enhancement of its own instead, last in the chain, so it also follows Upscale, Sharpen, Face recovery and the
lens blur. This change replaces that one.

Fork work. HOLD: no upstream issue or PR now.

## What Changes

Task estimate: **20**, one slice.

- **A Grain family**, `Family::Grain` (prefix `gr`), **last** in `Family::APPLY_ORDER`, after Lens Blur. One variant,
  Sensor (FP32, the arithmetic's: it runs no model, needs no download and opens no session). Never added by default,
  never suggested by Autopilot.
- **Two controls**, as Lightroom's Grain has an Amount: an **amount** (0 to 1, default 1) and a **Match source** switch
  (default on). Matching, the amount is the share of the missing grain put back; adding (switch off), it is a share of a
  sensor's high-ISO grain added everywhere.
- **Match source** compares the **photograph's** grain (per luminance band, at its own size, before any resample) with
  the **result's**, tile by tile, and adds the shortfall: a clean photograph gets nothing, a lens blur's subject that
  kept its noise gets nothing, and its blurred background gets the photograph's noise back.
- **The Denoise rule**: where the chain denoises, the photograph matched is what the denoise made, so the grain never
  undoes a denoise.
- **Sensor-like noise**, deterministic (a hash of the output's coordinates), at the output's pixel size; the HDR gain map
  is untouched; cache tag `gr-sensor-fp32-m1v1`.
- **The limiting mask does not limit grain**: on `integration`, a chain with a limiting mask is split before the
  trailing lens blurs and grain, and the grain runs over the blend.
- **CLI** `--grain` (`--gr`) with `--gr-amount <0-100>` and `--gr-add`; **GUI** Grain in the Add enhancement menu and its
  panel; strings in all 14 locales. The export edit record lists it.
- **Restore's grain goes**: `Restore.grain`, `--rs-grain`, the Restore panel's slider and the `-g` restore cache tag.
  `imaging::grain` stays the shared module (Remove's fill grain draws from its hash).

## Capabilities

### New Capabilities

- `grain`: what a grain run does, what it matches, and what it never changes.

### Modified Capabilities

- `cli-enhance`: `--grain`, `--gr-amount`, `--gr-add`.
- `gui-enhance`: the Grain enhancement and its panel.

## Impact

- `imaging`: `src/grain.rs` (`lacking` and `Lack`, tile by tile; `added_where`; `sensor`; `Profile::floored`).
- `opai`: `models/grain/{mod,amount,variant,pipeline}.rs`; `Family`, `Operation`, the catalogue, `build.rs`;
  `ImagePipeline::measures_against_photograph` / `run_against`; the chain driver keeps the photograph; Restore's grain
  removed; tests and `tests/public_api.rs`.
- `cli`: `args.rs`, `chain.rs`, `autopilot.rs`, `events.rs`, `look.rs`, `docs/CLI_REFERENCE.md`.
- `gui`: `frontend/features/enhancements/GrainOptions.tsx`, `EnhancementRow.tsx`, `lib/enhancements.ts`, the 14 locales,
  tests; the Restore panel's Grain slider removed.
- No model, no new dependency. A grained restore stored by the first version simply misses (its `-g` tag is gone); every
  other restore keeps its key.
