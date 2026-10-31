//! Finding what a chain's lens blurs are driven by: one depth estimate of the chain's input, and the photograph's lens
//! from its EXIF, handed to every lens blur.
//!
//! The counterpart of `inference::faces` for the lens blur, with one difference: a lens blur cannot run without its
//! depth, so a failed estimate fails the run, where a failed face detection only restores nobody.
//!
//! ```text
//!   chain has no lens blur?  --> chain unchanged, nothing estimated
//!   read_lens(source path), the file's frame, the picture's framing
//!   lens known from the EXIF alone, and every lens blur at or above its own f-number?
//!                            --> each lens blur := with_lens(lens); nothing estimated
//!   Depth (cached execute) on the chain's input, after its removals
//!      Ok(map)    --> lens := Lens::resolve(exif, map's focal estimate, framing)
//!                     each lens blur := with_depth(map), with_lens(lens)
//!      Err(error) --> the run fails with it (a stop included)
//!   each lens blur matting its edges whose blur reaches 3 px, with a subject in focus:
//!      trimap from the depth around its focus, on the chain's input
//!      Matting (cached execute) of the chain's input over that trimap
//!         Ok(matte)            --> lens blur := with_matte(matte); Matted
//!         a stop               --> the run stops
//!         not installed        --> the lens blur renders from the depth alone, and so do the rest; Unavailable
//!         Err(error)           --> the lens blur renders from the depth alone; Failed(error)
//! ```
//!
//! The depth is estimated on the chain's **input** — the framed source, before any enhancement — so changing the
//! focus, the aperture or any earlier enhancement never estimates it again: the cached run serves it. The matte is too,
//! keyed by its trimap, which the focus moves and the aperture does not.
//!
//! **Except for its removals**, which `process` runs first and hands this the result of as the input: the depth and
//! the matte of what a removal took out would still drive the blur over the fill, a silhouette of it sharp or blurred
//! where it was. The picture after them is named as the chain names that step, so another region is estimated again
//! and the same one is served.
//!
//! **The framing** is how much smaller the input is than its file across the diagonal: a crop made before the chain
//! (the GUI's, which `opai` never sees) is a picture smaller than the file it names, and the file's EXIF describes the
//! whole of it. See [`Lens::resolve`] for what a crop does to the lens.

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use imaging::plane::Plane;

use crate::error::{InferenceError, InitError};
use crate::image::{LensExif, Picture, read_frame_diagonal, read_lens};
use crate::inference::execute::ExecuteOptions;
use crate::models::lens_blur::lens::Blur;
use crate::models::lens_blur::refine::Refinement;
use crate::models::lens_blur::render::Refined;
use crate::models::lens_blur::trimap::{
    Band, cut_at_steps, firmed, imperceptible, joined, matted_focus, seeded, trimap, widened, worth_matting,
};
use crate::models::{Depth, DepthMap, Lens, LensBlur, Matte, Matting, Operation, OwnLens};
use crate::task::spawn_blocking;

/// `operations` with every lens blur handed `map` and `lens`; every other operation unchanged.
///
/// The pure half of the injection, public so a front end answering its own test seams can follow the same rules
/// without a depth model on disk.
pub fn fill_chain_depth(operations: Vec<Operation>, map: &Arc<DepthMap>, lens: Lens) -> Vec<Operation> {
    operations
        .into_iter()
        .map(|operation| match operation {
            Operation::LensBlur(blur) => Operation::LensBlur(blur.with_depth(Arc::clone(map)).with_lens(lens)),
            other => other,
        })
        .collect()
}

/// `operations` with every lens blur that was handed a depth also handed `weight` to keep sharp, signed `signature`:
/// a mask's weight at the chain input's size, and the mask's signature. Every other operation unchanged, and so is a
/// blur with no depth, which adds nothing to the photograph and so has nothing to keep in focus.
pub(crate) fn fill_chain_keep_sharp(operations: Vec<Operation>, weight: &Arc<Plane>, signature: u64) -> Vec<Operation> {
    operations
        .into_iter()
        .map(|operation| match operation {
            Operation::LensBlur(blur) if blur.depth().is_some() => {
                Operation::LensBlur(blur.with_keep_sharp(Arc::clone(weight), signature))
            }
            other => other,
        })
        .collect()
}

/// The depth estimate `operations` need, or `None` where they hold no lens blur.
pub(crate) fn depth_asked(operations: &[Operation]) -> Option<Depth> {
    operations.iter().find_map(|operation| match operation {
        Operation::LensBlur(blur) => Some(blur.depth_estimate()),
        _ => None,
    })
}

/// A picture whose diagonal is within this fraction of its file's is the whole frame: a ratio of 1.005 is not a crop,
/// and would only turn a cache tag's `l36-6` into `l36.2-6.03`.
const WHOLE_FRAME_SLACK: f32 = 0.01;

/// How many times `file_diagonal` (pixels, the whole frame's) is the diagonal of a picture `dimensions` across: 1 for
/// the whole frame and where the file is not known, so a picture with no file is rendered as the frame it has.
///
/// The picture is taken to be the file's pixels at their own scale, cut: a crop is not resampled, so its diagonal over
/// the file's is how much of the frame it holds.
fn framing_of(file_diagonal: Option<f32>, dimensions: (u32, u32)) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "a pixel count a ratio is taken of")]
    let picture = (dimensions.0 as f32).hypot(dimensions.1 as f32);

    match file_diagonal {
        Some(file) if picture > 0.0 && file / picture > 1.0 + WHOLE_FRAME_SLACK => file / picture,
        _ => 1.0,
    }
}

/// The lens the file at `path` records, checked against the file's own frame, and the framing of the `picture` (its
/// dimensions in pixels) cut from it: the reads [`own_lens`] and [`find_chain_depth`] share, so what a front end shows
/// is what a run blurs from. Reads the file's EXIF and header, so a caller on an async runtime runs it on a blocking
/// thread. A picture with no file reads as a lens nothing is known about, at the framing it has.
fn framed_exif(path: &Path, picture: (u32, u32)) -> (LensExif, f32) {
    let exif = read_lens(path);
    let file_diagonal = read_frame_diagonal(path);
    let framing = framing_of(file_diagonal, picture);
    // The sensor the EXIF names is of the pixels the EXIF says it has: a file since resized has not those.
    let exif = file_diagonal.map_or(exif, |diagonal| exif.checked_against(diagonal));

    (exif, framing)
}

/// The lens of the file at `path` as its EXIF alone settles it, for the picture `framed` (its dimensions in pixels, or
/// `None` for the whole frame): what a front end shows before any depth is estimated, answered at once and never
/// running the depth model. Reads the file's EXIF and header, so a caller on an async runtime runs it on a blocking
/// thread.
///
/// The same reads, checks and framing as [`find_chain_depth`], so the f-number a slider ends at is the one a run blurs
/// from. A file that cannot be read is a lens nothing is known about: the assumed f/8.
pub fn own_lens(path: &Path, framed: Option<(u32, u32)>) -> OwnLens {
    let (exif, framing) = framed_exif(path, framed.unwrap_or_default());

    OwnLens::from_exif(&exif, framing)
}

/// `operations` with every lens blur handed the depth `estimate` finds and the lens the file at `path` records, framed
/// as the `picture` (its dimensions) it is run over is: `estimate` is called at most once, with `options`, and only for
/// a chain holding a lens blur that the lens does not already show adds nothing.
///
/// # Errors
///
/// Whatever `estimate` fails with, a stop included: a lens blur has nothing to run on without its depth.
pub(crate) async fn find_chain_depth<E, F>(
    operations: Vec<Operation>,
    path: &Path,
    picture: (u32, u32),
    options: Option<ExecuteOptions>,
    estimate: E,
) -> Result<Vec<Operation>, InferenceError>
where
    E: FnOnce(Depth, Option<ExecuteOptions>) -> F,
    F: Future<Output = Result<DepthMap, InferenceError>>,
{
    let Some(depth) = depth_asked(&operations) else {
        return Ok(operations);
    };

    // A few reads of the file's header, off the runtime as every file read is, and before the estimate: they decide
    // whether it is needed.
    let path = path.to_path_buf();
    let (exif, framing) = spawn_blocking::<_, InferenceError, _>(move || framed_exif(&path, picture)).await?;

    // A lens the EXIF alone settles, and every blur at or above its own f-number: the photograph is returned as it is,
    // so the model that would say how far away things are is neither loaded nor run.
    if let Some(lens) = Lens::without_depth(&exif, framing) {
        let adds_nothing = operations.iter().all(|operation| match operation {
            Operation::LensBlur(blur) => blur.adds_nothing_to(&lens),
            _ => true,
        });

        if adds_nothing {
            tracing::info!(own_f_number = lens.own_f_number, framing, "no lens blur adds anything; no depth estimated");

            return Ok(fill_chain_lens(operations, lens));
        }
    }

    let map = estimate(depth, options).await?;
    let lens = Lens::resolve(&exif, map.focal_35mm(), framing);

    // `info`, once per run: which lens a blur was rendered for is what explains a result that looks too strong or
    // too weak, and an assumed one is the case a user most needs to see.
    tracing::info!(
        focal_35mm = lens.focal_35mm,
        own_f_number = lens.own_f_number,
        origin = ?lens.origin,
        depth_scale = lens.depth_scale,
        framing,
        exif_focal = exif.focal_mm,
        exif_focal_35mm = exif.focal_35mm,
        exif_f_number = exif.f_number,
        depth_focal_35mm = map.focal_35mm(),
        "the lens a lens blur is rendered for"
    );

    bench_dump("depth", map.dimensions(), map.metres());

    Ok(fill_chain_depth(operations, &Arc::new(map), lens))
}

/// Fork only, for the benchmark (`opai-bench`): where `OPAI_BENCH_DUMP` is set to a path prefix, `values` (one a
/// pixel, row-major) are written to `<prefix>-<name>.f32` as a little-endian `u32` width, `u32` height and `f32`
/// values. A failed write is logged and changes nothing else.
fn bench_dump(name: &str, (width, height): (u32, u32), values: &[f32]) {
    let Some(prefix) = std::env::var_os("OPAI_BENCH_DUMP") else {
        return;
    };
    let path = format!("{}-{name}.f32", prefix.to_string_lossy());
    let mut bytes = Vec::with_capacity(8 + 4 * values.len());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    values.iter().for_each(|value| bytes.extend_from_slice(&value.to_le_bytes()));
    if let Err(error) = std::fs::write(&path, bytes) {
        tracing::warn!(%error, path, "the benchmark dump could not be written");
    }
}

/// What matting the subjects of a chain's lens blurs came to, for a front end to show beside the result.
///
/// Of every lens blur in the chain, the outcome that most needs saying: a failure before a model that is not installed,
/// that before a matte, and a matte before one skipped.
#[derive(Debug, Clone)]
pub enum ChainMatte {
    /// The chain carries no lens blur that mattes its subject's edges, so nothing was matted.
    NotAsked,
    /// A lens blur's subject was matted, or its matte served from the store, and composited over the blur.
    Matted,
    /// No lens blur's subject was worth matting — a blur under 3 px everywhere, or nothing in focus to speak of — so
    /// each renders from its depth alone, as it would have anyway.
    Skipped,
    /// A lens blur adds too little blur to see: less than 5% of the photograph blurs to a disc of its diagonal / 1500,
    /// the depth-of-field tables' acceptable circle of confusion (`trimap::imperceptible`). It rendered, and is not
    /// matted; a front end tells the user to focus on something nearer or to open the lens further, since a render
    /// that changed nothing visible reads as one that did not happen — a wide-angle lens focused far away, typically.
    LittleBlur,
    /// The matting model is not installed here: its graph is not published with the others yet. Every lens blur renders
    /// from its depth alone. Expected wherever it was never installed, and nothing to warn a user of.
    Unavailable(InferenceError),
    /// The matting failed, and that lens blur renders from its depth alone. The rest of the chain is unaffected.
    Failed(InferenceError),
}

impl ChainMatte {
    /// How much this outcome needs saying, beside another lens blur's: the larger is the chain's.
    const fn weight(&self) -> u8 {
        match self {
            Self::NotAsked => 0,
            Self::Skipped => 1,
            Self::LittleBlur => 2,
            Self::Matted => 3,
            Self::Unavailable(_) => 4,
            Self::Failed(_) => 5,
        }
    }

    /// The chain's outcome once a lens blur's came to `other`.
    fn and(self, other: Self) -> Self {
        if other.weight() > self.weight() { other } else { self }
    }
}

/// How many analyses the lens blurs in `operations` may each ask for once the depth is known: a matte for each that
/// mattes its subject's edges, a second, over the seeded trimap, for each of those that looks for its subject's holes,
/// and Bilbao's matte of the whole photograph once for all of them.
pub(crate) fn mattes_asked(operations: &[Operation]) -> usize {
    let (mattes, holes) = operations
        .iter()
        .filter_map(|operation| match operation {
            Operation::LensBlur(blur) if blur.matting().is_some() => Some(blur.finds_holes()),
            _ => None,
        })
        .fold((0, 0), |(mattes, holes), finds| (mattes + 1, holes + usize::from(finds)));

    mattes + holes + usize::from(holes > 0)
}

/// `operations` with every lens blur that mattes its subject handed the matte of `source`, the chain's input after its
/// removals, over the trimap its depth and focus give, from `matte`, and what that came to: called once for each such
/// blur whose blur reaches 3 px somewhere and whose focus has a subject, with the options `options` gives for the next
/// of the analyses still to come (its argument, at least 1), so each reports where the last left off.
///
/// A blur that looks for its subject's holes ([`LensBlur::finds_holes`]) is matted twice: over the trimap the depth
/// drew, and over that trimap with Bilbao's holes seeded as background ([`seeded`]), the two then [`joined`]; both with
/// the strands Bilbao sees beyond the depth's subject left unknown first ([`widened`]). Bilbao's matte of the whole
/// photograph is asked for once, for the first such blur, and served from the store afterwards whatever the focus.
/// Where Bilbao is not installed or fails, or finds no hole, the subject is matted once, as solid.
///
/// Called after [`find_chain_depth`], whose depth and lens it reads. A blur that adds nothing, mattes nothing, or was
/// handed no depth is left as it is.
///
/// # Errors
///
/// A stop, from `matte` or between its calls. Any other failure of `matte` — the graph not installed, a run that
/// failed — is not one: it is reported in the [`ChainMatte`], and the blur renders from its depth alone, as it did
/// before matting existed. A graph that is not installed is not asked for again by the chain's later lens blurs.
pub(crate) async fn find_chain_matte<O, M, F>(
    operations: Vec<Operation>,
    source: &Picture,
    options: O,
    matte: M,
) -> Result<(Vec<Operation>, ChainMatte), InferenceError>
where
    O: Fn(usize) -> Option<ExecuteOptions>,
    M: Fn(Matting, Option<ExecuteOptions>) -> F,
    F: Future<Output = Result<Matte, InferenceError>>,
{
    // Atomic rather than a `Cell` only so the future stays `Send`: it is read and written on one task.
    let remaining = AtomicUsize::new(mattes_asked(&operations));
    let mut outcome = ChainMatte::NotAsked;
    let mut matted = Vec::with_capacity(operations.len());
    // Bilbao's matte, once asked for: `Some(None)` where it could not be had.
    let mut salient: Option<Option<Arc<Matte>>> = None;

    // The options for the next analysis, and one fewer still to come.
    let next = || {
        let left = remaining.load(Ordering::Relaxed);
        remaining.store(left.saturating_sub(1), Ordering::Relaxed);
        options(left.max(1))
    };

    for operation in operations {
        let Operation::LensBlur(blur) = operation else {
            matted.push(operation);
            continue;
        };
        let Some(matting) = blur.matting() else {
            matted.push(Operation::LensBlur(blur));
            continue;
        };
        let holes = blur.finds_holes();

        if matches!(outcome, ChainMatte::Unavailable(_)) {
            matted.push(Operation::LensBlur(blur));
            continue;
        }

        // The trimap off the runtime, as every pass over a photograph's pixels is: the depth guided to its size, then
        // the band, at a quarter of it.
        let (asked, pixels) = (blur.clone(), source.shared_pixels());
        let (trimap, refined, band) =
            match spawn_blocking::<_, InferenceError, _>(move || trimap_of(&asked, &pixels)).await? {
                Ok(found) => found,
                Err(not_matted) => {
                    outcome = outcome.and(not_matted);
                    matted.push(Operation::LensBlur(blur));
                    continue;
                }
            };

        // Bilbao once a chain, for the first blur that looks for holes; the store serves it to every later focus.
        if holes && salient.is_none() {
            salient = Some(match matte(Matting::bilbao(), next()).await {
                Ok(found) => Some(Arc::new(found)),
                Err(error) if error.stop_reason().is_some() => return Err(error),
                Err(error) => {
                    tracing::debug!(%error, "the subject's holes are not looked for: Bilbao could not matte it");
                    None
                }
            });
        }
        let saliency = salient.clone().flatten().filter(|_| holes);
        // The strands the depth cannot see, where Bilbao sees them, left to the matting model.
        let (trimap, refined) = match &saliency {
            Some(saliency) => {
                let saliency = Arc::clone(saliency);
                spawn_blocking::<_, InferenceError, _>(move || {
                    let found = widened(&trimap, &saliency, &refined, band).unwrap_or(trimap);
                    (found, refined)
                })
                .await?
            }
            None => (trimap, refined),
        };
        // The refine brush over all of it: blur strokes sure background, focus strokes over sure background unknown,
        // even in front of the band. What the matting model sees, and so what its cache is keyed by.
        let refine = blur.refine().cloned();
        let trimap = match &refine {
            Some(refine) => {
                let refine = Arc::clone(refine);
                spawn_blocking::<_, InferenceError, _>(move || Refinement::trimap(&refine, &trimap)).await?
            }
            None => trimap,
        };
        bench_dump("widened", (trimap.width(), trimap.height()), trimap.as_slice());
        let trimap = Arc::new(trimap);
        let seeded = match &saliency {
            Some(saliency) => {
                bench_dump("saliency", saliency.dimensions(), saliency.plane().as_slice());
                let (plain, saliency, pixels) = (Arc::clone(&trimap), Arc::clone(saliency), source.shared_pixels());
                let refine = refine.clone();
                // No seed planted where focus is painted: the brush is laid over the seeded trimap as over the plain.
                let seeded = spawn_blocking::<_, InferenceError, _>(move || {
                    let seeded = seeded(&plain, &saliency, Some(&pixels.to_rgb8()));
                    match &refine {
                        Some(refine) => seeded.map(|seeded| Refinement::trimap(refine, &seeded)),
                        None => seeded,
                    }
                })
                .await?;
                if let Some(seeded) = &seeded {
                    bench_dump("seeded", (seeded.width(), seeded.height()), seeded.as_slice());
                }
                seeded
            }
            None => None,
        };

        // Recorded by `execute`, where it happened; this only notes what the chain does about it.
        match matte(matting.clone().with_trimap(trimap), next()).await {
            Ok(plain) => {
                let firm = saliency.clone();
                let found = match (seeded, saliency) {
                    (Some(seeded), Some(saliency)) => {
                        match matte(matting.with_trimap(Arc::new(seeded)), next()).await {
                            Ok(holed) => {
                                spawn_blocking::<_, InferenceError, _>(move || joined(&plain, &holed, &saliency))
                                    .await?
                            }
                            Err(error) if error.stop_reason().is_some() => return Err(error),
                            Err(error) => {
                                tracing::debug!(%error, "the subject's holes could not be matted; it is matted as solid");
                                plain
                            }
                        }
                    }
                    // No hole found, or none looked for: the slot the seeded matte would have taken is passed over.
                    _ => {
                        if holes {
                            let left = remaining.load(Ordering::Relaxed);
                            remaining.store(left.saturating_sub(1), Ordering::Relaxed);
                        }
                        plain
                    }
                };
                // Where Bilbao is sure of the subject, Porto's hesitation made whole.
                let found = match firm {
                    Some(saliency) => {
                        spawn_blocking::<_, InferenceError, _>(move || firmed(&found, &saliency).unwrap_or(found))
                            .await?
                    }
                    None => found,
                };
                // And the brush over the matte, last, so neither the join nor the firming undoes it: at least the focus
                // painted, and less the blur.
                let found = match refine {
                    Some(refine) => {
                        spawn_blocking::<_, InferenceError, _>(move || Refinement::matte(&refine, &found)).await?
                    }
                    None => found,
                };
                bench_dump("matte", found.dimensions(), found.plane().as_slice());
                outcome = outcome.and(ChainMatte::Matted);
                matted.push(Operation::LensBlur(blur.with_matte(Arc::new(found), Some(Arc::new(refined)))));
            }
            Err(error) if error.stop_reason().is_some() => return Err(error),
            Err(error) if not_installed(&error) => {
                tracing::debug!(%error, "the matting model is not installed; every lens blur renders from the depth");
                outcome = outcome.and(ChainMatte::Unavailable(error));
                matted.push(Operation::LensBlur(blur));
            }
            Err(error) => {
                tracing::debug!(%error, "the lens blur's subject could not be matted; it renders from the depth");
                outcome = outcome.and(ChainMatte::Failed(error));
                matted.push(Operation::LensBlur(blur));
            }
        }
    }

    Ok((matted, outcome))
}

/// Whether `error` says the model is not installed and no source publishes it: the matting graph, not yet published
/// with the others, everywhere it was never put by hand.
fn not_installed(error: &InferenceError) -> bool {
    matches!(error, InferenceError::Install(init) if matches!(**init, InitError::UnpublishedModel { .. }))
}

/// The trimap of `blur`'s subject on `source` and the depth refined to `source` it was drawn on, or why it is not
/// matted: [`ChainMatte::LittleBlur`] where the blur is too small to see, and [`ChainMatte::Skipped`] where it is not
/// worth matting otherwise — no depth, a blur that adds nothing or under 3 px anywhere, or no subject in focus.
fn trimap_of(blur: &LensBlur, source: &image::DynamicImage) -> Result<(Plane, Refined, Band), ChainMatte> {
    let (Some(depth), Some(lens)) = (blur.depth(), blur.lens()) else {
        return Err(ChainMatte::Skipped);
    };
    let diagonal = (source.width() as f32).hypot(source.height() as f32);
    let (x, y) = blur.focus();
    let focus_m = matted_focus(depth.focus_metres(x.get(), y.get()));
    let found = Blur::new(&lens, blur.f_number().as_f32(), focus_m, diagonal).ok_or(ChainMatte::Skipped)?;
    let band = Band::around(focus_m);
    let radii: Vec<f32> = depth.disparity().iter().map(|&disparity| found.radius(disparity)).collect();
    bench_dump("radius", depth.dimensions(), &radii);

    // Before the matting's own test, which a blur too small to see always fails too: this is the one of the two a user
    // is told about. `info`, since it is the answer to "why did nothing happen".
    if imperceptible(&radii, diagonal) {
        tracing::info!(
            focus_m,
            f_number = blur.f_number().as_f32(),
            focal_35mm = lens.focal_35mm,
            "the lens blur adds too little blur to see at this focal length and focus distance"
        );
        return Err(ChainMatte::LittleBlur);
    }

    if !worth_matting(depth, &found, band) {
        tracing::debug!(focus_m, "the lens blur's subject is not matted: the blur is too small or nothing is in focus");
        return Err(ChainMatte::Skipped);
    }

    // What lies behind the subject across a jump in the depth is dropped from the band, at the depth's resolution.
    let cut = cut_at_steps(depth, &found, band, diagonal);
    bench_dump("cut", (cut.width(), cut.height()), cut.as_slice());
    let refined = Refined::of(depth, source);

    let trimap = trimap(&refined, band, Some(&cut));
    bench_dump("trimap", (trimap.width(), trimap.height()), trimap.as_slice());

    Ok((trimap, refined, band))
}

/// `operations` with every lens blur handed `lens` and no depth: the blurs a chain knows add nothing.
fn fill_chain_lens(operations: Vec<Operation>, lens: Lens) -> Vec<Operation> {
    operations
        .into_iter()
        .map(|operation| match operation {
            Operation::LensBlur(blur) => Operation::LensBlur(blur.with_lens(lens)),
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::image::lens_fixtures;
    use crate::models::lens_blur::tests::two_planes;
    use crate::models::{
        FNumber, FloatPrecision, Fraction, LensBlur, LensBlurEdges, LensOrigin, MattingVariant, Scale, Upscale,
    };
    use image::DynamicImage;

    fn blur() -> Operation {
        LensBlur::lisbon(FNumber::new(1.4).expect("in range"), Fraction::CENTRE, Fraction::CENTRE)
    }

    fn upscale() -> Operation {
        Upscale::kyoto(FloatPrecision::Fp32, Scale::new(2.0).expect("in range"))
    }

    /// [`find_chain_depth`] over an estimate answering `answer`, for a picture with no file, and how many times it was
    /// asked.
    fn found(
        operations: Vec<Operation>,
        answer: Result<DepthMap, InferenceError>,
    ) -> (Result<Vec<Operation>, InferenceError>, usize) {
        found_for(operations, answer, Path::new(""), (8, 8))
    }

    /// [`found`] for the picture of `dimensions` read from the file at `path`.
    fn found_for(
        operations: Vec<Operation>,
        answer: Result<DepthMap, InferenceError>,
        path: &Path,
        dimensions: (u32, u32),
    ) -> (Result<Vec<Operation>, InferenceError>, usize) {
        let calls = AtomicUsize::new(0);
        let estimate = |depth: Depth, _options| {
            assert_eq!(depth, Depth::lisbon());
            calls.fetch_add(1, Ordering::SeqCst);
            std::future::ready(answer)
        };

        let outcome = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime")
            .block_on(find_chain_depth(operations, path, dimensions, None, estimate));

        (outcome, calls.load(Ordering::SeqCst))
    }

    #[test]
    fn a_chain_without_a_lens_blur_estimates_nothing_and_is_unchanged() {
        let (outcome, calls) = found(vec![upscale()], Ok(two_planes(8, 8, 1.0, 2.0)));

        assert_eq!(calls, 0);
        assert_eq!(outcome.expect("unchanged"), vec![upscale()]);
    }

    #[test]
    fn every_lens_blur_is_handed_the_one_estimate_and_a_lens() {
        let map = two_planes(8, 8, 1.0, 2.0);
        let (outcome, calls) = found(vec![blur(), upscale(), blur()], Ok(map.clone()));

        assert_eq!(calls, 1, "two lens blurs did not share one estimate");
        let operations = outcome.expect("filled");
        assert_eq!(operations[1], upscale());

        for index in [0, 2] {
            let Operation::LensBlur(blur) = &operations[index] else { panic!("reordered") };
            assert_eq!(blur.depth(), Some(&map));
            // No file, so no EXIF: the depth model's focal length, and an assumed own aperture.
            let lens = blur.lens().expect("a lens");
            assert_eq!((lens.focal_35mm, lens.origin), (36.0, LensOrigin::Assumed));
        }
    }

    #[test]
    fn a_failed_estimate_and_a_stop_each_fail_the_chain() {
        for error in [InferenceError::Cancelled, InferenceError::Unmeasurable { operation: "Lisbon".into(), valid: 3 }]
        {
            let (outcome, _) = found(vec![blur()], Err(error.clone()));

            assert_eq!(outcome.expect_err("no lens blur without depth").to_string(), error.to_string());
        }
    }

    fn blur_at(f_number: f64) -> Operation {
        LensBlur::lisbon(FNumber::new(f_number).expect("in range"), Fraction::CENTRE, Fraction::CENTRE)
    }

    fn lens_blur(operation: &Operation) -> &LensBlur {
        let Operation::LensBlur(blur) = operation else { panic!("{operation:?} is not a lens blur") };

        blur
    }

    #[test]
    fn a_chain_of_blurs_that_add_nothing_estimates_no_depth_and_still_carries_the_lens() {
        // No EXIF, so an assumed own f/8: f/8 and above add nothing.
        for f_number in [8.0, 16.0] {
            let (outcome, calls) = found(vec![blur_at(f_number), upscale()], Ok(two_planes(8, 8, 1.0, 2.0)));

            assert_eq!(calls, 0, "f/{f_number} estimated a depth to add no blur");
            let operations = outcome.expect("filled");
            let blur = lens_blur(&operations[0]);
            assert_eq!(blur.depth(), None);
            assert_eq!(blur.lens().expect("a lens").own_f_number, 8.0);
            assert_eq!(operations[1], upscale());
        }

        // Any blur that does add something needs it, and every blur is handed it.
        let (outcome, calls) = found(vec![blur_at(16.0), blur_at(2.8)], Ok(two_planes(8, 8, 1.0, 2.0)));
        assert_eq!(calls, 1);
        let operations = outcome.expect("filled");
        assert!(operations.iter().all(|operation| lens_blur(operation).depth().is_some()));
    }

    #[test]
    fn a_blur_is_skipped_only_where_the_exif_settles_the_own_f_number_without_the_depth() {
        // 24 mm f/4 with no 35 mm equivalent and no sensor: the crop factor is the depth model's to estimate, so even
        // an f/32 waits for it.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("photo.jpg");
        let recorded = lens_fixtures::fields(Some(24.0), None, Some(4.0), None);
        std::fs::write(&path, lens_fixtures::jpeg(&recorded)).expect("the fixture is written");

        let (outcome, calls) = found_for(vec![blur_at(32.0)], Ok(two_planes(8, 8, 1.0, 2.0)), &path, (8, 8));
        assert_eq!(calls, 1, "the depth model's say on the crop factor was not waited for");
        assert_eq!(lens_blur(&outcome.expect("filled")[0]).lens().expect("a lens").origin, LensOrigin::Estimated);
    }

    /// A `width` x `height` JPEG whose EXIF is that of a 24 mm f/4 lens, 36 mm equivalent, so f/6 of the whole frame.
    fn photograph(dir: &Path, width: u32, height: u32) -> std::path::PathBuf {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(width, height)
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .expect("encodes");
        let (jpeg, exif) = (
            bytes.into_inner(),
            lens_fixtures::jpeg(&lens_fixtures::fields(Some(24.0), Some(36), Some(4.0), None)),
        );

        // The EXIF segment, between the start and end markers of its own file, spliced in after the real file's start.
        let spliced = [&jpeg[..2], &exif[2..exif.len() - 2], &jpeg[2..]].concat();
        let path = dir.join("photo.jpg");
        std::fs::write(&path, spliced).expect("the fixture is written");

        path
    }

    #[test]
    fn a_crop_is_a_picture_smaller_than_its_file_and_its_lens_is_scaled_to_the_framing() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = photograph(dir.path(), 600, 400);

        // Whole: f/10 is above the photograph's own f/6, and adds nothing.
        let (outcome, calls) = found_for(vec![blur_at(10.0)], Ok(two_planes(8, 8, 1.0, 2.0)), &path, (600, 400));
        assert_eq!(calls, 0);
        let lens = lens_blur(&outcome.expect("filled")[0]).lens().expect("a lens");
        assert_eq!((lens.focal_35mm, lens.own_f_number, lens.origin), (36.0, 6.0, LensOrigin::Exif));

        // Cut to half the width and height, the same lens is a sensor half the size: 72 mm at f/12, which f/10 is wider
        // than. The photograph is a crop, so the depth is estimated and the blur is made.
        let (outcome, calls) = found_for(vec![blur_at(10.0)], Ok(two_planes(8, 8, 1.0, 2.0)), &path, (300, 200));
        assert_eq!(calls, 1);
        let lens = lens_blur(&outcome.expect("filled")[0]).lens().expect("a lens");
        assert_eq!((lens.focal_35mm, lens.own_f_number, lens.origin), (72.0, 12.0, LensOrigin::Exif));
    }

    #[test]
    fn the_own_lens_is_the_one_a_run_blurs_from_for_the_picture_as_framed() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = photograph(dir.path(), 600, 400);

        // Whole, and with no framing given: the 36 mm f/6 of the file's EXIF, read and never estimated.
        let whole = own_lens(&path, Some((600, 400)));
        assert_eq!((whole.own_f_number, whole.focal_35mm, whole.origin), (6.0, Some(36.0), LensOrigin::Exif));
        assert_eq!(own_lens(&path, None), whole);

        // Cut to half the width and height: 72 mm at f/12, exactly as `find_chain_depth` resolves it for the same crop.
        let cropped = own_lens(&path, Some((300, 200)));
        assert_eq!((cropped.own_f_number, cropped.focal_35mm, cropped.origin), (12.0, Some(72.0), LensOrigin::Exif));
    }

    #[test]
    fn the_own_lens_of_a_file_that_cannot_be_read_is_the_assumed_f8() {
        let own = own_lens(Path::new("/nowhere/at/all.jpg"), Some((300, 200)));

        assert_eq!((own.own_f_number, own.focal_35mm, own.origin), (8.0, None, LensOrigin::Assumed));
    }

    #[test]
    fn the_framing_is_the_files_diagonal_over_the_pictures_and_never_less_than_one() {
        let diagonal = 600_f32.hypot(400.0);

        assert_eq!(framing_of(Some(diagonal), (600, 400)), 1.0);
        assert!((framing_of(Some(diagonal), (300, 200)) - 2.0).abs() < 1e-5);
        // Turned a quarter, the same diagonal; a few pixels off, still the whole frame.
        assert_eq!(framing_of(Some(diagonal), (400, 600)), 1.0);
        assert_eq!(framing_of(Some(diagonal), (598, 399)), 1.0);
        // A picture larger than its file, no file, and nothing to measure.
        assert_eq!(framing_of(Some(diagonal), (1200, 800)), 1.0);
        assert_eq!(framing_of(None, (300, 200)), 1.0);
        assert_eq!(framing_of(Some(diagonal), (0, 0)), 1.0);
    }

    /// Bilbao's matte of a 960 x 640 photograph whose depth puts a subject in the middle third: the subject, with a
    /// 20 px hole the depth calls solid at its centre.
    fn salient() -> Matte {
        let alpha: Vec<f32> = (0..640_u32)
            .flat_map(|y| (0..960_u32).map(move |x| (x, y)))
            .map(|(x, y)| {
                let subject = (320..640).contains(&x) && (214..426).contains(&y);
                let hole = (470..490).contains(&x) && (310..330).contains(&y);
                if subject && !hole { 1.0 } else { 0.0 }
            })
            .collect();

        Matte::new(960, 640, &alpha)
    }

    /// [`find_chain_matte`] over `operations` handed a two-plane depth and a 36 mm f/6 lens, on a 960 x 640 photograph,
    /// with a matting that answers Bilbao with [`salient`] and Porto with its trimap; and what each matting was asked,
    /// with the trimap's value at the hole's centre.
    fn chain_matted(edges: &[LensBlurEdges]) -> (Vec<Operation>, Vec<(MattingVariant, Option<f32>)>) {
        chain_refined(edges, None)
    }

    /// [`chain_matted`] with every blur refined by `refine`.
    fn chain_refined(
        edges: &[LensBlurEdges],
        refine: Option<&Arc<crate::models::LensBlurRefine>>,
    ) -> (Vec<Operation>, Vec<(MattingVariant, Option<f32>)>) {
        let source = Picture::new(
            "/pictures/bonsai.jpg",
            DynamicImage::ImageRgb8(image::RgbImage::from_pixel(960, 640, image::Rgb([90, 120, 150]))),
            "feedfacefeedface",
        );
        let blurs = edges.iter().map(|&edges| {
            let blur = lens_blur(&blur_at(1.4)).clone().with_edges(edges);
            Operation::LensBlur(match refine {
                Some(refine) => blur.with_refine(Arc::clone(refine)),
                None => blur,
            })
        });
        let operations = fill_chain_depth(
            blurs.collect(),
            &Arc::new(two_planes(96, 64, 1.0, 10.0)),
            Lens::new(36.0, 6.0, LensOrigin::Exif),
        );
        let asked = std::sync::Mutex::new(Vec::new());
        let matte = |matting: Matting, _options| {
            asked
                .lock()
                .unwrap()
                .push((matting.variant(), matting.trimap().map(|trimap| trimap.get(480, 320))));
            std::future::ready(Ok(match matting.trimap() {
                None => salient(),
                Some(trimap) => Matte::new(trimap.width(), trimap.height(), trimap.as_slice()),
            }))
        };

        let (matted, outcome) = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime")
            .block_on(find_chain_matte(operations, &source, |_| None, matte))
            .expect("nothing stopped");
        assert!(matches!(outcome, ChainMatte::Matted), "{outcome:?}");

        (matted, asked.into_inner().unwrap())
    }

    #[test]
    fn a_hole_bilbao_finds_is_matted_over_seeds_and_the_subject_around_it_kept() {
        let (matted, asked) = chain_matted(&[LensBlurEdges::Matte]);

        // Bilbao, then Porto over the depth's trimap, solid at the hole, then over the seeded one, background there.
        let (bilbao, porto) =
            (MattingVariant::Bilbao(FloatPrecision::Fp16), MattingVariant::Porto(FloatPrecision::Fp32));
        assert_eq!(asked, vec![(bilbao, None), (porto, Some(1.0)), (porto, Some(0.0))]);

        let matte = lens_blur(&matted[0]).matte().expect("a matte");
        assert_eq!(matte.alpha_at(480, 320), 0.0, "the hole is not background");
        assert_eq!(matte.alpha_at(400, 300), 1.0, "the subject around it is not the subject");
    }

    #[test]
    fn focus_painted_over_a_hole_keeps_it_sharp_and_blur_painted_over_the_subject_takes_it_out() {
        use crate::mask::Stroke;
        use crate::models::{LensBlurRefine, RefineStroke};

        // Focus over the hole Bilbao finds, blur over a part of the subject, both hard dots 10 px across.
        let dot = |blur, x: f32, y: f32| RefineStroke::new(blur, Stroke::new(false, 0.005, 0.0, vec![x, y]));
        let refine = Arc::new(LensBlurRefine::new(vec![dot(false, 0.5, 0.5), dot(true, 400.5 / 960.0, 300.5 / 640.0)]));
        let (matted, asked) = chain_refined(&[LensBlurEdges::Matte], Some(&refine));

        // Porto is asked about the hole over the seeded trimap, not told it is background: no seed under focus.
        let porto = MattingVariant::Porto(FloatPrecision::Fp32);
        assert_eq!(asked[1..], [(porto, Some(1.0)), (porto, Some(0.5))]);

        let blur = lens_blur(&matted[0]);
        let matte = blur.matte().expect("a matte");
        assert_eq!(matte.alpha_at(480, 320), 1.0, "the focus painted over the hole is not the subject");
        assert_eq!(matte.alpha_at(400, 300), 0.0, "the blur painted over the subject is not background");
        assert_eq!(matte.alpha_at(360, 260), 1.0, "the subject beside the blur was taken out with it");
        assert!(blur.cache_tag().ends_with(&format!("-r{:016x}", refine.signature())), "{}", blur.cache_tag());

        // Without a stroke, the matte of the same chain is the plain one, the hole background.
        let (plain, _) = chain_matted(&[LensBlurEdges::Matte]);
        assert_eq!(lens_blur(&plain[0]).matte().expect("a matte").alpha_at(480, 320), 0.0);
    }

    #[test]
    fn bilbao_runs_once_a_chain_and_never_for_a_subject_matted_as_solid() {
        let (bilbao, porto) =
            (MattingVariant::Bilbao(FloatPrecision::Fp16), MattingVariant::Porto(FloatPrecision::Fp32));
        let variants = |asked: Vec<(MattingVariant, Option<f32>)>| {
            asked.into_iter().map(|(variant, _)| variant).collect::<Vec<_>>()
        };

        let (_, asked) = chain_matted(&[LensBlurEdges::Matte, LensBlurEdges::Matte]);
        assert_eq!(variants(asked), vec![bilbao, porto, porto, porto, porto]);

        let (matted, asked) = chain_matted(&[LensBlurEdges::Solid]);
        assert_eq!(variants(asked), vec![porto]);
        let matte = lens_blur(&matted[0]).matte().expect("a matte");
        assert_eq!(matte.alpha_at(480, 320), 1.0, "a solid subject was given its hole");
    }

    #[test]
    fn the_analyses_asked_count_bilbao_once_and_a_second_matte_for_each_subject_with_holes() {
        let with = |edges: &[LensBlurEdges]| {
            mattes_asked(
                &edges
                    .iter()
                    .map(|&edges| Operation::LensBlur(lens_blur(&blur_at(1.4)).clone().with_edges(edges)))
                    .collect::<Vec<_>>(),
            )
        };

        assert_eq!(with(&[]), 0);
        assert_eq!(with(&[LensBlurEdges::Depth]), 0);
        assert_eq!(with(&[LensBlurEdges::Solid]), 1);
        assert_eq!(with(&[LensBlurEdges::Matte]), 3);
        assert_eq!(with(&[LensBlurEdges::Matte, LensBlurEdges::Matte, LensBlurEdges::Solid]), 6);
    }
}
