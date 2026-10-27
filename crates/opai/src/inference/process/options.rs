//! What a caller may say about a run beyond the picture and the operations.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::inference::depth::OutputDepth;
use crate::inference::faces::FaceSelection;
use crate::inference::progress::OnInference;
use crate::mask::Mask;
use crate::providers::ExecutionProvider;

/// Everything about a run that has a sensible default.
///
/// Passed as an `Option` to [`Opai::process`](crate::Opai::process): only the picture and the operations have no
/// sensible default, and a caller that wants "just enhance this" passes `None` rather than four values it has no
/// opinion about.
///
/// `..Default::default()` is the documented way to set one field:
///
/// ```
/// # use opai::{OutputDepth, ProcessOptions};
/// let options = ProcessOptions { depth: OutputDepth::Source, ..Default::default() };
/// ```
///
/// Written that way, a field added later is source-compatible for every caller.
#[derive(Clone)]
pub struct ProcessOptions {
    // An `Option` of a bundle is the same idiom `image::save` already uses for its encode options. A bundle rather than
    // five more parameters, so that a field added later is source-compatible for every caller.
    //
    // Deliberately **not** `#[non_exhaustive]`, unlike this crate's public enums: that attribute bars an external crate
    // from struct-literal syntax altogether, including the `..Default::default()` form above — which is the very thing
    // that makes a field added later source-compatible. A `Default` plus that idiom gives the compatibility the
    // attribute would be reached for, and leaves the idiom usable.
    /// What to run on. Defaults to [`ExecutionProvider::Auto`], which resolves to the best this machine supports and,
    /// where a provider throws an error opening or running the model, falls back to the next best — the CPU last.
    /// Any other provider falls back to the CPU where it cannot open the model.
    pub provider: ExecutionProvider,

    /// How many bits per channel the result carries. Defaults to [`OutputDepth::Eight`] — see that type for why, and
    /// for why a front end's own default should be the other one.
    pub depth: OutputDepth,

    /// Where progress reports go. Defaults to none, and a caller that asks for none is charged for none.
    pub on_progress: Option<OnInference>,

    /// The token that stops the run. Defaults to a fresh one nobody holds, so a caller with no cancel button does not
    /// have to make one.
    ///
    /// Borrowed from the caller in spirit though owned here: pass a clone of the token you kept, and cancelling yours
    /// cancels this run. A child token under one parent is how a front end stops every image of an export queue from
    /// one gesture.
    pub cancel: CancellationToken,

    // `false` serves the two callers that need it rather than merely prefer it: one measuring what a model costs —
    // which would otherwise measure a read, or charge the storing of a result to the model — and one processing images
    // it must not persist.
    //
    // A `bool` rather than an enum because there is no third mode anyone has asked for, and per run rather than a
    // setter on the application because a run cannot see a hidden global's value — the reference has to bind its own
    // to a local for that reason.
    /// Whether this run may read from and write to the run cache. Defaults to `true`.
    ///
    /// `false` computes every operation, returns exactly what a cached run would have returned, and leaves **nothing**
    /// behind.
    ///
    /// **Decided once for the whole run**, not per operation, so a run cannot read from the cache and then decline to
    /// write back to it.
    ///
    /// It is a different question from [`Opai::cache_mode`](crate::Opai::cache_mode), which reports what is *backing*
    /// the cache: a run with this set to `false` still reads `Disk` there.
    pub cache: bool,

    /// Where the chain's face recoveries get their faces from. Defaults to [`FaceSelection::AsGiven`]: the faces each
    /// one was built with.
    ///
    /// [`FaceSelection::Detect`] runs one face-recovery detection over the source before the chain — the one the GUI's
    /// picker and Autopilot run, so a photograph either already examined is served from the store — under this run's
    /// provider, stop and cache. Its progress is reported on the same bar as the chain, which moves forward once: every
    /// operation owns an equal `1 / N` of it and the detection takes the first fifth of the first operation's slot
    /// (none, when the store already knew the faces).
    pub faces: FaceSelection,

    // An `Arc` because a batch export hands one mask to every photograph's run and these options are `Clone`: a mask
    // from a file holds a plane the size of that file.
    /// The part of the photograph the enhancements are for. Defaults to none: the result is the chain's everywhere.
    ///
    /// With a mask, the chain still runs over the whole frame, and its result is then blended with the photograph it
    /// started from, `source + w (result - source)` per pixel: the result where the mask is white, the photograph where
    /// it is black. A chain that changed the size (an upscale) is blended with a Lanczos3 enlargement of the
    /// photograph, so the masked-out part is a plain enlargement. A result that is not the photograph's shape is an
    /// error ([`InferenceError::MaskShape`](crate::InferenceError::MaskShape)), not an unmasked picture.
    ///
    /// **No step's cache key includes the mask**, so a run that differs from an earlier one only in its mask runs no
    /// model: it serves every step from the store and redoes the blend. The result's identity does include it. An
    /// [empty](Mask::is_empty) mask, and a mask over an empty chain, are no mask at all, identity included.
    pub mask: Option<Arc<Mask>>,
}

impl std::fmt::Debug for ProcessOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Written out rather than derived: a callback has nothing to print, and the fact a reader of this actually
        // wants is whether one was registered at all.
        f.debug_struct("ProcessOptions")
            .field("provider", &self.provider)
            .field("depth", &self.depth)
            .field("on_progress", &self.on_progress.is_some())
            .field("cancel", &self.cancel)
            .field("cache", &self.cache)
            .field("faces", &self.faces)
            // The signature rather than the strokes, which can run to thousands of points, or a base's millions.
            .field("mask", &self.mask.as_ref().map(|mask| format!("{:016x}", mask.signature())))
            .finish()
    }
}

impl Default for ProcessOptions {
    fn default() -> Self {
        Self {
            provider: ExecutionProvider::Auto,
            depth: OutputDepth::default(),
            on_progress: None,
            cancel: CancellationToken::new(),
            // On, because the whole point of the cache is that a user who re-runs an enhancement does not pay for it
            // twice, and a default of `false` would mean every caller had to know to ask for that.
            cache: true,
            faces: FaceSelection::AsGiven,
            mask: None,
        }
    }
}
