//! The trimap a lens blur mattes its subject from: what the depth says is surely the subject in focus, surely not, and
//! too close to the edge between them to tell.
//!
//! The subject is what lies within a band of distances around the plane of focus, from `focus / 1.1` in front of it to
//! `focus x 1.5` behind it: tight in front, so that a nearer person is not part of the subject and keeps the blur that
//! covers it, and generous behind, because hair, fur and the tips of leaves read farther than the face or the canopy
//! they belong to. Within 0.7 percent of the long side of the band's edge the depth is not to be trusted — the depth
//! model's edge is soft and the guided filter's only follows the photograph's own — so that ring is left unknown for
//! the matting model, and only what lies beyond it either way is sure. A pixel nearer than the band is never the
//! subject, in the ring or not.
//!
//! **Behind the subject, a jump.** Within the band, what the depth parts from the subject in focus by a jump in the
//! blur (the wall close behind a chair) is not the subject, and [`cut_at_steps`] drops it; what a surface joins to it
//! (the head above the shoulders, the floor under the chair) is kept, however far into the band it reaches.
//!
//! **The subject's strands.** The depth cannot see a single hair against the sky either: it reads at the sky's distance,
//! and the trimap calls it surely background. [`widened`] leaves unknown what Bilbao gives any coverage to and is joined
//! to the subject's unknown ring, so the matting model is asked about the strands.
//!
//! **The subject's holes.** The depth cannot see the background between a canopy's leaves or through the loop of a
//! strap: it calls the subject there solid, or unknown in patches too large for the matting model, which then answers
//! about a half — half the sharp gap over the blurred fill, a smudge. [`seeded`] reads Bilbao's matte of the whole
//! photograph for those holes and plants sure background in them, so the matting model draws each gap's edge; and
//! [`joined`] keeps the matte without the seeds wherever Bilbao says the subject is, so a thin part of the subject
//! beside a seed (a cable, a twig) is not handed to the background with it.

// How it is built (the OpenSpec design of `add-lens-blur-8-matting`, X5): the band on the guided-upsampled depth, the
// morphology at a quarter of the size (a 40-pixel square at 24 megapixels is ten pixels there, and the band's edge need
// not be finer than the depth's), enlarged back by the nearest pixel, and a full-size pixel on the wrong side of the
// depth's own edge after that enlargement made unknown rather than sure.
//
// The distance focused at is rounded to 2.5 percent before the band is drawn around it, so that a matte, which takes
// about 15 s, is shared by nearby clicks on one subject, which nobody could tell apart.
//
// The band is of distances alone, never bounded by the blur. Bounding it so that its edge is blurred by no more than a
// few pixels — so that a floor running through it shows no step — was tried and refused: at 181 mm f/1.4 a 3 px bound
// is a slab a few centimetres deep, which cuts through the face and the sign held in front of it, and the matting model
// then mattes the cut, so the subject comes apart in smeared patches; 12 px still splits the sign. The subject is one
// object, and a continuous surface through the band's edge keeps its step.

use image::RgbImage;
use imaging::morph::{dilated, eroded};
use imaging::plane::Plane;
use imaging::rows::for_each_row;

use super::lens::Blur;
use super::render::Refined;
use crate::models::{DepthMap, Matte};

/// How much nearer than the plane of focus the subject reaches, as a ratio of distances.
const NEAR_RATIO: f32 = 1.1;

/// How much farther than the plane of focus the subject reaches, as a ratio of distances.
const FAR_RATIO: f32 = 1.5;

/// The ratio the distance focused at is rounded to a power of before the band is drawn around it: 2.5 percent.
const FOCUS_STEP: f32 = 1.025;

/// How far either side of the band's edge is unknown, as a fraction of the photograph's long side: 39 px on a
/// 5562-pixel-wide photograph.
pub(crate) const UNKNOWN: f32 = 0.007;

/// The factor the band is reduced by before the morphology.
const REDUCTION: u32 = 4;

/// The largest blur, in pixels at the photograph's size, under which a matte is not worth estimating: an edge blurred
/// by less shows no curls and no haze to remove.
const MIN_RADIUS: f32 = 3.0;

/// The smallest share of the photograph the band must cover for a matte to be estimated: below it, there is no subject
/// in focus to speak of.
const MIN_BAND: f64 = 0.005;

/// The least difference in blur across which the depth is a jump rather than a surface, as a share of the photograph's
/// diagonal: 2 px on a 1,620 x 1,080 photograph, 7 px at 24 megapixels, so one scene is cut alike at any size.
const STEP: f32 = 0.001;

/// How far apart, as a share of the depth map's long side, the two sides of a jump are looked for: the depth model's
/// edge is soft, spread over a few of its pixels, so neighbours alone never differ by a jump.
const STEP_SPAN: f32 = 0.005;

/// The least share of the depth map a part of the band cut off by jumps must cover to be dropped: a smaller one is the
/// subject's own (a petal's tip, the end of a slat) that the soft depth happens to isolate.
const MIN_CUT: f64 = 0.01;

/// The band of disparities (`1 / m`) the subject lies in, around a plane of focus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Band {
    /// The farthest the subject reaches, as a disparity: the smaller bound.
    far: f32,
    /// The nearest, as a disparity: the larger bound.
    near: f32,
}

impl Band {
    /// The band around a plane of focus `focus_m` metres away: see [`matted_focus`] for the distance a matted lens
    /// blur focuses at.
    pub(crate) fn around(focus_m: f32) -> Self {
        let focus = focus_m.recip();

        Self { far: focus / FAR_RATIO, near: focus * NEAR_RATIO }
    }

    /// Whether a pixel at `disparity` is in the band.
    pub(crate) fn holds(self, disparity: f32) -> bool {
        (self.far..=self.near).contains(&disparity)
    }

    /// Whether a pixel at `disparity` lies behind the band.
    pub(crate) fn behind(self, disparity: f32) -> bool {
        disparity < self.far
    }

    /// Whether a pixel at `disparity` lies in front of the band.
    pub(crate) fn in_front(self, disparity: f32) -> bool {
        disparity > self.near
    }
}

/// The distance a matted lens blur focuses at, for a focus point whose depth reads `focus_m` metres: rounded to a power
/// of [`FOCUS_STEP`], so two clicks on one subject a few centimetres apart draw one band, one trimap and one matte.
pub(crate) fn matted_focus(focus_m: f32) -> f32 {
    // A distance that has no logarithm is left as it is.
    if focus_m.is_finite() && focus_m > 0.0 {
        FOCUS_STEP.powf((focus_m.ln() / FOCUS_STEP.ln()).round())
    } else {
        focus_m
    }
}

/// Whether a lens blur's subject is worth matting: the blur reaches [`MIN_RADIUS`] somewhere in the photograph, and
/// `band` holds at least [`MIN_BAND`] of it.
///
/// `blur` is the blur at the size the matte is estimated at, the photograph's own, and both questions are asked of
/// `map` at its own resolution, which is enough for either and costs nothing.
pub(crate) fn worth_matting(map: &DepthMap, blur: &Blur, band: Band) -> bool {
    let disparity = map.disparity();
    let (nearest, farthest) = disparity
        .iter()
        .fold((0.0_f32, f32::INFINITY), |(near, far), &value| (near.max(value), far.min(value)));
    let largest = blur.radius(nearest).abs().max(blur.radius(farthest).abs());

    let held = disparity.iter().filter(|&&value| band.holds(value)).count();

    #[allow(clippy::cast_precision_loss, reason = "a share of a pixel count")]
    let share = held as f64 / disparity.len().max(1) as f64;

    largest >= MIN_RADIUS && share >= MIN_BAND
}

/// What of `band` lies behind the subject in focus across a jump in the depth, at `map`'s resolution: 1 where a pixel
/// of the band is dropped from the subject, 0 elsewhere. `blur` is the blur on a photograph `diagonal_px` across.
///
/// The band reaches a fixed 1.5 times the distance focused at, so a wall close behind a chair is in it and would be
/// held sharp with the chair. What the depth joins to the subject by a surface — her head above her shoulders, a
/// petal curving away, the floor the chair stands on — is kept, however far it reaches; what it parts from the subject
/// by a jump of [`STEP`] in blur or more is dropped, where it is large enough ([`MIN_CUT`]) to be something else. The
/// near side is never dropped: the band keeps it tight already.
pub(crate) fn cut_at_steps(map: &DepthMap, blur: &Blur, band: Band, diagonal_px: f32) -> Plane {
    let (width, height) = map.dimensions();
    let (columns, rows) = (width as usize, height as usize);
    let disparity = map.disparity();
    let step = STEP * diagonal_px;
    let radii = Plane::from_vec(width, height, disparity.iter().map(|&value| blur.radius(value)).collect())
        .expect("one radius a pixel");

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a span of a few pixels")]
    let span = ((STEP_SPAN * width.max(height) as f32).round() as u32).max(1);
    // A jump is where the blur within the span either way differs by a step: the soft edge of a wall behind the chair,
    // never the gentle slope of a face turning away, nor the few tenths of a pixel between two leaves of a canopy.
    let (least, most) = (eroded(&radii, span), dilated(&radii, span));
    let (radius, least, most) = (radii.as_slice(), least.as_slice(), most.as_slice());
    let held = |index: usize| band.holds(disparity[index]);
    // In focus: sharp to within half a step, or nearer than the focus. The subject grows from it.
    let sharp = |index: usize| radius[index] >= -step / 2.0;
    let open = |index: usize| held(index) && (sharp(index) || most[index] - least[index] < step);

    // The parts of the band the jumps divide it into, by a fill from each pixel no part has reached yet; a part is
    // kept where it holds a pixel in focus, or is too small to be anything but the subject's.
    let mut part = vec![u32::MAX; columns * rows];
    let mut kept: Vec<bool> = Vec::new();
    let mut stack = Vec::new();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "a share of a pixel count"
    )]
    let min_cut = (MIN_CUT * part.len() as f64) as usize;
    for start in 0..part.len() {
        if part[start] != u32::MAX || !open(start) {
            continue;
        }

        #[allow(clippy::cast_possible_truncation, reason = "fewer parts than pixels")]
        let label = kept.len() as u32;
        let (mut size, mut in_focus) = (0_usize, false);
        part[start] = label;
        stack.push(start);
        while let Some(index) = stack.pop() {
            size += 1;
            in_focus |= sharp(index);

            let (x, y) = (index % columns, index / columns);
            let neighbours = [
                (x > 0).then(|| index - 1),
                (x + 1 < columns).then(|| index + 1),
                (y > 0).then(|| index - columns),
                (y + 1 < rows).then(|| index + columns),
            ];
            for next in neighbours.into_iter().flatten() {
                if part[next] == u32::MAX && open(next) {
                    part[next] = label;
                    stack.push(next);
                }
            }
        }
        kept.push(in_focus || size < min_cut);
    }

    // Grown back by the span over the jumps themselves, so the subject's own side of its edge is never dropped: what
    // that takes of the wall beside it lies within the unknown ring, for the matting model to tell by its colour.
    let joined =
        plane_of(width, height, |x, y| part[y * columns + x] != u32::MAX && kept[part[y * columns + x] as usize]);
    let joined = dilated(&joined, span);

    plane_of(width, height, |x, y| held(y * columns + x) && joined.as_slice()[y * columns + x] < 0.5)
}

/// The trimap of the subject `band` holds, at the size of `refined`, the depth guided to the photograph: 1 where it is
/// surely the subject, 0 where it is surely not, 0.5 between. `cut`, at any size (the depth map's), drops from the band
/// what [`cut_at_steps`] parts from the subject.
pub(crate) fn trimap(refined: &Refined, band: Band, cut: Option<&Plane>) -> Plane {
    let (width, height) = (refined.plane.width(), refined.plane.height());
    let (columns, rows) = (width as usize, height as usize);
    let values = refined.plane.as_slice();
    let disparity = |index: usize| values[index] * refined.nearest;
    // The cut's pixel under a pixel of the photograph, by the nearest.
    let cut_at = |index: usize| {
        cut.is_some_and(|cut| {
            let (x, y) = (index % columns, index / columns);
            #[allow(clippy::cast_possible_truncation, reason = "a pixel of a plane no larger than the photograph")]
            let (cut_x, cut_y) = (
                (x as u64 * u64::from(cut.width()) / columns as u64) as u32,
                (y as u64 * u64::from(cut.height()) / rows as u64) as u32,
            );
            cut.get(cut_x, cut_y) > 0.5
        })
    };
    let held = |index: usize| band.holds(disparity(index)) && !cut_at(index);

    // The band reduced by a box: a reduced pixel is in it where most of its block is. And the band with what is in
    // front of it, likewise: the subject's edge against a nearer thing is that thing's edge, not one to matte.
    let (small_width, small_height) = (width.div_ceil(REDUCTION), height.div_ceil(REDUCTION));
    let in_front = |index: usize| band.in_front(disparity(index));
    let reduced = |inside_of: &(dyn Fn(usize) -> bool + Sync)| {
        let mut small = Plane::new(small_width, small_height, 0.0);
        for_each_row(small.as_mut_slice(), small_width as usize, small_height as usize, |y, out| {
            let top = y * REDUCTION as usize;
            let bottom = (top + REDUCTION as usize).min(rows);

            for (x, value) in out.iter_mut().enumerate() {
                let left = x * REDUCTION as usize;
                let right = (left + REDUCTION as usize).min(columns);
                let inside = (top..bottom).flat_map(|row| (left..right).map(move |column| row * columns + column));
                let (count, total) =
                    inside.fold((0, 0), |(count, total), index| (count + usize::from(inside_of(index)), total + 1));

                *value = if 2 * count > total { 1.0 } else { 0.0 };
            }
        });
        small
    };
    let small = reduced(&held);
    let unoccluded = reduced(&|index| held(index) || in_front(index));

    // Rounded up, so the ring is at least the 0.7 percent asked for at the full size.
    let reduced_radius = unknown_radius(width, height).div_ceil(REDUCTION).max(1);
    // Sure where the band, with what is in front of it, holds the whole ring: an edge against a nearer thing is not
    // left unknown, or the matting model, which cannot see depth, takes the subject's side of it for background.
    let (sure, maybe) = (eroded(&unoccluded, reduced_radius), dilated(&small, reduced_radius));

    // What is in front of the band, opened by a pixel: at a bright edge against a dark background the guided depth
    // overshoots, so a leaf's rim reads nearer than the band in dots and one-pixel arcs, which, sure background, the
    // matting model cannot draw over and the blur shows as dark specks along the edge. A nearer thing is wider.
    let front = plane_of(width, height, |x, y| in_front(y * columns + x));
    let front = dilated(&eroded(&front, 1), 1);

    let mut trimap = Plane::new(width, height, 0.0);
    for_each_row(trimap.as_mut_slice(), columns, rows, |y, out| {
        let small_y = (y / REDUCTION as usize) as u32;

        for (x, value) in out.iter_mut().enumerate() {
            let small_x = (x / REDUCTION as usize) as u32;
            let index = y * columns + x;
            let inside = held(index);

            *value = if front.as_slice()[index] > 0.5 {
                // A nearer thing — a person passing in front — is never the subject, however close to its edge: left
                // unknown, the matting model gives its edge to the subject, and it is then rendered sharp.
                0.0
            } else if sure.get(small_x, small_y) > 0.5 && small.get(small_x, small_y) > 0.5 {
                // Sure only where the full-size pixel agrees with the reduced band it was enlarged from.
                if inside { 1.0 } else { 0.5 }
            } else if maybe.get(small_x, small_y) > 0.5 || inside {
                0.5
            } else {
                0.0
            };
        }
    });

    trimap
}

// The strands. The depth model cannot see a single hair against the sky, so a backlit portrait's flyaway hair reads at
// the sky's distance, the trimap calls it surely background a ring or more from the head, and the matting model is never
// asked about it: on the bench's backlit portrait (`o_f12a8afc`) 70 percent of the ground truth's strands beyond the
// head's outline were sure background. Bilbao, which mattes the whole photograph from its colours alone, sees a share
// of them as a faint alpha (a tenth or two). So the trimap's sure background is left unknown where Bilbao gives it any
// coverage and the patch reaches the unknown ring around the subject: the strands that grow from the head, and not a
// faint halo Bilbao leaves around something else in the photograph. Never in front of the band: a nearer thing is not
// the subject, however salient.

/// Bilbao's coverage above which a pixel the depth calls surely background may be a strand of the subject's.
const STRAND: f32 = 0.05;

/// `trimap` with its sure background left unknown where Bilbao's `saliency` sees a strand of the subject joined to its
/// unknown ring, and in the small islands of it Bilbao calls the subject (false holes, [`false_hole_area`]), never in
/// front of `band` by `refined`, the depth the trimap was drawn on; or `None` where that changes nothing, or Bilbao
/// found another subject than the depth (see [`MIN_SALIENT`]).
pub(crate) fn widened(trimap: &Plane, saliency: &Matte, refined: &Refined, band: Band) -> Option<Plane> {
    let (width, height) = (trimap.width(), trimap.height());
    let (columns, rows) = (width as usize, height as usize);
    if (refined.plane.width(), refined.plane.height()) != (width, height) {
        return None;
    }
    let salient = saliency.plane().resized(width, height);
    let (known, salience, depth) = (trimap.as_slice(), salient.as_slice(), refined.plane.as_slice());
    agreed_share(known, salience)?;

    let candidate = |index: usize| {
        known[index] == 0.0 && salience[index] > STRAND && !band.in_front(depth[index] * refined.nearest)
    };

    // A fill from the unknown ring over the candidates.
    let mut reached = vec![false; known.len()];
    let mut stack: Vec<usize> = (0..known.len()).filter(|&index| known[index] == 0.5).collect();
    while let Some(index) = stack.pop() {
        let (x, y) = (index % columns, index / columns);
        let neighbours = [
            (x > 0).then(|| index - 1),
            (x + 1 < columns).then_some(index + 1),
            (y > 0).then(|| index - columns),
            (y + 1 < rows).then_some(index + columns),
        ];
        for next in neighbours.into_iter().flatten() {
            if !reached[next] && candidate(next) {
                reached[next] = true;
                stack.push(next);
            }
        }
    }

    // Grown by the seeds' inset, so a strand Bilbao draws thinner than it is, at its graph's size, is unknown across.
    let grown = dilated(
        &plane_of(width, height, |x, y| reached[y * columns + x]),
        seed_inset(unknown_radius(width, height)),
    );
    let mut out = trimap.clone();
    let mut changed = 0_usize;
    for (index, value) in out.as_mut_slice().iter_mut().enumerate() {
        if *value == 0.0 && grown.as_slice()[index] > 0.5 && !band.in_front(depth[index] * refined.nearest) {
            *value = 0.5;
            changed += 1;
        }
    }
    let strands = changed;

    // The depth's false holes: an island of sure background within the subject, too small to be the background around
    // it, where Bilbao calls the photograph the subject. The guided depth overshoots at a bright detail, so the watch
    // face and the buckle on a backlit wrist read nearer than the band, in dots the opening of what is in front keeps;
    // sure background, they were holes in the matte the fill's sky showed through, as pale patches on the watch. In
    // front of the band or not: a nearer thing that is an island within the subject, and that Bilbao calls the subject,
    // is the depth's overshoot, never a passer-by, whose outline reaches the background around the subject.
    let background: Vec<bool> = known.iter().map(|&value| value == 0.0).collect();
    let out_values = out.as_mut_slice();
    for index in islands(&background, columns, false_hole_area(unknown_radius(width, height))) {
        if out_values[index] == 0.0 && salience[index] > SALIENT {
            out_values[index] = 0.5;
            changed += 1;
        }
    }

    tracing::debug!(strands, false_holes = changed - strands, "the subject's strands and false holes left unknown");

    (changed > 0).then_some(out)
}

/// The largest island of sure background, in pixels, that may be a false hole in the subject: a square four `ring`s on a
/// side, 1,936 px on the bench's photographs. The background around a subject is one island far larger.
fn false_hole_area(ring: u32) -> usize {
    (4 * ring as usize).pow(2)
}

/// Whether Bilbao's `salience` (at the trimap's size) found the subject `known` (a trimap) holds: of what the trimap
/// surely calls the subject, the share Bilbao calls the subject too, where it is [`MIN_SALIENT`] or more, or else where
/// [`MIN_WITHIN`] or more of what Bilbao calls the subject lies within the trimap's (sure or unknown). `None` where
/// neither holds, or nothing is sure.
fn agreed_share(known: &[f32], salience: &[f32]) -> Option<f64> {
    let (sure, agreed, salient, within) = known.iter().zip(salience).fold(
        (0_usize, 0_usize, 0_usize, 0_usize),
        |(sure, agreed, salient, within), (&value, &salience)| {
            let (is_sure, is_salient) = (value == 1.0, salience > SALIENT);
            (
                sure + usize::from(is_sure),
                agreed + usize::from(is_sure && is_salient),
                salient + usize::from(is_salient),
                within + usize::from(is_salient && value > 0.0),
            )
        },
    );
    #[allow(clippy::cast_precision_loss, reason = "shares of pixel counts")]
    let (share, inside) = (agreed as f64 / sure.max(1) as f64, within as f64 / salient.max(1) as f64);

    (sure > 0 && (share >= MIN_SALIENT || salient > 0 && inside >= MIN_WITHIN)).then_some(share)
}

// The holes (the "B7" of the prototype in the fork's notes, `lens-blur/gaps`, with the refinements its gate on two more
// photographs called for). On the bonsai of that prototype, a ring of 34 px: Bilbao's subject (its matte above a half)
// closed by 68 px at a quarter of the size, which fills every hole in it smaller than that and leaves the outline; the
// sure background planted where Bilbao is below a tenth, 4 px in from it, within that closed subject alone; and the
// sure subject within 17 px of what Bilbao calls background, within it too, made unknown. Every radius is the ring's
// multiple, so it scales with the photograph as the ring does.
//
// Within the closed subject alone, both: a large thing the depth puts in the band and Bilbao does not find salient (the
// table under a bonsai's pot, a glass case before a woman's hands) is not a hole, and keeps the depth's answer. Both
// were tried everywhere in the band first: the table blurred, and the case came apart in patches.
//
// The join: seeded, the matting model took both strands of a cable beside a seeded gap for background (Bilbao reads
// them at 0.5 to 0.98). So where Bilbao calls a pixel the subject (above a half, grown by the seeds' 4 px), the matte is
// the plain one; elsewhere, in the holes, the smaller of the two. It was the larger of the two on the subject and the
// seeded one in the holes at first: the tip of a nearer woman's strap, which the depth leaves out (plain alpha about 0)
// and Bilbao finds salient, came back from the seeded matte at up to 0.7, held sharp in front of the subject.

/// Bilbao's coverage above which a pixel is the salient subject.
const SALIENT: f32 = 0.5;

/// Bilbao's coverage below which a pixel inside the subject's outline is a hole: sure background.
const HOLE: f32 = 0.1;

/// The least share of the depth's sure subject that Bilbao must find salient for its holes to be trusted: below it,
/// Bilbao has found another subject than the one in focus (focused on the street behind a passer-by, the passer-by),
/// and the trimap is left as the depth drew it.
const MIN_SALIENT: f64 = 0.25;

/// The least share of Bilbao's subject that must lie within the depth's for its holes to be trusted however little of
/// the depth's sure subject it covers: a chair on a floor the band holds from wall to wall is a fifth of the depth's
/// subject, and the gaps between its slats went unseeded, the fence behind held sharp in one of them. Bilbao's subject
/// within the depth's is a part of it, never another subject (a passer-by is nearer than the band).
const MIN_WITHIN: f64 = 0.9;

/// The trimap `trimap` with sure background planted in the holes Bilbao's `saliency` finds in the subject, and the sure
/// subject beside them unknown; or `None` where that changes nothing, or Bilbao found another subject than the depth.
///
/// `saliency` is at any size (Bilbao's graph's); it is resampled to the trimap's.
pub(crate) fn seeded(trimap: &Plane, saliency: &Matte, photo: Option<&RgbImage>) -> Option<Plane> {
    let (width, height) = (trimap.width(), trimap.height());
    let (columns, rows) = (width as usize, height as usize);
    let ring = unknown_radius(width, height);
    let salient = saliency.plane().resized(width, height);
    let (known, salience) = (trimap.as_slice(), salient.as_slice());

    // The guard: of what the depth surely calls the subject, how much Bilbao calls it too.
    let Some(share) = agreed_share(known, salience) else {
        tracing::debug!("the subject's holes are not looked for: the salient subject is another");
        return None;
    };

    // Bilbao's subject closed by twice the ring, at a quarter of the size: its outline, with its holes filled.
    let (small_width, small_height) = (width.div_ceil(REDUCTION), height.div_ceil(REDUCTION));
    let reduction = REDUCTION as usize;
    let small_salient =
        plane_of(small_width, small_height, |x, y| salience[y * reduction * columns + x * reduction] > SALIENT);
    let closing = (2 * ring).div_ceil(REDUCTION).max(1);
    let outline = eroded(&dilated(&small_salient, closing), closing);
    // Seeds a block further in: the block on the outline itself holds the edge between Bilbao's subject and what is
    // beside it, and a seed there would be a sliver of background against the sure subject.
    let inside = eroded(&outline, 1);
    let at = |plane: &Plane, x: usize, y: usize| plane.get((x / reduction) as u32, (y / reduction) as u32) > 0.5;

    // The sure subject beside what Bilbao calls background, by half the ring, at the quarter size the ring is drawn at:
    // a block is beside it where most of the block is.
    let beside = |x: usize, y: usize| known[y * columns + x] == 1.0 && salience[y * columns + x] < SALIENT;
    let small_beside = plane_of(small_width, small_height, |x, y| {
        let (top, left) = (y * reduction, x * reduction);
        let (bottom, right) = ((top + reduction).min(rows), (left + reduction).min(columns));
        let count = (top..bottom)
            .flat_map(|row| (left..right).map(move |column| (column, row)))
            .filter(|&(column, row)| beside(column, row))
            .count();

        2 * count > (bottom - top) * (right - left)
    });
    let near_hole = dilated(&small_beside, (ring / 2).div_ceil(REDUCTION).max(1));

    // The holes: Bilbao's background inside the band, in from its edge by an eighth of the ring.
    let hole = plane_of(width, height, |x, y| known[y * columns + x] > 0.0 && salience[y * columns + x] < HOLE);
    let mut seeds = eroded(&hole, seed_inset(ring));
    if let Some(photo) = photo {
        let planted = |index: usize| {
            at(&outline, index % columns, index / columns) && at(&inside, index % columns, index / columns)
        };
        unseed_the_subjects_colour(&mut seeds, trimap, photo, ring, planted);
    }

    let mut out = trimap.clone();
    for_each_row(out.as_mut_slice(), columns, rows, |y, out| {
        for (x, value) in out.iter_mut().enumerate() {
            if !at(&outline, x, y) {
                continue;
            }
            if seeds.as_slice()[y * columns + x] > 0.5 && at(&inside, x, y) {
                *value = 0.0;
            } else if *value == 1.0 && at(&near_hole, x, y) {
                *value = 0.5;
            }
        }
    });
    let changed = out.as_slice().iter().zip(known).filter(|(seeded, plain)| seeded != plain).count();

    tracing::debug!(share, changed, "the subject's holes seeded");

    (changed > 0).then_some(out)
}

/// The matte of the subject from `seeded`, the matte over the [`seeded`] trimap, and `plain`, the one over the trimap the
/// depth drew: the plain one wherever Bilbao's `saliency` calls a pixel the subject (grown by the seeds' inset), and the
/// smaller of the two elsewhere, in the holes. Seeds only add background, so a seeded alpha above the plain one is the
/// matting model reading a changed neighbourhood (a nearer strap Bilbao finds salient drawn back as the subject), never
/// the subject.
pub(crate) fn joined(plain: &Matte, seeded: &Matte, saliency: &Matte) -> Matte {
    let (width, height) = seeded.dimensions();
    debug_assert_eq!(plain.dimensions(), (width, height), "two mattes of one photograph");

    let ring = unknown_radius(width, height);
    let salient = saliency.plane().resized(width, height);
    let subject = dilated(
        &plane_of(width, height, |x, y| salient.as_slice()[y * width as usize + x] > SALIENT),
        seed_inset(ring),
    );
    let (plain, seeded) = (plain.coverage(), seeded.coverage());

    let mut alpha: Vec<f32> = seeded
        .iter()
        .zip(&plain)
        .zip(subject.as_slice())
        .map(|((&seeded, &plain), &subject)| if subject > 0.5 { plain } else { seeded.min(plain) })
        .collect();

    // The specks: what the seeds took from the subject in islands too small to be a hole — a seed on a leaf's rim
    // Bilbao blurred into the background, the matting model's answer shifting by a few pixels — has the plain matte.
    let lost: Vec<bool> = plain.iter().zip(&alpha).map(|(&plain, &alpha)| plain - alpha > 0.5).collect();
    for index in islands(&lost, width as usize, speck_area(ring)) {
        alpha[index] = plain[index];
    }

    Matte::new(width, height, &alpha)
}

/// The largest island of the subject the seeds may take away and the join gives back as a speck, in pixels: a square a
/// quarter of the `ring` on a side, 64 px for a ring of 34.
fn speck_area(ring: u32) -> usize {
    (ring as usize / 4).pow(2).max(1)
}

/// The pixels of every island of `inside` (a `columns`-wide mask, its pixels joined by their four neighbours) smaller
/// than `area`.
fn islands(inside: &[bool], columns: usize, area: usize) -> Vec<usize> {
    let mut seen = vec![false; inside.len()];
    let (mut small, mut island, mut stack) = (Vec::new(), Vec::new(), Vec::new());

    for start in 0..inside.len() {
        if !inside[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        island.clear();

        while let Some(index) = stack.pop() {
            island.push(index);
            let (x, y) = (index % columns, index / columns);
            let neighbours = [
                (x > 0).then(|| index - 1),
                (x + 1 < columns).then_some(index + 1),
                (y > 0).then(|| index - columns),
                (index + columns < inside.len()).then_some(index + columns),
            ];
            for next in neighbours.into_iter().flatten() {
                if inside[next] && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        if island.len() < area {
            small.extend_from_slice(&island);
        }
    }

    small
}

/// How much nearer the sure subject's colour around it a seed's colour must be than the sure background's, as a share
/// of its distance from the background's, for the seed to be taken for the subject.
const SUBJECT_COLOURED: f32 = 0.5;

/// `seeds` less every seed (of the pixels `planted` keeps) whose colour in `photo` is the sure subject's around it
/// rather than the sure background's: a seed on a thin stalk or stem Bilbao cannot see, which would take a piece of
/// the subject for background and blur it. Pixel by pixel, since one region of seeds can straddle a stalk's tip and
/// the background beside it. Around means within two and a half rings, from the means of `trimap`'s sure subject and
/// sure background over blocks a ring on a side. A seed with too little of either around it stays: deep in a canopy
/// there is no sure background near, and that is what the seeds are for.
fn unseed_the_subjects_colour(
    seeds: &mut Plane,
    trimap: &Plane,
    photo: &RgbImage,
    ring: u32,
    planted: impl Fn(usize) -> bool,
) {
    let (width, height) = (trimap.width(), trimap.height());
    if photo.dimensions() != (width, height) {
        return;
    }
    let columns = width as usize;
    let block = ring.max(1) as usize;
    let (across, down) = (columns.div_ceil(block), (height as usize).div_ceil(block));
    let raw = photo.as_raw();
    let colour = |index: usize| [0, 1, 2].map(|channel| f32::from(raw[3 * index + channel]));

    // Each block's colour sums and counts of the sure subject (0) and the sure background (1), then the same over the
    // five by five blocks around each block.
    let mut sums = vec![[[0.0_f32; 3]; 2]; across * down];
    let mut counts = vec![[0_u32; 2]; across * down];
    for (index, &value) in trimap.as_slice().iter().enumerate() {
        let class = if value == 1.0 {
            0
        } else if value == 0.0 {
            1
        } else {
            continue;
        };
        let cell = (index / columns / block) * across + (index % columns) / block;
        sums[cell][class].iter_mut().zip(colour(index)).for_each(|(sum, value)| *sum += value);
        counts[cell][class] += 1;
    }
    let around: Vec<Option<[[f32; 3]; 2]>> = (0..across * down)
        .map(|cell| {
            let (cx, cy) = (cell % across, cell / across);
            let mut sum = [[0.0_f32; 3]; 2];
            let mut seen = [0_u32; 2];
            for y in cy.saturating_sub(2)..(cy + 3).min(down) {
                for x in cx.saturating_sub(2)..(cx + 3).min(across) {
                    for class in 0..2 {
                        sum[class].iter_mut().zip(sums[y * across + x][class]).for_each(|(a, s)| *a += s);
                        seen[class] += counts[y * across + x][class];
                    }
                }
            }
            #[allow(clippy::cast_precision_loss, reason = "pixel counts")]
            seen.iter()
                .all(|&seen| seen as usize >= block)
                .then(|| [0, 1].map(|class| sum[class].map(|value| value / seen[class] as f32)))
        })
        .collect();

    let distance = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum::<f32>().sqrt();
    let mut dropped = 0_usize;
    for (index, seed) in seeds.as_mut_slice().iter_mut().enumerate() {
        if *seed <= 0.5 || !planted(index) {
            continue;
        }
        let cell = (index / columns / block) * across + (index % columns) / block;
        let Some([subject, background]) = around[cell] else {
            continue;
        };
        let own = colour(index);
        if distance(own, subject) < SUBJECT_COLOURED * distance(own, background) {
            *seed = 0.0;
            dropped += 1;
        }
    }

    tracing::debug!(dropped, "seeds the subject's colour taken back");
}

/// How far in from Bilbao's background a seed is planted, in pixels: an eighth of the `ring`, rounded, and at least one.
fn seed_inset(ring: u32) -> u32 {
    ((ring + 4) / 8).max(1)
}

/// A `width` x `height` plane of 1 where `inside(x, y)` and 0 elsewhere.
fn plane_of(width: u32, height: u32, inside: impl Fn(usize, usize) -> bool + Sync) -> Plane {
    let mut plane = Plane::new(width, height, 0.0);
    for_each_row(plane.as_mut_slice(), width as usize, height as usize, |y, out| {
        for (x, value) in out.iter_mut().enumerate() {
            *value = f32::from(u8::from(inside(x, y)));
        }
    });

    plane
}

/// How far either side of the band's edge is unknown on a `width` x `height` photograph, in pixels: [`UNKNOWN`] of its
/// long side.
pub(crate) fn unknown_radius(width: u32, height: u32) -> u32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a radius of a few dozen pixels"
    )]
    let radius = (UNKNOWN * width.max(height) as f32).round() as u32;

    radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::lens_blur::lens::{Lens, LensOrigin};

    /// A `width` x `height` depth, already at the photograph's size: `metres(x, y)` at each pixel.
    fn refined(width: u32, height: u32, metres: impl Fn(u32, u32) -> f32) -> Refined {
        let disparity: Vec<f32> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| metres(x, y).recip())
            .collect();
        let nearest = disparity.iter().copied().fold(f32::MIN_POSITIVE, f32::max);

        Refined {
            plane: Plane::from_vec(width, height, disparity.iter().map(|value| value / nearest).collect()).unwrap(),
            nearest,
        }
    }

    #[test]
    fn a_depth_step_is_sure_either_side_of_its_edge_and_unknown_along_it() {
        // A subject at 1 m on the left half of an 800 x 600 photograph, before a wall at 10 m: the unknown band is
        // about 6 px (0.7 percent of 800) either side of column 400, at the quarter size's resolution.
        let depth = refined(800, 600, |x, _| if x < 400 { 1.0 } else { 10.0 });
        let trimap = trimap(&depth, Band::around(1.0), None);
        let radius = unknown_radius(800, 600);
        assert_eq!(radius, 6);

        for y in [0, 300, 599] {
            for x in 0..800 {
                let value = trimap.get(x, y);
                let from_edge = x.abs_diff(400);

                if from_edge > radius + 2 * REDUCTION {
                    assert_eq!(value, if x < 400 { 1.0 } else { 0.0 }, "({x}, {y}) is not sure");
                }
                if from_edge < REDUCTION {
                    assert_eq!(value, 0.5, "({x}, {y}) on the edge is sure");
                }
                // Never sure on the wrong side of the depth's own edge.
                assert!(!(x < 400 && value == 0.0) && !(x >= 400 && value == 1.0), "({x}, {y}): {value}");
            }
        }

        assert!(trimap.as_slice().iter().all(|value| [0.0, 0.5, 1.0].contains(value)));
    }

    #[test]
    fn the_band_is_tight_in_front_and_generous_behind() {
        // Focused at 2 m: 1.9 m and 2.9 m are the subject, 1.7 m (nearer than 2 / 1.1) and 3.2 m (beyond 2 x 1.5) are
        // not.
        let band = Band::around(2.0);

        for (metres, held) in [(1.7, false), (1.9, true), (2.0, true), (2.9, true), (3.2, false)] {
            assert_eq!(band.holds(1.0 / metres), held, "{metres} m");
        }
        assert!(band.behind(1.0 / 3.2));
        assert!(!band.behind(1.0 / 1.7));
    }

    #[test]
    fn a_nearer_thing_outside_the_band_is_background_to_the_subject() {
        // A person nearer than the band, at 0.8 m before a subject focused at 1 m, is surely not the subject away from
        // its edge, as the wall behind is.
        let depth = refined(400, 300, |x, _| match x {
            0..100 => 0.8,
            100..300 => 1.0,
            _ => 10.0,
        });
        let trimap = trimap(&depth, Band::around(1.0), None);

        assert_eq!(trimap.get(20, 150), 0.0);
        assert_eq!(trimap.get(200, 150), 1.0);
        assert_eq!(trimap.get(380, 150), 0.0);

        // Right up to its edge with the subject: that edge is the nearer person's, so neither side of it is left
        // unknown — the person's would be matted into the sharp subject, and the subject's taken for background by a
        // model that cannot see depth. The edge against the wall behind is unknown as before.
        let radius = unknown_radius(400, 300);
        for y in [0, 150, 299] {
            for x in 0..100 {
                assert_eq!(trimap.get(x, y), 0.0, "({x}, {y}) in front of the band is not surely background");
            }
            assert_eq!(trimap.get(100, y), 1.0, "the subject's side of the nearer person's edge is sure");
            assert_eq!(trimap.get(300 + radius / 2, y), 0.5, "the wall's side of the edge is unknown");
        }
    }

    #[test]
    fn nearby_focus_points_on_one_subject_draw_one_band() {
        // Clicks a percent nearer or farther on the same subject focus at the same rounded distance, and so draw the
        // same band, trimap and matte; three percent away is another.
        let step = matted_focus(3.0);
        assert_eq!(matted_focus(step * 1.01), step);
        assert_eq!(matted_focus(step / 1.01), step);
        assert_eq!(Band::around(matted_focus(step * 1.01)), Band::around(step));
        assert!((matted_focus(3.0) / 3.0 - 1.0).abs() <= 0.0125);
        assert_ne!(matted_focus(3.0 * 1.03), matted_focus(3.0));
        assert_eq!(matted_focus(0.0), 0.0);
    }

    /// A scene at `scale` times 1000 x 750: the depth's subject, sure, from (300, 200) to (700, 550), ringed by 7 px of
    /// unknown, with a table below it the depth also calls the subject, (300, 550) to (700, 700); and Bilbao's matte of
    /// it, which finds the subject but not the table, and a 20 px hole in the subject at (490, 370), which the depth
    /// calls solid. What `salient` says is in the subject is Bilbao's.
    fn scene(scale: u32, salient: impl Fn(u32, u32) -> bool) -> (Plane, Matte) {
        let (width, height) = (1000 * scale, 750 * scale);
        let within = |x: u32, y: u32, (left, top, right, bottom): (u32, u32, u32, u32)| {
            (left * scale..right * scale).contains(&x) && (top * scale..bottom * scale).contains(&y)
        };
        let subject = |x: u32, y: u32| within(x, y, (300, 200, 700, 700));
        let ring = |x: u32, y: u32| within(x, y, (293, 193, 707, 707));
        let pixels = || (0..height).flat_map(move |y| (0..width).map(move |x| (x, y)));

        let trimap: Vec<f32> = pixels()
            .map(|(x, y)| {
                if subject(x, y) {
                    1.0
                } else if ring(x, y) {
                    0.5
                } else {
                    0.0
                }
            })
            .collect();
        let saliency: Vec<f32> = pixels().map(|(x, y)| if salient(x, y) { 0.97 } else { 0.02 }).collect();

        (Plane::from_vec(width, height, trimap).unwrap(), Matte::new(width, height, &saliency))
    }

    /// Bilbao's subject at `scale`: the depth's, without the table, with a 20 px hole.
    fn bilbao(scale: u32) -> impl Fn(u32, u32) -> bool {
        move |x, y| {
            let inside = (300 * scale..700 * scale).contains(&x) && (200 * scale..550 * scale).contains(&y);
            let hole = (490 * scale..510 * scale).contains(&x) && (370 * scale..390 * scale).contains(&y);
            inside && !hole
        }
    }

    #[test]
    fn a_strand_bilbao_sees_beyond_the_ring_is_left_unknown_and_a_faint_halo_apart_or_a_nearer_thing_is_not() {
        // A head at 1 m on the left of an 800 x 600 photograph before a sky at 10 m, with a nearer post at 0.5 m on the
        // right. Bilbao sees a strand of hair (0.2) running out of the head 60 px into the sky, a faint smudge (0.2) in
        // the sky well away from the head, and the post (0.9), which stands against the head.
        let depth = refined(800, 600, |x, y| {
            if x < 300 {
                1.0
            } else if (300..330).contains(&x) && y >= 400 {
                0.5
            } else {
                10.0
            }
        });
        let band = Band::around(1.0);
        let plain = trimap(&depth, band, None);
        let strand = |x: u32, y: u32| (300..360).contains(&x) && (200..203).contains(&y);
        let smudge = |x: u32, y: u32| (600..640).contains(&x) && (100..140).contains(&y);
        let post = |x: u32, y: u32| (300..330).contains(&x) && y >= 400;
        let saliency: Vec<f32> = (0..600)
            .flat_map(|y| (0..800).map(move |x| (x, y)))
            .map(|(x, y)| {
                if x < 300 || post(x, y) {
                    0.9
                } else if strand(x, y) || smudge(x, y) {
                    0.2
                } else {
                    0.0
                }
            })
            .collect();
        let saliency = Matte::new(800, 600, &saliency);
        assert_eq!(plain.get(350, 201), 0.0, "the strand's tip is sure background to the depth");

        let found = widened(&plain, &saliency, &depth, band).expect("a strand was found");
        // The strand to its tip, and a pixel either side of it, for the matting model.
        for x in [310, 330, 359] {
            assert_eq!(found.get(x, 201), 0.5, "({x}, 201) on the strand");
        }
        assert_eq!(found.get(340, 199), 0.5);
        // Not the sky beside it, the smudge apart from the head, nor the nearer post.
        assert_eq!(found.get(340, 190), 0.0);
        assert_eq!(found.get(620, 120), 0.0);
        assert_eq!(found.get(315, 500), 0.0);
        // Nothing else changed.
        for (index, (found, plain)) in found.as_slice().iter().zip(plain.as_slice()).enumerate() {
            let (x, y) = (index % 800, index / 800);
            assert!(found == plain || (300..365).contains(&x) && (195..208).contains(&y), "({x}, {y})");
        }

        // Bilbao with no faint alpha changes nothing.
        let solid: Vec<f32> = (0..600).flat_map(|_| (0..800).map(|x| if x < 300 { 0.9 } else { 0.0 })).collect();
        assert!(widened(&plain, &Matte::new(800, 600, &solid), &depth, band).is_none());
    }

    #[test]
    fn a_small_island_of_sure_background_bilbao_calls_the_subject_is_left_unknown_and_a_hole_it_sees_is_not() {
        // A wrist at 1 m on the left of an 800 x 600 photograph before a sky at 10 m. The guided depth overshoots at a
        // bright watch face, which reads at 0.5 m, nearer than the band: sure background, an island within the wrist.
        // Bilbao calls the watch the subject; a second island of the same, a real gap, it calls background.
        let watch = |x: u32, y: u32| (150..160).contains(&x) && (300..310).contains(&y);
        let gap = |x: u32, y: u32| (150..160).contains(&x) && (100..110).contains(&y);
        let depth = refined(800, 600, |x, y| {
            if watch(x, y) || gap(x, y) {
                0.5
            } else if x < 400 {
                1.0
            } else {
                10.0
            }
        });
        let band = Band::around(1.0);
        let plain = trimap(&depth, band, None);
        assert_eq!(plain.get(155, 305), 0.0, "the watch is sure background to the depth");
        let saliency: Vec<f32> = (0..600)
            .flat_map(|y| (0..800).map(move |x| (x, y)))
            .map(|(x, y)| if x < 400 && !gap(x, y) { 0.9 } else { 0.0 })
            .collect();

        let found = widened(&plain, &Matte::new(800, 600, &saliency), &depth, band).expect("a false hole was found");
        for (x, y) in [(150, 300), (155, 305), (159, 309)] {
            assert_eq!(found.get(x, y), 0.5, "({x}, {y}) on the watch");
        }
        assert_eq!(found.get(155, 105), 0.0, "the gap stays background");
        assert_eq!(found.get(600, 300), 0.0, "the sky stays background");
    }

    #[test]
    fn a_hole_bilbao_finds_in_the_subject_is_seeded_as_background_and_nothing_outside_it() {
        let (plain, saliency) = scene(1, bilbao(1));
        let seeded = seeded(&plain, &saliency, None).expect("a hole was found");

        // The hole is sure background, in from its edge; the subject beside it unknown, for Porto to draw its edge.
        assert_eq!(seeded.get(500, 380), 0.0);
        assert_eq!(seeded.get(491, 371), 0.0);
        assert_eq!(seeded.get(489, 380), 0.5);
        assert_eq!(seeded.get(510, 380), 0.5);

        // Everywhere else the trimap is the depth's: the subject well away from the hole, its outline, the background,
        // and the table, which Bilbao does not find salient but is no hole in the subject.
        for (x, y) in [(400, 300), (600, 500), (300, 400), (296, 400), (100, 100), (500, 620), (690, 690), (500, 552)] {
            assert_eq!(seeded.get(x, y), plain.get(x, y), "({x}, {y})");
        }
        // What changed is the hole and its edge, and the subject's along the table, where the depth's subject and
        // Bilbao's part: unknown there, for Porto to say where the one ends.
        for (index, (seeded, plain)) in seeded.as_slice().iter().zip(plain.as_slice()).enumerate() {
            let (x, y) = (index % 1000, index / 1000);
            let near_hole = (480..520).contains(&x) && (360..400).contains(&y);
            let along_table = (540..560).contains(&y) && *seeded == 0.5;
            assert!(seeded == plain || near_hole || along_table, "({x}, {y}): {plain} became {seeded}");
        }
        assert!(seeded.as_slice().iter().all(|value| [0.0, 0.5, 1.0].contains(value)));
    }

    #[test]
    fn a_subject_with_no_hole_or_another_salient_subject_leaves_the_trimap_as_it_is() {
        // No hole, and Bilbao's subject the depth's, table and all: nothing to seed.
        let (plain, saliency) = scene(1, |x, y| (300..700).contains(&x) && (200..700).contains(&y));
        assert!(seeded(&plain, &saliency, None).is_none());

        // Bilbao found something else, a passer-by at the left, with a hole of its own: the depth's subject is not it,
        // and its holes are not this subject's.
        let elsewhere = |x: u32, y: u32| {
            (20..250).contains(&x) && (100..600).contains(&y) && !((120..140).contains(&x) && (300..320).contains(&y))
        };
        let (plain, saliency) = scene(1, elsewhere);
        assert!(seeded(&plain, &saliency, None).is_none());

        // Nothing sure at all.
        assert!(seeded(&Plane::new(400, 300, 0.5), &Matte::new(1, 1, &[1.0]), None).is_none());
    }

    #[test]
    fn a_salient_subject_within_a_much_larger_band_still_has_its_holes_seeded() {
        // A chair on a floor the band holds from wall to wall: the depth's sure subject is the lower two thirds of a
        // 1000 x 900 photograph, and Bilbao's subject, the chair, a 200 px square of it with a 20 px gap between two
        // slats, under a tenth of that. All of Bilbao's subject lies within the depth's, so it is a part of it.
        let (width, height) = (1000_u32, 900_u32);
        let pixels = || (0..height).flat_map(move |y| (0..width).map(move |x| (x, y)));
        let chair = |x: u32, y: u32| (400..600).contains(&x) && (400..600).contains(&y);
        let gap = |x: u32, y: u32| (400..600).contains(&x) && (490..510).contains(&y);
        let trimap: Vec<f32> = pixels().map(|(_, y)| if y >= 300 { 1.0 } else { 0.0 }).collect();
        let saliency: Vec<f32> = pixels().map(|(x, y)| if chair(x, y) && !gap(x, y) { 0.97 } else { 0.02 }).collect();
        let (plain, saliency) = (Plane::from_vec(width, height, trimap).unwrap(), Matte::new(width, height, &saliency));

        let seeded = seeded(&plain, &saliency, None).expect("the chair's gap was seeded");
        assert_eq!(seeded.get(500, 500), 0.0, "the gap between the slats is sure background");
        assert_eq!(seeded.get(200, 700), 1.0, "the floor away from the chair is the depth's");
    }

    #[test]
    fn the_holes_radii_scale_with_the_photograph_as_the_ring_does() {
        // The same scene at twice the size: the same hole, twice as large, is seeded at the same place, and the ring of
        // unknown beside it is twice as wide.
        let (small, small_saliency) = scene(1, bilbao(1));
        let (large, large_saliency) = scene(2, bilbao(2));
        let (small, large) =
            (seeded(&small, &small_saliency, None).unwrap(), seeded(&large, &large_saliency, None).unwrap());

        assert_eq!((small.get(500, 380), large.get(1000, 760)), (0.0, 0.0));
        assert_eq!((small.get(489, 380), large.get(975, 760)), (0.5, 0.5));
        assert_eq!((small.get(486, 380), large.get(975, 760)), (1.0, 0.5));
        let unknown = |plane: &Plane| plane.as_slice().iter().filter(|&&value| value == 0.5).count();
        let seeds = |plane: &Plane| plane.as_slice().iter().filter(|&&value| value == 0.0).count();
        #[allow(clippy::cast_precision_loss, reason = "pixel counts compared as ratios")]
        let ratio = |count: fn(&Plane) -> usize| count(&large) as f64 / count(&small) as f64;
        // Four times the area, give or take the quarter size's rounding.
        assert!((3.0..5.5).contains(&ratio(unknown)), "{}", ratio(unknown));
        assert!((3.0..5.5).contains(&ratio(seeds)), "{}", ratio(seeds));

        // A hole wider than the closing is no hole: an opening in the outline, left to the depth.
        let wide = |x: u32, y: u32| bilbao(1)(x, y) && !((400..600).contains(&x) && (300..450).contains(&y));
        let (plain, saliency) = scene(1, wide);
        let opened = seeded(&plain, &saliency, None).unwrap_or(plain);
        assert!((400..600).all(|x| (300..450).all(|y| opened.get(x, y) == 1.0)));
    }

    #[test]
    fn the_joined_matte_is_the_seeded_one_in_the_holes_and_never_less_than_the_plain_one_on_the_subject() {
        // Bilbao's subject on the left half; the plain matte all 1 (the depth's solid subject); the seeded one 0 down
        // the middle, where a thin part of the subject lay beside a seed.
        let (width, height) = (200, 100);
        let saliency = Matte::new(
            width,
            height,
            &(0..width * height).map(|i| if i % width < 100 { 0.9 } else { 0.0 }).collect::<Vec<_>>(),
        );
        let plain = Matte::new(width, height, &vec![1.0; (width * height) as usize]);
        let holed = Matte::new(width, height, &vec![0.2; (width * height) as usize]);

        let joined = joined(&plain, &holed, &saliency);

        // On the subject, and the seeds' inset past its edge, the plain matte; in the hole beyond, the seeded one.
        assert_eq!(joined.alpha_at(50, 50), 1.0);
        assert_eq!(joined.alpha_at(100, 50), 1.0);
        assert!((joined.alpha_at(150, 50) - 0.2).abs() < 1e-4);
        assert_eq!(joined.dimensions(), (width, height));
    }

    #[test]
    fn a_seed_the_colour_of_the_subject_around_it_is_taken_back_and_one_the_colour_of_the_background_is_not() {
        // A yellow subject on the left, a blue background on the right, both sure (a ring of 11 px); two seed squares in
        // the unknown strip between, two rings wide: one on a yellow stalk, one on a blue gap.
        let (width, height) = (1600, 400);
        let trimap = Plane::from_vec(
            width,
            height,
            (0..width * height)
                .map(|index| match index % width {
                    0..760 => 1.0,
                    760..800 => 0.5,
                    _ => 0.0,
                })
                .collect(),
        )
        .unwrap();
        let (stalk, gap) = ((770, 100), (770, 300));
        let square = |(left, top): (usize, usize), x: usize, y: usize| {
            (left..left + 20).contains(&x) && (top..top + 20).contains(&y)
        };
        let photo = RgbImage::from_fn(width, height, |x, y| {
            let (x, y) = (x as usize, y as usize);
            if x < 760 || square(stalk, x, y) {
                image::Rgb([220, 200, 40])
            } else {
                image::Rgb([40, 60, 160])
            }
        });
        let mut seeds = plane_of(width, height, |x, y| square(stalk, x, y) || square(gap, x, y));

        unseed_the_subjects_colour(&mut seeds, &trimap, &photo, unknown_radius(width, height), |_| true);

        assert_eq!(seeds.get(780, 110), 0.0, "the stalk is the subject's colour: no seed");
        assert_eq!(seeds.get(780, 310), 1.0, "the gap is the background's colour: seeded");
    }

    #[test]
    fn the_seeded_matte_never_draws_back_what_the_plain_one_left_out() {
        // A nearer strap the depth left out (the plain matte 0), Bilbao's subject on its left half; seeded, the matting
        // model read the changed neighbourhood and drew it back at 0.7 across the strip.
        let (width, height) = (200, 100);
        let saliency = Matte::new(
            width,
            height,
            &(0..width * height).map(|i| if i % width < 100 { 0.9 } else { 0.0 }).collect::<Vec<_>>(),
        );
        let plain = Matte::new(width, height, &vec![0.0; (width * height) as usize]);
        let holed = Matte::new(width, height, &vec![0.7; (width * height) as usize]);

        let joined = joined(&plain, &holed, &saliency);

        assert_eq!(joined.alpha_at(50, 50), 0.0, "on Bilbao's subject");
        assert_eq!(joined.alpha_at(150, 50), 0.0, "in the holes");
    }

    #[test]
    fn a_speck_the_seeds_took_from_the_subject_is_given_back_and_a_hole_is_not() {
        // A strip of a 4800-pixel-wide photograph (a ring of 34 px) Bilbao calls background throughout, the depth's
        // subject solid over it; seeded, the matting model opened a 40 px hole and took a 6 x 6 px speck off a leaf.
        let (width, height) = (4800, 120);
        let at = |x: u32, y: u32, (left, top, size): (u32, u32, u32)| {
            (left..left + size).contains(&x) && (top..top + size).contains(&y)
        };
        let (hole, speck) = ((1000, 40, 40), (3000, 50, 6));
        let holed: Vec<f32> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| if at(x, y, hole) || at(x, y, speck) { 0.0 } else { 1.0 })
            .collect();
        let plain = Matte::new(width, height, &vec![1.0; (width * height) as usize]);
        let saliency = Matte::new(1, 1, &[0.0]);

        let joined = joined(&plain, &Matte::new(width, height, &holed), &saliency);

        assert_eq!(joined.alpha_at(1020, 60), 0.0, "the hole is open");
        assert_eq!(joined.alpha_at(1000, 40), 0.0, "to its edge");
        assert_eq!(joined.alpha_at(3002, 52), 1.0, "the speck is the subject again");
        assert_eq!(joined.alpha_at(3005, 55), 1.0);
    }

    #[test]
    fn a_rim_the_depth_reads_nearer_by_a_pixel_is_not_background() {
        // The subject at 1 m before a wall at 10 m, with a one-pixel arc along its edge and a dot inside it that read
        // 0.8 m (the guided depth's overshoot at a bright edge), and a real nearer thing, 12 px wide, at the left.
        let depth = refined(400, 300, |x, y| match (x, y) {
            (0..12, _) | (199, _) | (150, 150) => 0.8,
            (12..200, _) => 1.0,
            _ => 10.0,
        });
        let trimap = trimap(&depth, Band::around(1.0), None);

        for y in [10, 150, 290] {
            assert_ne!(trimap.get(199, y), 0.0, "the arc on the edge at (199, {y}) is sure background");
            assert!((0..12).all(|x| trimap.get(x, y) == 0.0), "the nearer thing is not background");
        }
        assert_eq!(trimap.get(150, 150), 0.5, "the dot is not background, and not sure subject");
    }

    fn map(metres: impl Fn(u32, u32) -> f32) -> DepthMap {
        let values: Vec<f32> = (0..96).flat_map(|y| (0..128).map(move |x| (x, y))).map(|(x, y)| metres(x, y)).collect();

        DepthMap::new(128, 96, &values, Some(36.0))
    }

    #[test]
    fn a_matte_is_worth_estimating_only_for_a_blur_of_three_pixels_and_a_subject_in_focus() {
        let lens = Lens::new(36.0, 6.0, LensOrigin::Exif);
        let scene = map(|x, _| if x < 64 { 1.0 } else { 10.0 });
        let blur = |f_number: f32, diagonal: f32| Blur::new(&lens, f_number, 1.0, diagonal).expect("adds a blur");

        // f/1.4 on a 6000 x 4000 photograph blurs the wall by tens of pixels.
        assert!(worth_matting(&scene, &blur(1.4, 7211.0), Band::around(1.0)));
        // f/5.6 on a thumbnail adds under three pixels anywhere.
        assert!(!worth_matting(&scene, &blur(5.6, 200.0), Band::around(1.0)));
        // Focused on nothing: no pixel of the scene lies within the band around 3 m.
        assert!(!worth_matting(&scene, &blur(1.4, 7211.0), Band::around(3.0)));
    }

    /// The blur of f/1.4 on a 36 mm lens focused at 1 m, on a photograph 2,000 px across: a jump is 2 px of blur, which
    /// is 0.09 of disparity (1 m against 1.1 m), and the wall at 1.3 m behind blurs by 5 px.
    fn blur_at_one_metre() -> Blur {
        Blur::new(&Lens::new(36.0, 6.0, LensOrigin::Exif), 1.4, 1.0, 2000.0).expect("adds a blur")
    }

    #[test]
    fn a_wall_behind_the_subject_across_a_jump_is_cut_from_the_band_and_a_small_part_is_not() {
        // A chair at 1 m on the left half, a wall at 1.3 m on the right — within the band, which reaches 1.5 m — and a
        // tip of the chair the depth reads at the wall's distance, 6 x 6 px, too small to be anything else.
        let metres =
            |x: u32, y: u32| if x < 64 && !((20..26).contains(&x) && (40..46).contains(&y)) { 1.0 } else { 1.3 };
        let band = Band::around(1.0);
        let cut = cut_at_steps(&map(metres), &blur_at_one_metre(), band, 2000.0);

        assert_eq!((cut.width(), cut.height()), (128, 96));
        for y in [0, 48, 95] {
            assert!((70..128).all(|x| cut.get(x, y) == 1.0), "the wall is kept in row {y}");
            assert!((0..65).all(|x| cut.get(x, y) == 0.0), "the chair or its side of the jump is cut in row {y}");
        }
        assert_eq!(cut.get(22, 42), 0.0, "the small part is cut");

        // The trimap, at a photograph's size, leaves the wall out of the subject as it would the wall beyond the band.
        let depth = refined(800, 600, |x, y| metres(x * 128 / 800, y * 96 / 600));
        assert_eq!(trimap(&depth, band, None).get(700, 300), 1.0);
        assert_eq!(trimap(&depth, band, Some(&cut)).get(700, 300), 0.0);
        assert_eq!(trimap(&depth, band, Some(&cut)).get(100, 300), 1.0);
    }

    #[test]
    fn a_surface_turning_away_from_the_focus_is_kept_however_far_it_reaches() {
        // Shoulders in focus at the bottom, the head above them reaching 1.4 m at the top: 6 px of blur farther, but
        // smoothly, a tenth of a pixel a row.
        let scene = map(|_, y| 1.0 + 0.4 * (95 - y) as f32 / 95.0);
        let cut = cut_at_steps(&scene, &blur_at_one_metre(), Band::around(1.0), 2000.0);

        assert!(cut.as_slice().iter().all(|&value| value == 0.0));
    }

    #[test]
    fn the_small_steps_between_the_leaves_of_a_canopy_are_not_jumps() {
        // Leaves 8 px across at 1.03 m and 1.08 m in turn, a pixel of blur apart, before a sky beyond the band.
        let scene = map(|x, y| match (x, y) {
            (16..112, 16..80) if (x / 8 + y / 8) % 2 == 0 => 1.03,
            (16..112, 16..80) => 1.08,
            _ => 50.0,
        });
        let cut = cut_at_steps(&scene, &blur_at_one_metre(), Band::around(1.0), 2000.0);

        assert!(cut.as_slice().iter().all(|&value| value == 0.0));
    }
}
