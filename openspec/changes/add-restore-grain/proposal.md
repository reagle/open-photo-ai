> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I have not yet tested its application in
> practice.

## Why

Restore's Kazan (PiSA-SR, a one-step diffusion restorer) redraws detail, and what it draws is clean: the photograph's
own sensor noise or film grain is gone from the result, so skin, sky and walls read as smooth, "plastic", next to what a
camera records. Every major tool that ships a generative or AI restorer now offers grain back for this reason: Topaz
Photo AI has a Grain enhancement described as a finishing step after denoise, sharpening, upscaling or face recovery;
Gigapixel added a Grain toggle under every model's settings because "generative models can sometimes look too smooth or
artificial"; Magnific's Skin Enhancer and Precise have a "Smart Grain"; Krea's enhancer has grain strength and size.
Lightroom's Denoise and Super Resolution do not add grain back; its Grain effect is a separate manual control. (Sources
and the rest of the survey: `~/dev/opai-notes/research/restore-grain-2026-10-31.md`.)

What none of them does by default is **put back the noise the photograph had**. A fixed amount is too much on a clean
photograph and too little on a grainy one. Measuring the source's own noise, per brightness, and adding back what the
restoration took away is the realistic version, and it needs no model.

Fork work, on the fork's Restore line (Kazan is fork-only). HOLD: no upstream issue or PR now.

## What Changes

Task estimate: **14**, one slice.

- **Restore gains a grain amount** (`grain`, 0 to 1, default 0 = off): the share of the noise the restoration removed
  that is put back. 1 matches the source.
- **Measured, not invented**: the source's and the restoration's fine residual (each pixel less its 3 x 3 mean) is
  measured robustly in eight luminance bands, on the restoration's input and on its output; the missing variance per band
  is added back, interpolated by each pixel's luminance (signal-dependent, as a Poisson-Gaussian sensor's is, and as
  AV1's film grain scales its noise by a piecewise-linear function of luma).
- **Sensor-like noise**: mostly luminance (one draw shared by the three channels as much as the source's own residuals
  are correlated), each channel at its own measured level, with a slight spatial correlation (a [1 2 1] kernel), at the
  **output's** pixel size.
- **Deterministic**: drawn from a hash of the output pixel's coordinates, so the same run gives the same pixels and the
  run cache is stable; the cache tag gains `-g<amount>` only when grain is on.
- **The gain map is untouched.**
- **A shared `imaging::grain`** (measure, synthesise) that Remove's fill-grain matching (fork Remove line) can move onto.
- **CLI**: `--rs-grain <0-100>` (percent). **GUI**: a Grain slider in the Restore panel, 0 to 100 percent, under
  Strength; strings in all 14 locales.

## Capabilities

### New Capabilities

- `restore-grain`: what the grain amount means, how it is measured and synthesised, and what it never changes.

### Modified Capabilities

- `cli-enhance`: `--rs-grain`.
- `gui-enhance`: the Grain slider in the Restore panel.

## Impact

- `imaging`: new `src/grain.rs` (`Profile`, `measured`, `added`, the coordinate hash), `lib.rs`.
- `opai`: `models/restore/{mod.rs,grain.rs}` (the `grain` field with a serde default, `Grained` wrapping the restorer's
  pipeline, the cache tag), `models/catalogue.rs` (the `grain` parameter), `models/build.rs` (`ParameterValues` grain),
  `models/grain.rs` (the bounded `Grain` amount), tests; `tests/public_api.rs`.
- `cli`: `args.rs` (`--rs-grain`), `chain.rs`, `docs/CLI_REFERENCE.md`.
- `gui`: `frontend/features/enhancements/RestoreOptions.tsx`, the 14 locales, tests.
- No model, no new dependency. Existing cache entries keep their keys (grain off is today's tag).
