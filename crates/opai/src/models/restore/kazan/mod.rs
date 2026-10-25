//! Kazan: a generative restorer — PiSA-SR, a one-step diffusion restorer on Stable Diffusion 2.1 — and the tile
//! geometry, range and execution-provider profile it runs under. The restoration it runs is the shared
//! [`filter`](crate::models::filter), with no guard, over the photograph [resampled](super::resample) to the size the
//! run asked for.
//!
//! **Generative, not faithful.** Moscow, Petersburg and Novgorod estimate the sharp photograph; Kazan redraws
//! plausible detail where the blur destroyed it. On a soft face that is the difference between a slightly crisper
//! blur and eyelashes. On lettering or anything the model cannot recognise it is invented detail, and the strength
//! slider is the only fidelity control.
//!
//! **Enlarging is resampling first.** PiSA-SR is trained on photographs bicubically enlarged to the target size and
//! restores at that size, as Osaka does; nothing in the graph changes the resolution. So a 2x run is a Lanczos3
//! resample to twice the size, then the same tiles at the new size — four times as many of them.
//!
//! The graph is the whole of one tile, built so the shared filter can drive it unchanged: `[1,3,512,512]` in `[0,1]`
//! in, the same out. Inside: the VAE encoder (latent mean, so a tile always restores the same way), the UNet with both
//! of PiSA-SR's LoRAs merged at their default weights and the empty-prompt embedding folded in as a constant, the
//! one-step update at timestep 1, the VAE decoder, and the tile's colour matched back to its input's per-channel mean
//! and deviation. Exported by `pisasr_export.py` (fork notes), not published upstream.

// Deleting the model is deleting this directory plus the arms in `RestoreVariant` that name it.

use crate::models::precision::Precision;
use crate::providers::profile::{EpProfile, cpu_and_gpu_at_fp16};
use imaging::TileGeometry;
use imaging::tensor::Normalisation;

/// The tile this model's graph is exported at, and how far tiles overlap.
///
/// 512 is the resolution Stable Diffusion 2.1-base was trained at; at the default 256 the UNet sees a 32x32 latent
/// and has too little context to recognise what it is redrawing. The overlap is wider than the default's 16 because
/// two tiles of a generative model disagree more about the pixels they share than two tiles of a convolutional one.
pub(crate) const GEOMETRY: TileGeometry = TileGeometry { size: 512, overlap: 64 };

// Pinned as a literal and asserted in the family's tests, for the reason sharpen's `RANGE` gives: feeding a graph the
// wrong range is not an error a runtime reports, it is a worse photograph.
/// The range the graph reads and writes: `[0, 1]`. The `[-1, 1]` the VAE wants is applied inside it.
pub(crate) const RANGE: Normalisation = Normalisation::Unit;

/// The execution-provider tuning for this model, at the precision it carries: CoreML off the Neural Engine at FP16,
/// and the provider defaults at FP32.
pub(crate) fn profile(precision: Precision) -> EpProfile {
    // A UNet's attention blocks are the same layer-normalisation, reshape and transpose around a matmul that keep the
    // Restormer models off the Neural Engine; see `moscow`. Not yet measured through this project's own sessions.
    cpu_and_gpu_at_fp16(precision)
}
