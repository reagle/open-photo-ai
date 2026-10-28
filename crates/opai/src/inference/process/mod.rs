//! Running an ordered chain of operations over one image: what is refused, what each one runs, and in what order.
//!
//! [`fn@plan`] decides what a chain will do and refuses what it cannot serve; [`run`] carries it out;
//! [`identity_after`] names what came out. [`process`] is the three of them together, after finding the faces a chain's
//! face recoveries restore where it was asked to, and is the whole body of [`Opai::process`](crate::Opai::process).

// What a run leaves in the log. `process` brackets the run with a pair of records at `info` — what it was asked to
// apply, the source's identity and the resolved depth on the way in; the elapsed time and whether it succeeded on the
// way out — so an ordinary session's file is an account of what the user did. The bracket is here rather than on
// `Opai::process` because this is where the operations, the depth and the cache handle are all in scope together, and
// because a record at both would be two answers to "when did the run start".
//
// Inside `run`, every per-operation decision is at `debug`: a step served from the store, a step computed, and the
// pass records one layer down. That level is the bound the log file's size rests on — the loop here is per
// *operation*, and the tile or region loop is two layers below, inside the pipeline, where nothing records anything.
// Progress is the callback that reports at that granularity; the log is not a second progress bar. So the same
// enhancement over a 2-megapixel photograph and over a 100-megapixel one writes the same number of lines.
//
// The levels follow `logging`'s rule: a refused chain, a stored prefix that would not decode and a failed run are each
// one `warn`, and a stopped run closes at `info`.
//
// No model and no contract is named here — not a name, not a module path, not a `use`. What an operation installs,
// opens, runs and counts as a stage is asked of `ImagePipeline`, and which pipeline an operation reaches is answered by
// `Operation::pipeline` — one match per family in `models/operation.rs`, and one match per contract inside the family.
// A ninth family, a fifth upscale model or a third contract changes nothing in this file; the refusals are made at
// those two seams, still over the whole chain before anything is installed.

use std::sync::Arc;

use image::DynamicImage;
use image::imageops::FilterType;
use rust_sak::memo::Memo;
use tokio_util::sync::CancellationToken;

use std::future::Future;

use super::acquire;
use super::execute::{ExecuteOptions, execute};
use super::faces::{ChainFaces, FaceSelection, FacedChain, find_chain_faces};
use super::handover::Handover;
use super::progress::{ChainProgress, OnInference};
use crate::cache::RunCache;
use crate::error::InferenceError;
use crate::image::Picture;
use crate::mask::Mask;
use crate::models::remove::RemovedAt;
use crate::models::{Detection, Faces, Family, Operation, Remove};
use crate::pipeline::{Backend, Shared};
use crate::providers::ExecutionProvider;
use crate::sessions::SessionHandle;
use crate::task::spawn_blocking;
use crate::telemetry::metrics;
use crate::telemetry::unit::{self, Outcome, Unit, unit_span};
use imaging::ChannelDepth;
use imaging::mix;
use imaging::tensor::{Channel as _, Sampler, flattened};
use tracing::Instrument as _;

mod identity;
mod options;
mod plan;
mod report;

pub(crate) use identity::{identity_after, identity_masked, identity_of_data};
pub use options::ProcessOptions;
pub(crate) use plan::{Planned, plan};
pub use report::{Enhanced, ProviderReport, ProviderVerdict};

/// `operations` in [`Family::APPLY_ORDER`](crate::models::Family::APPLY_ORDER): borrowed where they already are, and
/// otherwise a stably sorted copy, so two operations of one family keep the order they were given in.
///
/// **Sorted rather than refused.** Every chain has exactly one order that is right — a face recovery's faces are in the
/// chain input's coordinates, so it must precede an upscale, and every other pairing has a better order — and a caller
/// that listed its stack another way asked for that result, not for an error. Putting the rule here rather than in each
/// front end is what keeps two front ends from running one stack two ways.
pub(crate) fn in_apply_order(operations: &[Operation]) -> std::borrow::Cow<'_, [Operation]> {
    let position = |operation: &Operation| operation.family().apply_rank();

    if operations.is_sorted_by_key(position) {
        return std::borrow::Cow::Borrowed(operations);
    }

    let mut sorted = operations.to_vec();
    sorted.sort_by_key(position);

    std::borrow::Cow::Owned(sorted)
}

/// Runs `source` through `operations` and composes the identity of what came out.
///
/// The body of [`Opai::process`](crate::Opai::process).
///
/// `cache` is the application's store, or `None` where it has none. Whether this run actually uses it is
/// [`ProcessOptions::cache`].
pub(crate) async fn process<B: Backend>(
    backend: &B,
    cache: Option<&Memo>,
    source: &Picture,
    operations: &[Operation],
    options: Option<ProcessOptions>,
) -> Result<Enhanced, InferenceError> {
    process_with(backend, cache, source, operations, options, |options| async move {
        execute(backend, cache, source, &Detection::for_face_recovery(), options)
            .await
            .map(|executed| executed.value)
    })
    .await
}

/// [`process`] over any face detection: `detect` is called at most once, and only where
/// [`ProcessOptions::faces`] asks for one and the chain carries a face recovery.
pub(crate) async fn process_with<B, D, F>(
    backend: &B,
    cache: Option<&Memo>,
    source: &Picture,
    operations: &[Operation],
    options: Option<ProcessOptions>,
    detect: D,
) -> Result<Enhanced, InferenceError>
where
    B: Backend,
    D: FnOnce(Option<ExecuteOptions>) -> F,
    F: Future<Output = Result<Faces, InferenceError>>,
{
    let mut options = options.unwrap_or_default();

    // Before anything else is asked of the chain: a removal of nothing is no removal, so nothing is installed for it,
    // no faces are looked for on its account, and the result is named as the chain without it.
    let operations = without_empty_removals(operations);

    // Before the plan, so the faces each recovery carries are part of every cache key the run derives. A stop here
    // ends the call before the run's opening record, as a stop before `process` would.
    let (operations, faces) = match std::mem::take(&mut options.faces) {
        FaceSelection::AsGiven => (operations, ChainFaces::NotAsked),
        FaceSelection::Detect(select) => {
            let handover = options.on_progress.as_ref().map(|on_progress| Handover::new(on_progress, &operations));
            let detection = ExecuteOptions {
                provider: options.provider,
                on_progress: handover.as_ref().map(Handover::detection),
                cancel: options.cancel.clone(),
                cache: options.cache,
            };

            let FacedChain { operations, faces } =
                find_chain_faces(operations.to_vec(), |index, face| select(index, face), Some(detection), detect)
                    .await?;

            // Asked for once the detection has ended, when whether it reported anything is known.
            if let Some(handover) = &handover {
                options.on_progress = Some(handover.chain());
            }

            (std::borrow::Cow::Owned(operations), faces)
        }
    };

    // Faces found on the source include any a removal in the chain is about to take out; a face recovery would paste a
    // face over the fill. They are dropped from every recovery's selection before the plan, so the keys say so.
    let (width, height) = source.dimensions();
    let operations = sparing_removed_faces(operations, width, height);

    // Before everything else — the plan, the identity, every cache key and every report — so all of them describe the
    // chain that actually runs. A chain already in order is borrowed as it is, so its cache keys are the ones it has
    // always had.
    let operations = &*in_apply_order(&operations);

    // The removals lead the chain (`Family::APPLY_ORDER`), and a limiting mask is blended with the photograph *after*
    // them, so it limits the enhancements and never brings back what was removed. A mask over a chain of removals
    // alone, or over an empty one, has nothing to limit: the result is named and made as with no mask.
    let removals = operations.iter().take_while(|operation| operation.family() == Family::Remove).count();
    let mask = options.mask.take().filter(|mask| !mask.is_empty() && operations.len() > removals);

    // Resolved **here**, once, and handed to the run, to the identity composition and to every cache key the run
    // derives. Resolving it twice — once for what runs and once for what the result is named — is two answers to one
    // question, and the identity is only true of the pixels while they agree.
    //
    // Resolving per operation would work, each operation's input being the previous one's output, but only by
    // accident, and it would put the depth of a result at the mercy of how many enhancements a user happened to
    // select.
    let depth = options.depth.resolve(source.pixels());

    // Before the opening record, so a refused chain is recorded once — as refused, by `plan` — and never as a run
    // that began and failed. It is a scan of values already in memory, so nothing is spent before the refusal.
    let planned = plan::<B>(operations)?;

    let ids = operation_ids(operations);
    let span = unit_span!(
        "enhancement",
        ids = %ids,
        identity = source.identity(),
        provider = %options.provider,
        depth = depth.tag()
    );

    // Scoped rather than held: an entered span cannot be held across the awaits below.
    let cache = span.in_scope(|| {
        let cache = RunCache::for_run(cache, options.cache, source);

        // The identity rather than the path: it is what the cache keys are folded from and what names the result, so
        // it is the one value that ties these two records to every cache record of the same run — and to a second run
        // over the same photograph an hour later. A source's *path* is a place the user chose and is deliberately not
        // here.
        tracing::info!(
            operations = operations.len(),
            ids = %ids,
            identity = source.identity(),
            depth = depth.tag(),
            provider = %options.provider,
            "enhancement started"
        );

        // Here rather than in [`cache`](crate::cache), because a run with no store never builds a `RunCache` for that
        // module to record from — and once per run rather than once per operation, which is the same volume rule the
        // per-step records below are held to. `debug`, since a caller that turned the cache off asked for exactly
        // this, and a machine whose store would not open already said so at a level that makes it visible.
        if cache.is_none() {
            tracing::debug!(
                requested = options.cache,
                "this run has no store; every operation is computed and nothing is kept"
            );
        }

        cache
    });

    let started = std::time::Instant::now();

    let outcome = run(
        backend,
        source.shared_pixels(),
        &planned,
        Steps { provider: options.provider, depth, keep: if mask.is_some() { removals } else { 0 } },
        options.on_progress,
        &options.cancel,
        cache.as_ref(),
    )
    .instrument(span.clone())
    .await;

    // After the chain and before the closing record, so the record's time and outcome are the whole run's, a stop
    // during the blend included.
    let outcome = match (outcome, &mask) {
        (Ok(Ran { pixels, providers, kept }), Some(mask)) => {
            let photograph = kept.unwrap_or_else(|| source.shared_pixels());

            masked(photograph, pixels, mask, depth, &options.cancel)
                .instrument(span.clone())
                .await
                .map(|pixels| (pixels, providers))
                .map_err(RunFailure::from)
        }
        (outcome, _) => outcome.map(|ran| (ran.pixels, ran.providers)),
    };

    // Recorded before the `?`, so that a run which failed says so rather than leaving a reader to infer it from a
    // beginning with no end. `duration` goes through `Duration`'s `Debug`, which renders `1.523s` and `250ms` — close
    // enough to Go's `time.Duration` spelling that a support instruction written for one log reads against the other.
    //
    // Every closing record names the photograph, so a reader holding only that line can say which run it closes.
    let duration = started.elapsed();
    let identity = source.identity();
    span.in_scope(|| match &outcome {
        Ok(_) => {
            tracing::info!(operations = operations.len(), identity, ?duration, "enhancement finished");
            unit::ended(Unit::Enhancement, &span, duration, Outcome::Finished);
        }
        Err(RunFailure { error, step }) => match error.stop_reason() {
            // A cancellation or a shutdown is not a failure, and nothing about it says one.
            Some(reason) => {
                tracing::info!(operations = operations.len(), identity, reason, ?duration, "enhancement stopped");
                unit::ended(Unit::Enhancement, &span, duration, Outcome::Stopped);
            }
            // The one record of the failure: the step that failed rides on the run's own record rather than on a
            // second one, so a reader is not told of two failures where there was one.
            None => {
                tracing::warn!(
                    operations = operations.len(),
                    identity,
                    operation = step.as_ref().map(|(operation, _)| operation.as_str()),
                    index = step.as_ref().map(|(_, index)| *index),
                    ?duration,
                    %error,
                    "enhancement failed"
                );
                unit::ended(Unit::Enhancement, &span, duration, Outcome::Failed { kind: error.kind(), error });
            }
        },
    });

    let (pixels, providers) = outcome.map_err(|failure| failure.error)?;

    // The path is the source's: it says where the pixels came from, never where they are going, so a result carries
    // the photograph it was made from and writing it is still a destination the caller names.
    //
    // The identity is composed from the image, the operations and the resolved depth, and `providers` is
    // deliberately no part of it: a result computed on a GPU and one computed on the CPU are the same pixels and are
    // interchangeable, so a provider in the identity would halve the run cache's hit rate on a machine whose GPU is
    // sometimes busy, for no correctness gain at all.
    //
    // Built with `derive`, so the result keeps the source's metadata — its colour profile — whether the pixels were
    // computed just now or served from the run cache, which stores pixels only: the profile is re-attached here from
    // the source on every path, and the source identity already covers its bytes.
    //
    // A mask is folded over the chain's identity rather than into it, so the steps' keys are the ones an unmasked run
    // stores under, and a run with no mask is named exactly as it was before masks existed.
    let chained = identity_after(source.identity(), operations, depth);
    let named = match &mask {
        Some(mask) => identity_masked(&chained, mask.signature()),
        None => chained,
    };
    let picture = source.derive(pixels, named);

    Ok(Enhanced { picture, providers, faces })
}

/// `pixels`, the chain's result, limited to `mask`: blended with `photograph` — the chain's input after its removals —
/// through the mask's weight at the photograph's size, the photograph enlarged with Lanczos3 first where the chain
/// changed the size.
///
/// Not a step: it reports no progress and is never stored, since redoing it costs less than reading it back. A stop
/// is honoured before it begins; it is quick enough not to need one inside.
async fn masked(
    photograph: Arc<DynamicImage>,
    pixels: Arc<DynamicImage>,
    mask: &Arc<Mask>,
    depth: ChannelDepth,
    cancel: &CancellationToken,
) -> Result<Arc<DynamicImage>, InferenceError> {
    if cancel.is_cancelled() {
        return Err(InferenceError::Cancelled);
    }

    let applying = Arc::clone(mask);
    let started = std::time::Instant::now();

    // Off the runtime, as every full-resolution pass is: a rasterisation and a blend at 24 megapixels, and under an
    // upscale a Lanczos enlargement the size of the result.
    let blended =
        spawn_blocking::<_, InferenceError, _>(move || blend(&photograph, pixels, &applying, depth)).await??;

    // `debug`, with the per-step records: a user painting a mask redoes this on every stroke.
    tracing::debug!(
        mask = %format_args!("{:016x}", mask.signature()),
        duration = ?started.elapsed(),
        "mask applied"
    );

    Ok(blended)
}

/// [`masked`]'s pixel work, on the calling thread.
///
/// # Errors
///
/// Returns [`InferenceError::MaskShape`] where the result is not the photograph's shape.
fn blend(
    photograph: &DynamicImage,
    result: Arc<DynamicImage>,
    mask: &Mask,
    depth: ChannelDepth,
) -> Result<Arc<DynamicImage>, InferenceError> {
    let (width, height) = (photograph.width(), photograph.height());
    let (result_width, result_height) = (result.width(), result.height());

    // No enhancement moves pixels — the crop is before the chain — so a result whose shape is not the photograph's
    // came from something the mask cannot be laid over, and is refused rather than handed back unmasked under the
    // identity of a masked result or smeared across the wrong pixels. Each side of an upscale is rounded on its own, so
    // `result_width = width * s + a` and `result_height = height * s + b` with `a` and `b` each within half a pixel,
    // and `result_width * height - result_height * width = a * height - b * width` is at most `(width + height) / 2`
    // either way. A whole pixel on each side is allowed, for a scale that is floored or ceiled rather than rounded.
    let (w, h, rw, rh) = (u64::from(width), u64::from(height), u64::from(result_width), u64::from(result_height));
    if (rw * h).abs_diff(rh * w) > w + h {
        return Err(InferenceError::MaskShape { width, height, result_width, result_height });
    }

    // At the source's size, and read bilinearly at the result's: an 8x upscale does not allocate a full-size plane.
    let weight = mask.rasterised(width, height);

    // A mask that is white everywhere keeps the result untouched and reads nothing of the photograph, so there is
    // nothing to enlarge it for.
    let enlarged;
    let photograph = if (result_width, result_height) == (width, height) || weight.range().0 >= 1.0 {
        photograph
    } else {
        enlarged = enlarge(photograph, result_width, result_height, depth);
        &enlarged
    };
    let sampler = Sampler::new(photograph);

    // Every step produces the resolved depth, so the result is that depth's RGB and the conversion is a move.
    let result = Arc::unwrap_or_clone(result);
    Ok(Arc::new(match depth {
        ChannelDepth::Eight => u8::into_dynamic(mix::masked(&sampler, result.into_rgb8(), &weight)),
        ChannelDepth::Sixteen => u16::into_dynamic(mix::masked(&sampler, result.into_rgb16(), &weight)),
    }))
}

/// `photograph` enlarged to `width` x `height` with Lanczos3, at `depth`.
///
/// Any alpha is composited against black first, as colorization's stretch does and for its reason: Lanczos over
/// straight alpha lets the colour stored under a transparent pixel bleed into its neighbours.
fn enlarge(photograph: &DynamicImage, width: u32, height: u32, depth: ChannelDepth) -> DynamicImage {
    let (from_width, from_height) = (photograph.width(), photograph.height());

    match (photograph, depth) {
        // Borrowed: nothing needs flattening, and at 24 megapixels a copy made only to be resampled is 72 MB.
        (DynamicImage::ImageRgb8(_), ChannelDepth::Eight) | (DynamicImage::ImageRgb16(_), ChannelDepth::Sixteen) => {
            photograph.resize_exact(width, height, FilterType::Lanczos3)
        }
        (_, ChannelDepth::Eight) => u8::into_dynamic(flattened::<u8>(
            &Sampler::new(photograph),
            from_width,
            from_height,
        ))
        .resize_exact(width, height, FilterType::Lanczos3),
        (_, ChannelDepth::Sixteen) => u16::into_dynamic(flattened::<u16>(
            &Sampler::new(photograph),
            from_width,
            from_height,
        ))
        .resize_exact(width, height, FilterType::Lanczos3),
    }
}

/// `operations` without any removal that removes nothing: borrowed where there is none, which is every chain but one
/// a front end added a removal to before anything was painted.
fn without_empty_removals(operations: &[Operation]) -> std::borrow::Cow<'_, [Operation]> {
    let empty = |operation: &Operation| matches!(operation, Operation::Remove(removal) if removal.removes_nothing());

    if !operations.iter().any(empty) {
        return std::borrow::Cow::Borrowed(operations);
    }

    std::borrow::Cow::Owned(operations.iter().filter(|operation| !empty(operation)).cloned().collect())
}

/// `operations` with every face recovery's faces cut to those no removal in the chain takes out, over a `width` x
/// `height` source: a face whose box centre is marked is not restored. Unchanged — borrowed or owned as it came — where
/// the chain removes nothing or restores no face.
fn sparing_removed_faces(
    operations: std::borrow::Cow<'_, [Operation]>,
    width: u32,
    height: u32,
) -> std::borrow::Cow<'_, [Operation]> {
    let removals: Vec<&Remove> = operations
        .iter()
        .filter_map(|operation| match operation {
            Operation::Remove(removal) => Some(removal),
            _ => None,
        })
        .collect();
    let restoring = operations
        .iter()
        .any(|operation| matches!(operation, Operation::FaceRecovery(recovery) if !recovery.faces().is_empty()));

    if removals.is_empty() || !restoring || width == 0 || height == 0 {
        return operations;
    }

    let marked = RemovedAt::new(&removals, width, height);
    let spared = operations
        .iter()
        .map(|operation| match operation {
            Operation::FaceRecovery(recovery) => {
                let kept = recovery.faces().iter().copied().filter(|face| !marked.at_centre_of(face));
                Operation::FaceRecovery(recovery.with_faces(Faces::new(kept)))
            }
            other => other.clone(),
        })
        .collect();

    std::borrow::Cow::Owned(spared)
}

/// The operations of a run as one field value, comma-separated: two steps of one chain read as `<tag>,<tag>`.
///
/// Their [`cache_tag`](Operation::cache_tag)s rather than their display names.
fn operation_ids(operations: &[Operation]) -> String {
    // Cache tags because that is what the per-step records below and the cache keys are keyed on — so one `grep` over
    // `opai.log` for an operation returns the run it belonged to as well as what each step of it did.
    operations.iter().map(Operation::cache_tag).collect::<Vec<_>>().join(",")
}

/// Why a run produced no image, and which step it was on when it did not.
pub(crate) struct RunFailure {
    /// What the run returns to its caller.
    pub(crate) error: InferenceError,
    /// The failing operation's cache tag and its index in the chain, or `None` for a failure between steps — a
    /// cancellation noticed before the next one began, or a store read abandoned by a shutdown.
    pub(crate) step: Option<(String, usize)>,
}

impl From<InferenceError> for RunFailure {
    fn from(error: InferenceError) -> Self {
        Self { error, step: None }
    }
}

/// How a chain's steps are run: on what, at what depth, and which intermediate to hand back beside the result.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Steps {
    /// The provider every step asks for.
    pub(crate) provider: ExecutionProvider,
    /// The resolved depth every step produces.
    pub(crate) depth: ChannelDepth,
    /// How many leading steps' result to hand back as [`Ran::kept`] — the photograph after its removals, which a
    /// limiting mask is blended with — or 0 for none.
    pub(crate) keep: usize,
}

/// What a chain run produced: the result, what it ran on, and the intermediate [`Steps::keep`] asked for.
pub(crate) struct Ran {
    /// The chain's result.
    pub(crate) pixels: Arc<DynamicImage>,
    /// What every step ran on.
    pub(crate) providers: ProviderReport,
    /// The picture after the first [`Steps::keep`] steps, or `None` where none was asked for.
    pub(crate) kept: Option<Arc<DynamicImage>>,
}

/// Runs `source` through the `planned` chain in order, each operation over the result of the one before it, at
/// `depth`.
///
/// `depth` is already resolved — [`process`] resolves it against the loaded image, once, and hands the answer both to
/// this and to the identity the result carries.
///
/// The chain arrives already [`plan`](fn@plan)ned, so a chain naming something that cannot be served was refused
/// before a byte was transferred on behalf of anything that can — even when every earlier step is stored: a cache must
/// not turn a bad request into a partial success.
///
/// **Every session stays open until the whole chain finishes**, not merely until the operation using it returns, so a
/// model a run holds cannot be reclaimed by the idle sweep, by an explicit release, or by a provider switch.
///
/// `cache` is the store this run reads and writes, already decided by [`process`] for the whole call. An operation
/// whose result is stored reports [`Stage::Cached`](super::progress::Stage::Cached) and acquires no session at all.
/// A cancelled or failed operation writes nothing.
///
/// An empty chain returns `source` unchanged, identity included: applying nothing derives nothing.
///
/// # Errors
///
/// [`RunFailure`], carrying the [`InferenceError`] and the step it happened in, and in every case **no image at all** —
/// not the operations that had already succeeded, and not the half-written buffer of the tile a cancellation landed
/// in. **Nothing the cache does appears here**: every one of its failures is a recompute, so it has no way to fail a
/// run.
///
/// Where [`Steps::keep`] names a leading run of steps, the picture after them is handed back too, decoded from the
/// store where they were served from it: what a limiting mask is blended with.
pub(crate) async fn run<B: Backend>(
    backend: &B,
    source: Arc<DynamicImage>,
    planned: &[Planned<B>],
    steps: Steps,
    on_progress: Option<OnInference>,
    cancel: &CancellationToken,
    cache: Option<&RunCache>,
) -> Result<Ran, RunFailure> {
    let Steps { provider, depth, keep } = steps;

    if planned.is_empty() {
        // No session, so nothing to report having run on — which is a different answer from "it ran on what was
        // asked for", and is exactly the distinction `ProviderReport::actual` exists to keep.
        return Ok(Ran { pixels: source, providers: ProviderReport::new(provider, []), kept: None });
    }

    let progress = ChainProgress::new(on_progress, planned.len());

    // What each step's cache key is derived from: the chain up to and including it.
    let operations: Vec<Operation> = planned.iter().map(|step| step.operation().clone()).collect();

    // Every handle the chain takes, held for the whole call and dropped when it returns. Holding a `SessionHandle` *is*
    // the guarantee: the session cache's "in use" test is the handle's own reference count. A chain naming one model
    // twice takes two handles on one entry, which is correct — the count goes up twice and down twice.
    let mut held: Vec<SessionHandle<B::Session>> = Vec::new();
    let mut current = source;
    let mut kept = None;

    // The bytes of the newest cached prefix walked past, not yet decoded, and the index the run of hits carrying them
    // began at. A hit supersedes the one before it — each stored entry is the whole picture at that point in the
    // chain — so only the newest is kept, and it is decoded once, when something actually needs the pixels.
    let mut pending: Option<Vec<u8>> = None;
    let mut skipped_from = 0_usize;

    // Cache reads are suppressed below this index. Only a prefix that would not decode raises it; see the rewind.
    let mut distrust_before = 0_usize;

    // The run cache sits in this loop, which is what makes dragging a slider cheap: a step whose result is already
    // there skips the model *and* the session acquisition, since acquiring would install a model for an operation that
    // is not going to run. See `cache` for what a failure at either end costs.
    //
    // The lookup walks **forward**, prefix by prefix, rather than probing for the longest stored prefix first. Probing
    // would read a several-hundred-megabyte value off disk merely to find out whether it was there, since the store
    // has no existence probe and `get_bytes` returns the value. A forward walk reads each stored prefix's bytes on its
    // way past and decodes only the newest, which is the one it uses.
    let mut index = 0;
    loop {
        // `index == planned.len()` is the tail: the chain ended on a run of hits, so the newest prefix *is* the
        // result. It reaches the decode below by the same path a miss does, because it is the same moment — the first
        // time anything needs the pixels — and a prefix that will not decode rewinds identically.
        let operation = planned.get(index);

        if cancel.is_cancelled() {
            return Err(InferenceError::Cancelled.into());
        }

        // What this step *will* produce, which is the slot it was stored under if it has been run before. The one
        // derivation the result's own identity uses, rather than a second one beside it: two that agreed today would
        // be two things to keep agreeing.
        let key = operation.and(cache).map(|cache| identity_after(cache.source(), &operations[..=index], depth));

        if let (Some(operation), Some(cache), Some(key)) = (operation, cache, key.as_ref())
            && index >= distrust_before
        {
            let reading = cache.clone();
            let wanted = key.clone();
            let stored = spawn_blocking::<_, InferenceError, _>(move || reading.get_bytes(&wanted)).await?;
            metrics::cache_lookup(metrics::STEP, stored.is_some());

            if let Some(bytes) = stored {
                // `debug`, because it repeats once per operation and a user dragging a slider would otherwise fill
                // their file with it. See this module's header for the rule.
                tracing::debug!(operation = %operation.operation().cache_tag(), index, key, "step served from the store");

                // Built here rather than before the lookup, because a miss - the ordinary case on a first run - would
                // otherwise allocate a reporter only to drop it unread and build an identical one further down. No
                // `Arc` either: nothing on this path shares it with a closure, which is the only reason the reporter
                // the tail builds needs one.
                //
                // Its full share of the bar, through the same path every other report goes through — so a chain whose
                // every operation was stored still ends on exactly 1 rather than never reporting at all.
                progress.operation(index, operation.operation().clone()).cached();

                if pending.is_none() {
                    skipped_from = index;
                }
                // Replaces the previous prefix's bytes, which are dropped undecoded: they describe the same picture
                // at an earlier point, and nothing between here and the next miss reads them.
                pending = Some(bytes);
                index += 1;

                if index != keep {
                    continue;
                }

                // The one hit whose pixels are wanted although nothing runs over them yet: the picture a limiting
                // mask is blended with. A prefix that will not decode rewinds as a miss's does, over the same span.
                match materialize(pending.take().expect("stored just above")).await? {
                    Some(image) => {
                        current = Arc::new(image);
                        kept = Some(Arc::clone(&current));
                    }
                    None => {
                        tracing::warn!(
                            from = skipped_from,
                            to = index,
                            "a stored result would not decode; rerunning the operations it stood for"
                        );

                        distrust_before = index;
                        index = skipped_from;
                    }
                }
                continue;
            }
        }

        // A miss, so this operation needs the pixels the skipped prefix stored — which is the one point at which they
        // are worth decoding.
        if let Some(bytes) = pending.take() {
            match materialize(bytes).await? {
                Some(image) => current = Arc::new(image),
                None => {
                    // The stored bytes no longer decode: a truncated write, an interrupted shutdown. That is a miss
                    // like any other, and the response to a miss is to run the operation — so the run of hits that was
                    // skipped on their account is walked again with the cache suppressed over exactly that span.
                    // Every other entry stays trusted, and the bar has already been clamped monotonic, so the repeat
                    // costs work rather than a report that goes backwards.
                    //
                    // `warn`: nothing failed that a caller will hear about, and the user is about to pay for a run of
                    // operations they had already paid for once.
                    tracing::warn!(
                        from = skipped_from,
                        to = index,
                        "a stored result would not decode; rerunning the operations it stood for"
                    );

                    distrust_before = index;
                    index = skipped_from;
                    continue;
                }
            }
        }

        // The tail: every operation is behind us and the prefix above has been decoded into `current`.
        let Some(operation) = operation else {
            break;
        };

        // Only a step that is computed has a span: one served from the store is its cache read's.
        let model = operation.operation().model_tag();
        let step = unit_span!("step", operation = %operation.operation().cache_tag(), index, model = %model);

        // Everything from here to the end of the step fails *in* it, so the failure carries which one it was, and the
        // step's span says so beside the run's. A stop inside a step is a stop there too.
        let in_step = |error: InferenceError| {
            let ending = match error.stop_reason() {
                Some(_) => Outcome::Stopped,
                None => Outcome::Failed { kind: error.kind(), error: &error },
            };
            unit::mark(&step, &ending);

            RunFailure { error, step: Some((operation.operation().cache_tag(), index)) }
        };

        let reporter = Arc::new(progress.operation(index, operation.operation().clone()));

        let stages = operation.pipeline.stages();

        // A step whose model throws an error on an `Auto` accelerator is run again on the next provider of its
        // ladder, from the start of the step: the steps before it keep what they produced. Every retry declines at
        // least one provider for good, so it ends — at the latest on the CPU, which is never declined.
        let (sessions, produced) =
            acquire::run_declining(backend, operation.pipeline.as_ref(), &reporter, provider, cancel, |sessions| {
                // Shared with the blocking half rather than borrowed into it; see `Shared`.
                let pipeline = Shared::clone(&operation.pipeline);

                let input = Arc::clone(&current);
                let cancelled = cancel.clone();
                let reported = Arc::clone(&reporter);
                let wanted = progress.wanted();
                // Handed to the store's writer from inside the blocking call, the moment the result exists.
                let writing = cache.cloned().zip(key.clone());

                // **Acquire on the runtime, run off it.** In a Tauri process the async runtime *is* the window's event
                // loop, so a minute of tiled inference on it would not merely slow the application down, it would stop
                // the window from drawing — including the progress bar this reporting exists to feed.
                //
                // The sessions travel into the blocking closure and back out of it, so they are owned throughout the
                // run and are pushed onto the chain's own list the moment it ends — there is no window in which a later
                // operation could find one of them reclaimed. For a diffusion operation that is three handles held
                // across **every** region of the image rather than across one, which makes the operation longer but not
                // different in kind. The error type is named rather than inferred: `Cancelled` converts into three of
                // this crate's errors, and which one a shutdown is reported as is the choice being made here —
                // `InferenceError::Shutdown`.
                let running = std::time::Instant::now();
                let ran = spawn_blocking::<_, InferenceError, _>(move || {
                    let report = |fraction: f64| reported.running(fraction);
                    let report = wanted.then_some(&report as &dyn Fn(f64));
                    let stopped = || cancelled.is_cancelled();

                    // One call, whatever the model is. Shared from here on, so the store's writer and the rest of the
                    // chain hold one buffer rather than two.
                    let produced = pipeline.run(&input, &sessions, depth, report, &stopped).map(Arc::new);

                    // Only what actually succeeded — a cancelled or failed operation arrives as an `Err`, so nothing
                    // needs to check the token a second time — and the result is handed back either way: a write that
                    // is declined or that breaks is dropped where it happened, because the pixels are already computed
                    // and throwing finished work away over a full disk would be the cache's problem becoming the
                    // enhancement's.
                    //
                    // **Queued, not written.** Encoding a large result as PNG and writing it is hundreds of
                    // milliseconds to seconds, and nothing in this run needs it done: the next step and the caller have
                    // the pixels. A later read of this key waits for the write rather than missing it — see
                    // `RunCache::put_later`.
                    if let (Ok(image), Some((cache, key))) = (&produced, &writing) {
                        cache.put_later(key, Arc::clone(image));
                    }

                    (sessions, produced)
                });

                // The model's run alone, timed from this attempt's hop: a failed attempt's time is the retry's to
                // replace.
                async move {
                    let (sessions, produced) = ran.await?;
                    let ran = running.elapsed();

                    Ok((sessions, produced.map(|image| (image, ran))))
                }
            })
            .instrument(step.clone())
            .await
            .map_err(in_step)?;

        let ran_on = ran_on(&sessions);
        held.extend(sessions);

        // Not recorded here: `process`'s closing record names the step, and a second record would be a second account
        // of one failure. Not the operations that had already succeeded either: a caller given a partial result has
        // no way to tell it from a finished enhancement and would show it to a user as one.
        let (produced, ran) = produced.map_err(in_step)?;

        // The counterpart of the hit above, at the same level and with the same fields, so one `grep` over an
        // operation's id returns every step of every run that touched it and says which way each went.
        step.in_scope(|| {
            tracing::debug!(operation = %operation.operation().cache_tag(), index, stages, "step computed");
        });
        unit::mark(&step, &Outcome::Finished);
        // The model's run alone: a first run's install and build are measured as units of their own.
        metrics::STEP_DURATION.record_with_tags(ran.as_secs_f64(), &[("model", &model), ("provider", ran_on)]);

        current = produced;
        index += 1;

        if index == keep {
            kept = Some(Arc::clone(&current));
        }
    }

    // Folded out of the handles the run was already holding, rather than accumulated by a line added to the
    // acquisition loop: `held` carries every session this chain took, each stamped with the provider it was built on,
    // so the report costs no plumbing at all — not in the loop, not in `Backend::acquire`, not in the pass sequence.
    //
    // That is also why a chain downgraded for its second model and not its first reports both: every handle is here,
    // not only the first one whose provider differed from what was asked for.
    let report = ProviderReport::new(provider, held.iter().map(SessionHandle::provider));

    Ok(Ran { pixels: current, providers: report, kept })
}

/// The provider a step's sessions ran on, as its duration is tagged: one provider, or `mixed` where its graphs were
/// built on different ones.
fn ran_on<S>(sessions: &[SessionHandle<S>]) -> &'static str {
    let mut providers = sessions.iter().map(SessionHandle::provider);
    let Some(first) = providers.next() else { return "none" };

    if providers.all(|provider| provider == first) { first.as_str() } else { "mixed" }
}

/// Decodes a stored entry off the async runtime, or `None` where it no longer decodes as an image.
async fn materialize(bytes: Vec<u8>) -> Result<Option<DynamicImage>, InferenceError> {
    // Its own function because the decode is the expensive half of a cache read and the chain does exactly one of them
    // per run, however many entries it walked past: a several-hundred-megabyte PNG is the work the tile loop is kept
    // off the runtime for, and it would be no better here.
    spawn_blocking::<_, InferenceError, _>(move || RunCache::decode(&bytes)).await
}

#[cfg(test)]
mod tests;

// The planner over every operation the library can name, family by family. Here beside `plan` rather than beside the
// carriers in `models::operation`, because `models` never names `inference`: the seam each of these also asks —
// `Operation::pipeline` — is `models`' own, and the planner above it is this module's.
#[cfg(test)]
mod every_family_plans;
