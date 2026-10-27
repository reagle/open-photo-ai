//! A low-resolution plane brought to a photograph's size with its edges on the photograph's own, and a dilation that
//! lets the larger of two values on an edge win.
//!
//! The first caller is the lens blur (`models::lens_blur`): a depth model answers at about 1280 pixels across, softly
//! at every outline, and a blur radius that changes a few pixels off the edge of a subject either smears the subject
//! into the background or leaves a sharp fringe of background around it. [`upsampled`] puts the depth's edges on the
//! photograph's (hair, leaves), and [`dilated_nearer`] pulls the pixels that straddle an edge onto its nearer side.

// The fast guided filter (He and Sun, 2015) over the colour guided filter (He, Sun and Tang, 2013): the per-pixel
// linear model `q = a . I + b` is solved at a quarter of the photograph's resolution, where every window is sixteen
// times cheaper, and only applied at full resolution. The coefficients vary slowly by construction (they are window
// means), which is what makes solving them coarsely nearly free in quality; the photograph's own edges come back in at
// the last step through `I`, which is read at full resolution.
//
// Box means by running sums ([`boxes`](crate::boxes)), so the cost per pixel does not grow with the window. The window
// is clipped at the edges of the plane and divided by the number of pixels it actually holds, the original paper's
// convention, rather than reflected: a reflected window counts the pixels next to an edge twice.

use crate::bilinear::Axis;
use crate::boxes::{box_mean, reduced};
use crate::plane::Plane;
use crate::rows::for_each_row;

/// How much smaller than the photograph the coefficients are solved at, on each axis.
const SUBSAMPLE: u32 = 4;

/// `low` brought to `width` x `height` along the edges of `guide`, by the fast colour guided filter.
///
/// `guide` is the photograph at `width` x `height`: its sRGB values in `[0, 1]`, three interleaved a pixel. `radius` is
/// the filter's window radius in the photograph's pixels; `eps` is its regularisation, the variance below which an
/// edge in the guide is treated as flat (smaller follows fainter edges). The result never leaves `low`'s own range.
///
/// # Panics
///
/// Panics where `guide` is not three values for every pixel of `width` x `height`, or where `low` has no area and the
/// photograph has some.
pub fn upsampled(low: &Plane, guide: &[f32], width: u32, height: u32, radius: u32, eps: f32) -> Plane {
    let (columns, rows) = (width as usize, height as usize);
    assert_eq!(guide.len(), 3 * columns * rows, "the guide is not three values for each of {width}x{height}");

    if columns * rows == 0 {
        return Plane::new(width, height, 0.0);
    }

    let (small_width, small_height) = (width.div_ceil(SUBSAMPLE), height.div_ceil(SUBSAMPLE));
    let (small_columns, small_rows) = (small_width as usize, small_height as usize);
    let window = ((radius as f32 / SUBSAMPLE as f32).round() as usize).max(1);

    let small_guide = reduced::<3>(guide, columns, rows, SUBSAMPLE as usize);
    let small = shrunk(low, small_width, small_height);

    // Thirteen planes, interleaved so that one pair of box passes means all of them: the guide's three channels, the
    // plane, the guide times the plane, and the six distinct products of the guide's channels with each other.
    let mut products = vec![0.0_f32; PRODUCTS * small_columns * small_rows];
    let pixels = products.as_chunks_mut::<PRODUCTS>().0.iter_mut().zip(small_guide.as_chunks::<3>().0);
    for ((out, &[r, g, b]), p) in pixels.zip(small.as_slice()) {
        *out = [r, g, b, *p, r * p, g * p, b * p, r * r, r * g, r * b, g * g, g * b, b * b];
    }

    let means = box_mean(&products, PRODUCTS, small_columns, small_rows, window);

    let mut coefficients = vec![0.0_f32; 4 * small_columns * small_rows];
    for (out, mean) in coefficients.as_chunks_mut::<4>().0.iter_mut().zip(means.as_chunks::<PRODUCTS>().0) {
        *out = solved(mean, eps);
    }

    // The second pass of the filter: each pixel lies in many windows, and its coefficients are the mean of theirs.
    let coefficients = box_mean(&coefficients, 4, small_columns, small_rows, window);

    // `Axis` over `SUBSAMPLE` times the small extent and only its first `width` entries read, rather than an axis from
    // the small extent onto the photograph's: a photograph whose side is not a multiple of four would otherwise be
    // read at a scale that is not the reduction's, sliding the coefficients up to three pixels off the edges they were
    // solved against by the far side of the picture.
    let across = Axis::new(small_width, small_width * SUBSAMPLE);
    let down = Axis::new(small_height, small_height * SUBSAMPLE);
    let (lo, hi) = low.range();

    let mut data = vec![0.0_f32; columns * rows];

    for_each_row(&mut data, columns, rows, |y, out| {
        let row = down.tap(y);
        let (top, bottom) = (&coefficients[4 * small_columns * row.lo..], &coefficients[4 * small_columns * row.hi..]);

        let pixels = guide[3 * columns * y..][..3 * columns].as_chunks::<3>().0;

        for (x, (value, rgb)) in out.iter_mut().zip(pixels).enumerate() {
            let column = across.tap(x);
            let [a_r, a_g, a_b, b] = std::array::from_fn(|channel| {
                let upper =
                    top[4 * column.lo + channel] * (1.0 - column.frac) + top[4 * column.hi + channel] * column.frac;
                let lower = bottom[4 * column.lo + channel] * (1.0 - column.frac)
                    + bottom[4 * column.hi + channel] * column.frac;

                upper * (1.0 - row.frac) + lower * row.frac
            });

            *value = (a_r * rgb[0] + a_g * rgb[1] + a_b * rgb[2] + b).clamp(lo, hi);
        }
    });

    Plane::from_vec(width, height, data).expect("one value for every pixel of the photograph")
}

/// How many planes the filter's box means are taken over at once.
const PRODUCTS: usize = 13;

/// One window's linear model `[a_r, a_g, a_b, b]` from its means, laid out as [`upsampled`] interleaves them.
fn solved(mean: &[f32; PRODUCTS], eps: f32) -> [f32; 4] {
    // In `f64`: the covariance is a difference of two nearly equal means wherever the guide is flat, which is most of
    // a photograph, and the inverse divides by its determinant.
    let [r, g, b, p, rp, gp, bp, rr, rg, rb, gg, gb, bb] =
        std::array::from_fn::<f64, PRODUCTS, _>(|i| f64::from(mean[i]));
    let eps = f64::from(eps);

    let (srr, srg, srb) = (rr - r * r + eps, rg - r * g, rb - r * b);
    let (sgg, sgb, sbb) = (gg - g * g + eps, gb - g * b, bb - b * b + eps);
    let (cr, cg, cb) = (rp - r * p, gp - g * p, bp - b * p);

    // The symmetric 3x3 inverse by its cofactors, which is exact enough here and has no branch: `eps` on the diagonal
    // keeps the determinant at least `eps` cubed.
    let (i_rr, i_rg, i_rb) = (sgg * sbb - sgb * sgb, srb * sgb - srg * sbb, srg * sgb - srb * sgg);
    let (i_gg, i_gb, i_bb) = (srr * sbb - srb * srb, srg * srb - srr * sgb, srr * sgg - srg * srg);
    let determinant = srr * i_rr + srg * i_rg + srb * i_rb;

    let a_r = (i_rr * cr + i_rg * cg + i_rb * cb) / determinant;
    let a_g = (i_rg * cr + i_gg * cg + i_gb * cb) / determinant;
    let a_b = (i_rb * cr + i_gb * cg + i_bb * cb) / determinant;

    [a_r, a_g, a_b, p - a_r * r - a_g * g - a_b * b].map(|value| value as f32)
}

/// `low` at `width` x `height`: box-reduced first where it is at least twice that on both axes, and then resized.
///
/// A bilinear resample reads two taps, so a plane much larger than its target (a depth map of 1280 across, for a
/// photograph under 2000) would skip every pixel between them and alias. The box reduction takes the whole blocks the
/// target's pixels stand for; the bilinear step then only closes the remaining factor of under two. The reduced grid
/// is read as if it were the target's scale, which leaves its far edge up to a pixel of `low`'s off: the depth map is
/// soft at that scale by construction.
fn shrunk(low: &Plane, width: u32, height: u32) -> Plane {
    let factor = (low.width() / width.max(1)).min(low.height() / height.max(1)) as usize;

    if factor < 2 || width == 0 || height == 0 {
        return low.resized(width, height);
    }

    let (columns, rows) = (low.width() as usize, low.height() as usize);
    let data = reduced::<1>(low.as_slice(), columns, rows, factor);

    Plane::from_vec(columns.div_ceil(factor) as u32, rows.div_ceil(factor) as u32, data)
        .expect("one value for every pixel of the reduction")
        .resized(width, height)
}

/// `plane` grey-dilated by a disc of `radius` pixels: each pixel takes the largest value within the disc around it.
///
/// For the edge grow, where the larger value is the nearer one: a pixel that mixes a subject and the background
/// behind it is pulled onto the subject's side, so its colour stays with the sharp subject rather than spreading
/// through the blurred background. The disc holds the offsets within `radius + 0.5` of the centre; under half a pixel
/// it is the pixel itself and the plane comes back unchanged.
pub fn dilated_nearer(plane: &Plane, radius: f32) -> Plane {
    let (width, height) = (plane.width() as usize, plane.height() as usize);

    if radius < 0.5 || width * height == 0 {
        return plane.clone();
    }

    // Direct over the footprint, one run of the row per row offset: the edge grow's radius is a few pixels (three on a
    // 24-megapixel photograph), where the footprint is a few dozen comparisons and a sliding maximum would cost more
    // in bookkeeping than it saved.
    let reach = radius + 0.5;
    let rows = reach.floor() as usize;
    let halves: Vec<usize> = (0..=rows).map(|dy| (reach * reach - (dy * dy) as f32).max(0.0).sqrt() as usize).collect();

    let source = plane.as_slice();
    let mut data = vec![0.0_f32; width * height];

    for_each_row(&mut data, width, height, |y, out| {
        out.fill(f32::NEG_INFINITY);

        for (dy, half) in halves.iter().enumerate() {
            for source_y in [y.checked_sub(dy), (dy > 0).then_some(y + dy).filter(|below| *below < height)] {
                let Some(source_y) = source_y else { continue };
                let row = &source[width * source_y..][..width];

                for (x, value) in out.iter_mut().enumerate() {
                    let window = &row[x.saturating_sub(*half)..=(x + half).min(width - 1)];
                    *value = window.iter().fold(*value, |largest, sample| largest.max(*sample));
                }
            }
        }
    });

    Plane::from_vec(plane.width(), plane.height(), data).expect("one value for every pixel of the plane")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A guide that is `level` grey everywhere.
    fn flat_guide(width: u32, height: u32, level: f32) -> Vec<f32> {
        vec![level; 3 * (width as usize) * (height as usize)]
    }

    #[test]
    fn a_constant_plane_stays_constant() {
        let guide: Vec<f32> = (0..3 * 64 * 48).map(|index| ((index * 7919) % 1000) as f32 / 1000.0).collect();
        let out = upsampled(&Plane::new(16, 12, 0.375), &guide, 64, 48, 24, 1e-3);

        assert!(out.as_slice().iter().all(|value| *value == 0.375), "a constant plane came back changed");
    }

    #[test]
    fn a_flat_guide_gives_the_bilinear_upsample_away_from_the_edges() {
        // With nothing in the guide to follow, the linear model is `a = 0` and `b` the plane's window mean, and a
        // window mean of a ramp is the ramp: the result is the plain upsample. Within two windows of an edge the
        // clipped window's mean is pulled inwards, which is the convention and not what this pins.
        let (width, height) = (160_u32, 96_u32);
        let low =
            Plane::from_vec(40, 24, (0..40 * 24).map(|index| (index % 40) as f32 / 40.0 + 0.1).collect()).unwrap();

        let out = upsampled(&low, &flat_guide(width, height, 0.6), width, height, 16, 1e-3);
        let bilinear = low.resized(width, height);
        let margin = 2 * (2 * 4 + 1) * SUBSAMPLE;

        for y in 0..height {
            for x in margin..width - margin {
                let (got, want) = (out.get(x, y), bilinear.get(x, y));
                assert!((got - want).abs() < 1e-3, "({x}, {y}) is {got} rather than the bilinear {want}");
            }
        }
    }

    #[test]
    fn a_step_lands_on_the_guides_edge() {
        // The guide has a hard edge at column 130 — not a multiple of the subsampling, so a result that snapped to the
        // reduced grid would land off it — and the plane, four times smaller, a step blurred over several of its own
        // pixels around the same place. The upsampled step's midpoint must be the guide's edge, give or take 2 px.
        let (width, height) = (256_u32, 64_u32);
        let edge = 130.0_f32;

        let guide: Vec<f32> = (0..width * height)
            .flat_map(|index| {
                let level = if ((index % width) as f32) < edge { 0.15 } else { 0.85 };
                [level, level * 0.9, level * 0.8]
            })
            .collect();

        let low = Plane::from_vec(
            64,
            16,
            (0..64 * 16)
                .map(|index| {
                    let x = (index % 64) as f32 + 0.5;
                    (((x - edge / 4.0) / 3.0).clamp(-1.0, 1.0) + 1.0) / 2.0
                })
                .collect(),
        )
        .unwrap();

        let out = upsampled(&low, &guide, width, height, 24, 1e-3);

        for y in 0..height {
            let crossing = (1..width).find(|x| out.get(*x, y) >= 0.5).expect("the step never reached its midpoint");
            assert!((crossing as f32 - edge).abs() <= 2.0, "row {y} steps at {crossing} rather than near {edge}");

            // And it is a step, not a ramp: two pixels either side of the edge are near the two ends of the range.
            assert!(out.get(edge as u32 - 3, y) < 0.15, "row {y} has not fallen away left of the edge");
            assert!(out.get(edge as u32 + 2, y) > 0.85, "row {y} has not risen right of the edge");
        }
    }

    #[test]
    fn a_plane_much_larger_than_its_target_is_averaged_rather_than_sampled() {
        // Every fourth column is 1, so the plane's mean is a quarter; a two-tap bilinear read at a quarter of its size
        // lands between a 1 and a 0 and answers a half everywhere. Guided by a flat colour the result is the plane's
        // window mean, which has to be the quarter.
        let (width, height) = (64_u32, 64_u32);
        let low = Plane::from_vec(64, 64, (0..64 * 64).map(|index| f32::from(index % 4 == 1)).collect()).unwrap();

        let out = upsampled(&low, &flat_guide(width, height, 0.5), width, height, 8, 1e-3);

        assert!(
            out.as_slice().iter().all(|value| (value - 0.25).abs() < 1e-3),
            "the plane was sampled, not averaged"
        );
    }

    #[test]
    fn a_dilation_grows_the_larger_value_by_the_radius_and_never_shrinks_anything() {
        let (width, height) = (41_u32, 41_u32);
        let mut values = vec![0.2_f32; 41 * 41];
        values[20 * 41 + 20] = 0.9;
        let plane = Plane::from_vec(width, height, values).unwrap();

        let grown = dilated_nearer(&plane, 3.0);

        for y in 0..height {
            for x in 0..width {
                let distance = ((x as f32 - 20.0).powi(2) + (y as f32 - 20.0).powi(2)).sqrt();
                let want = if distance <= 3.5 { 0.9 } else { 0.2 };

                assert_eq!(grown.get(x, y), want, "({x}, {y}) at {distance:.2} px from the peak");
                assert!(grown.get(x, y) >= plane.get(x, y), "({x}, {y}) shrank");
            }
        }

        // And on an edge running the full height, the larger side moves out by the radius and no further.
        let edge = Plane::from_vec(20, 9, (0..20 * 9).map(|index| if index % 20 < 10 { 1.0 } else { 0.0 }).collect());
        let grown = dilated_nearer(&edge.unwrap(), 3.0);

        for y in 0..9 {
            assert_eq!(grown.get(12, y), 1.0, "row {y} did not grow by three pixels");
            assert_eq!(grown.get(13, y), 0.0, "row {y} grew by more than three pixels");
        }
    }

    #[test]
    fn a_dilation_under_half_a_pixel_changes_nothing() {
        let plane = Plane::from_vec(5, 4, (0..20).map(|index| index as f32).collect()).unwrap();

        assert_eq!(dilated_nearer(&plane, 0.4), plane);
    }
}
