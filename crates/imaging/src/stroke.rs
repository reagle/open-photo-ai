//! A soft-edged brush stroke painted into, or erased from, a [`Plane`] of weights.
//!
//! One caller: the mask a run's enhancements are limited to (`opai::mask`), which turns a stroke's fractions of the
//! photograph into the pixels and radii this takes, and applies its strokes in order over a base.

// Pixels only. What a stroke's radius is a fraction of, what softness means to a user, the order strokes apply in and
// whether the finished weight is turned over are the mask's; this answers one question — how much of this stroke is at
// this pixel — and writes the answer.
//
// A paint is a max rather than a sum or an alpha composite: a brush that passes over the same place twice must not
// darken a soft edge it already painted. An erase subtracts, and is not the mirror image `min(w, 1 - s)`: that leaves
// `min(s, 1 - s)` where a soft paint is erased along its own path, a faint ring the user cannot see to remove, while
// `w - s` takes back exactly what the paint put there. It never leaves more than the mirror would, since `w - s` is
// below both `w` and `1 - s`; the price is that erasing the same place twice lowers a soft edge again.
//
// `s` is the stroke's, not a segment's: a pixel near a joint is within reach of two segments, and subtracting each
// would take the edge twice — a soft erase over a path of several points would become a wide hard one. So the stroke's
// coverage is settled first, the greatest weight any of its segments gives the pixel (the least distance to the path),
// and applied once, which is also all a paint needs.

// No Gaussian, unlike face recovery's feathered plane: the edge is a function of the distance to the stroke, which is
// already smooth, so the profile is written down rather than blurred into being — and it costs the painted area rather
// than a convolution of the whole frame.

use crate::plane::Plane;
use crate::rows::for_each_row;

/// Paints one stroke into `plane`, or erases it from it.
///
/// `points` is the stroke's path as `x0, y0, x1, y1, …` in pixels, where pixel `(x, y)` covers `[x, x + 1)` and its
/// centre is at `(x + 0.5, y + 0.5)`. One point is a dot; an odd trailing coordinate is ignored, and no point at all
/// paints nothing.
///
/// A pixel whose centre is within `core_px` of the path is the stroke's whole weight, 1; one at `radius_px` or further
/// is none of it, 0; between, the weight falls along a smoothstep. The radius is held at half a pixel at least and the
/// core at a pixel inside the radius at most, so every edge is anti-aliased over a pixel even at a softness of zero.
///
/// A paint raises each pixel to the stroke's weight there, `max(w, s)`; an erase lowers it by that weight, `max(w - s,
/// 0)`, which is at most one minus the stroke's weight. The weight is that of the nearest point of the path, so a
/// pixel near two segments is covered once. Only the pixels within the radius of the path are visited, so a stroke
/// costs the area its bounding box covers whatever the plane's size, and a stroke wholly off the plane touches nothing.
///
/// A coordinate that is not finite drops the segment it belongs to, a radius that is not finite is the half pixel it is
/// held to at least, and a core that is not finite is none.
pub fn painted(plane: &mut Plane, points: &[f32], radius_px: f32, core_px: f32, erase: bool) {
    let radius = if radius_px.is_finite() { radius_px.max(0.5) } else { 0.5 };
    let core = if core_px.is_finite() { core_px.clamp(0.0, (radius - 1.0).max(0.0)) } else { 0.0 };
    let (width, height) = (plane.width() as usize, plane.height() as usize);

    if width == 0 || height == 0 {
        return;
    }

    // A dot is the segment from a point to itself, so one path through the arithmetic serves both.
    let segments: Vec<([f32; 2], [f32; 2])> = match points.as_chunks::<2>().0 {
        [] => return,
        [only] => vec![(*only, *only)],
        path => path.windows(2).map(|pair| (pair[0], pair[1])).collect(),
    };

    let reaches: Vec<_> = segments
        .into_iter()
        .filter(|(from, to)| from.iter().chain(to).all(|coordinate| coordinate.is_finite()))
        .filter_map(|(from, to)| {
            reach(from, to, radius, width, height).map(|(columns, rows)| (from, to, columns, rows))
        })
        .collect();

    let Some((_, _, first_columns, first_rows)) = reaches.first() else { return };
    let (mut columns, mut rows) = (first_columns.clone(), first_rows.clone());

    for (_, _, segment_columns, segment_rows) in &reaches {
        columns = columns.start.min(segment_columns.start)..columns.end.max(segment_columns.end);
        rows = rows.start.min(segment_rows.start)..rows.end.max(segment_rows.end);
    }

    // The stroke's coverage over the box that holds it, a plane of its own so the base is read and written once.
    let across = columns.len();
    let mut coverage = vec![0.0_f32; across * rows.len()];

    for (from, to, segment_columns, segment_rows) in reaches {
        let band = &mut coverage[(segment_rows.start - rows.start) * across..(segment_rows.end - rows.start) * across];

        for_each_row(band, across, segment_rows.len(), |offset, row| {
            let centre_y = (segment_rows.start + offset) as f32 + 0.5;

            for x in segment_columns.clone() {
                let stroke = weight(to_segment([x as f32 + 0.5, centre_y], from, to), core, radius);
                let covered = &mut row[x - columns.start];

                *covered = covered.max(stroke);
            }
        });
    }

    let band = &mut plane.as_mut_slice()[rows.start * width..rows.end * width];

    for_each_row(band, width, rows.len(), |offset, row| {
        for (value, stroke) in row[columns.clone()].iter_mut().zip(&coverage[offset * across..][..across]) {
            *value = if erase { (*value - stroke).max(0.0) } else { value.max(*stroke) };
        }
    });
}

/// The columns and rows whose centres can lie within `radius` of the segment from `from` to `to`, clipped to a
/// `width` x `height` plane, or `None` where none of them is on it.
fn reach(
    from: [f32; 2],
    to: [f32; 2],
    radius: f32,
    width: usize,
    height: usize,
) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    // A centre at `i + 0.5` within `radius` of `[lo, hi]` is an index in `[lo - radius - 0.5, hi + radius - 0.5]`. In
    // `f64` and clamped before the cast, so a point far off the plane (or a NaN a fault let through) is an empty range
    // rather than a wrapped one.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "both ends are clamped into [0, len] before the cast"
    )]
    fn span(a: f32, b: f32, radius: f32, len: usize) -> std::ops::Range<usize> {
        let (lo, hi) = (f64::from(a.min(b)) - f64::from(radius) - 0.5, f64::from(a.max(b)) + f64::from(radius) - 0.5);

        (lo.floor().clamp(0.0, len as f64) as usize)..((hi.ceil() + 1.0).clamp(0.0, len as f64) as usize)
    }

    let (columns, rows) = (span(from[0], to[0], radius, width), span(from[1], to[1], radius, height));

    (!columns.is_empty() && !rows.is_empty()).then_some((columns, rows))
}

/// The distance from `point` to the segment from `from` to `to`.
fn to_segment(point: [f32; 2], from: [f32; 2], to: [f32; 2]) -> f32 {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let length = dx.mul_add(dx, dy * dy);

    // The nearest point of the segment, as a fraction of the way along it; a dot has no length and is its own nearest.
    let along = if length > 0.0 {
        ((point[0] - from[0]).mul_add(dx, (point[1] - from[1]) * dy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let (nx, ny) = (along.mul_add(dx, from[0]) - point[0], along.mul_add(dy, from[1]) - point[1]);

    nx.hypot(ny)
}

/// The stroke's own weight at `distance` from its path: 1 within `core`, 0 from `radius` on, and a smoothstep between.
fn weight(distance: f32, core: f32, radius: f32) -> f32 {
    if distance <= core {
        return 1.0;
    }

    if distance >= radius {
        return 0.0;
    }

    let t = (distance - core) / (radius - core);

    1.0 - t * t * 2.0f32.mul_add(-t, 3.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dot of `radius` and `core` at the centre of a `side` square, painted over nothing.
    fn dot(side: u32, radius: f32, core: f32) -> Plane {
        let mut plane = Plane::new(side, side, 0.0);
        let centre = side as f32 / 2.0;

        painted(&mut plane, &[centre, centre], radius, core, false);

        plane
    }

    #[test]
    fn a_dots_profile_is_whole_in_its_core_falls_smoothly_and_is_gone_past_its_radius() {
        // The centre of pixel (20, 20) is 0.5 from the dot's on each axis, so its distance is about 0.71.
        let plane = dot(40, 10.0, 4.0);

        for x in 20..24 {
            assert_eq!(plane.get(x, 20), 1.0, "({x}, 20) is inside the core and was not painted whole");
        }

        // Past the core the weight only falls, and it falls to nothing at the radius.
        let profile: Vec<f32> = (23..31).map(|x| plane.get(x, 20)).collect();
        assert!(profile.windows(2).all(|pair| pair[1] <= pair[0]), "the edge is not monotonic: {profile:?}");
        assert!(profile[2] > 0.0 && profile[2] < 1.0, "the soft edge has no partial weight: {profile:?}");
        assert_eq!(plane.get(30, 20), 0.0, "a centre 10.5 away is outside a radius of 10");
        assert_eq!(plane.get(0, 0), 0.0);

        // A circle, not a square: the corner of the dot's box is further than its side.
        assert!(plane.get(26, 26) < plane.get(28, 20), "the dot is not round");
    }

    #[test]
    fn a_hard_dot_is_still_anti_aliased_over_a_pixel() {
        // A core asked for at the radius itself is held a pixel inside it, so the edge is never a step.
        let plane = dot(40, 6.0, 6.0);
        let edge: Vec<f32> = (20..28).map(|x| plane.get(x, 20)).filter(|w| *w > 0.0 && *w < 1.0).collect();

        assert!(!edge.is_empty(), "a hard dot has no anti-aliased pixel");
    }

    #[test]
    fn a_straight_strokes_width_is_twice_its_radius() {
        let mut plane = Plane::new(100, 60, 0.0);

        painted(&mut plane, &[10.0, 30.0, 90.0, 30.0], 8.0, 7.0, false);

        // Across the middle of the stroke, the column at x = 50: whole from 23 to 36 (centres within 7), nothing from
        // 38 down and from 22 up past the radius of 8.
        let column: Vec<f32> = (0..60).map(|y| plane.get(50, y)).collect();
        let whole = column.iter().filter(|w| **w == 1.0).count();
        let touched = column.iter().filter(|w| **w > 0.0).count();

        assert_eq!(whole, 14, "the core is not 2 x 7 pixels wide: {column:?}");
        assert_eq!(touched, 16, "the stroke is not 2 x 8 pixels wide: {column:?}");

        // The ends are round caps: nothing beyond the radius past the last point.
        assert_eq!(plane.get(98, 30), 0.0);
        assert_eq!(plane.get(92, 30), 1.0);
    }

    #[test]
    fn erasing_the_same_stroke_takes_back_exactly_what_painting_put_there() {
        let path = [5.0, 5.0, 40.0, 25.0, 60.0, 10.0];
        let mut plane = Plane::new(70, 40, 0.0);

        painted(&mut plane, &path, 9.0, 3.0, false);
        assert!(plane.as_slice().iter().any(|w| *w > 0.0));

        painted(&mut plane, &path, 9.0, 3.0, true);
        assert!(plane.as_slice().iter().all(|w| *w == 0.0), "the erase left some of the paint behind");
    }

    #[test]
    fn painting_the_same_stroke_twice_changes_nothing() {
        let path = [5.0, 5.0, 40.0, 25.0, 60.0, 10.0];
        let mut once = Plane::new(70, 40, 0.0);
        painted(&mut once, &path, 9.0, 3.0, false);

        let mut twice = once.clone();
        painted(&mut twice, &path, 9.0, 3.0, false);

        assert_eq!(twice, once, "a second pass darkened the soft edge");
    }

    #[test]
    fn a_stroke_off_the_frame_touches_nothing() {
        let mut plane = Plane::new(30, 20, 0.25);

        painted(&mut plane, &[-50.0, -50.0, -20.0, -10.0], 5.0, 2.0, false);
        painted(&mut plane, &[200.0, 5.0], 5.0, 2.0, true);
        painted(&mut plane, &[f32::NAN, 5.0], 5.0, 2.0, false);

        assert!(plane.as_slice().iter().all(|w| *w == 0.25), "a stroke off the plane changed it");
    }

    #[test]
    fn no_points_paint_nothing_and_an_odd_coordinate_is_ignored() {
        let mut plane = Plane::new(30, 20, 0.0);

        painted(&mut plane, &[], 5.0, 2.0, false);
        painted(&mut plane, &[15.0], 5.0, 2.0, false);
        assert!(plane.as_slice().iter().all(|w| *w == 0.0));

        painted(&mut plane, &[15.0, 10.0, 7.0], 5.0, 2.0, false);
        assert_eq!(plane.get(15, 10), 1.0, "the whole pair before the odd coordinate was not painted");
    }

    /// A stroke of several points with a soft edge, whose joints are where a per-segment erase would subtract twice.
    const WINDING: [f32; 8] = [10.0, 20.0, 30.0, 20.0, 50.0, 30.0, 70.0, 10.0];

    #[test]
    fn a_soft_erase_of_a_whole_pixel_leaves_what_the_paint_over_nothing_does_not() {
        let (mut painted_over_nothing, mut erased_from_whole) = (Plane::new(80, 40, 0.0), Plane::new(80, 40, 1.0));

        painted(&mut painted_over_nothing, &WINDING, 8.0, 1.0, false);
        painted(&mut erased_from_whole, &WINDING, 8.0, 1.0, true);

        for (erased, paint) in erased_from_whole.as_slice().iter().zip(painted_over_nothing.as_slice()) {
            assert!((erased - (1.0 - paint)).abs() < 1e-6, "an erase left {erased} where the paint is {paint}");
        }
    }

    #[test]
    fn a_joint_is_not_erased_twice() {
        // The column at the second point is in reach of both segments that meet there. Half way down the soft edge
        // below it a paint is about a half, so a double subtraction would take the whole pixel and leave nothing.
        let (mut paint, mut erase) = (Plane::new(80, 40, 0.0), Plane::new(80, 40, 1.0));

        painted(&mut paint, &WINDING, 8.0, 1.0, false);
        painted(&mut erase, &WINDING, 8.0, 1.0, true);

        let halves: Vec<u32> = (21..40).filter(|y| (0.3..0.7).contains(&paint.get(30, *y))).collect();

        assert!(!halves.is_empty(), "the edge below the joint has no half weight to test");

        for y in halves {
            assert!(
                (erase.get(30, y) - (1.0 - paint.get(30, y))).abs() < 1e-6,
                "(30, {y}) was erased to {} where the paint is {}",
                erase.get(30, y),
                paint.get(30, y)
            );
        }
    }

    #[test]
    fn a_coordinate_that_is_not_a_number_drops_its_segment_and_a_radius_or_core_that_is_not_one_is_a_default() {
        // The middle point is NaN in one coordinate, so both segments that touch it go, rather than one smeared across
        // the plane.
        let mut plane = Plane::new(40, 20, 0.0);
        painted(&mut plane, &[5.0, 10.0, f32::NAN, 10.0, 35.0, 10.0], 3.0, 1.0, false);
        assert!(plane.as_slice().iter().all(|w| *w == 0.0), "a segment with a NaN coordinate was painted");

        // The segments either side of it still paint: a NaN in a later point does not spoil an earlier segment.
        painted(&mut plane, &[5.0, 10.0, 15.0, 10.0, f32::NAN, 10.0], 3.0, 1.0, false);
        assert_eq!(plane.get(10, 10), 1.0);
        assert_eq!(plane.get(30, 10), 0.0);

        // A radius or a core that is not a number is the half pixel and nothing, never a plane of NaN.
        for (radius, core) in [(f32::NAN, 1.0), (4.0, f32::NAN), (f32::INFINITY, f32::NAN), (4.0, f32::INFINITY)] {
            let mut plane = Plane::new(20, 20, 0.0);

            painted(&mut plane, &[10.5, 10.5], radius, core, false);

            assert!(plane.as_slice().iter().all(|w| w.is_finite()), "({radius}, {core}) left a NaN in the plane");
            assert!(plane.get(10, 10) > 0.0, "({radius}, {core}) painted nothing at the stroke's own pixel");
        }
    }
}
