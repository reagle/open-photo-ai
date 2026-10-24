//! Kazan: a generative sharpener — PiSA-SR, a one-step diffusion restorer on Stable Diffusion 2.1 — and the tile
//! geometry and execution-provider profile it runs under. The sharpening it runs is the shared
//! [`filter`](crate::models::filter), with no guard.
//!
//! **Generative, not faithful.** Moscow, Petersburg and Novgorod estimate the sharp photograph; Kazan redraws
//! plausible detail where the blur destroyed it. On a soft face that is the difference between a slightly crisper
//! blur and eyelashes. On lettering or anything the model cannot recognise it is invented detail, and the strength
//! slider is the only fidelity control.
//!
//! The graph is the whole of one tile, built so the shared filter can drive it unchanged: `[1,3,512,512]` in `[0,1]`
//! in, the same out. Inside: the VAE encoder (latent mean, so a tile always restores the same way), the UNet with both
//! of PiSA-SR's LoRAs merged at their default weights and the empty-prompt embedding folded in as a constant, the
//! one-step update at timestep 1, the VAE decoder, and the tile's colour matched back to its input's per-channel mean
//! and deviation. Exported by `pisasr_export.py` (fork notes), not published upstream.

// Deleting the model is deleting this directory plus the arms in `SharpenVariant` that name it.

use crate::models::precision::Precision;
use crate::providers::profile::{EpProfile, cpu_and_gpu_at_fp16};
use imaging::TileGeometry;

/// The tile this model's graph is exported at, and how far tiles overlap.
///
/// 512 is the resolution Stable Diffusion 2.1-base was trained at; at the default 256 the UNet sees a 32x32 latent
/// and has too little context to recognise what it is redrawing. The overlap is wider than the default's 16 because
/// two tiles of a generative model disagree more about the pixels they share than two tiles of a convolutional one.
pub(crate) const GEOMETRY: TileGeometry = TileGeometry { size: 512, overlap: 64 };

/// The execution-provider tuning for this model, at the precision it carries: CoreML off the Neural Engine at FP16,
/// and the provider defaults at FP32.
pub(crate) fn profile(precision: Precision) -> EpProfile {
    // A UNet's attention blocks are the same layer-normalisation, reshape and transpose around a matmul that keep the
    // Restormer models off the Neural Engine; see `moscow`. Not yet measured through this project's own sessions.
    cpu_and_gpu_at_fp16(precision)
}
