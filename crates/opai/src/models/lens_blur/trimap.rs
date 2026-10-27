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

use imaging::morph::{dilated, eroded};
use imaging::plane::Plane;
use imaging::rows::for_each_row;

use super::lens::Blur;
use super::render::Refined;
use crate::models::DepthMap;

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

/// The trimap of the subject `band` holds, at the size of `refined`, the depth guided to the photograph: 1 where it is
/// surely the subject, 0 where it is surely not, 0.5 between.
pub(crate) fn trimap(refined: &Refined, band: Band) -> Plane {
    let (width, height) = (refined.plane.width(), refined.plane.height());
    let (columns, rows) = (width as usize, height as usize);
    let values = refined.plane.as_slice();
    let disparity = |index: usize| values[index] * refined.nearest;
    let held = |index: usize| band.holds(disparity(index));

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

    let mut trimap = Plane::new(width, height, 0.0);
    for_each_row(trimap.as_mut_slice(), columns, rows, |y, out| {
        let small_y = (y / REDUCTION as usize) as u32;

        for (x, value) in out.iter_mut().enumerate() {
            let small_x = (x / REDUCTION as usize) as u32;
            let index = y * columns + x;
            let inside = held(index);

            *value = if band.in_front(disparity(index)) {
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
        let trimap = trimap(&depth, Band::around(1.0));
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
        let trimap = trimap(&depth, Band::around(1.0));

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
}
