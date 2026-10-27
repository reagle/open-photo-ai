//! The photograph moved towards a finished result by the control a run carries, or by a weight per pixel.

// Cross-family tier. **The bias never reaches the graph**: it is applied after everything the model contributed,
// which is what lets one session serve every position of a control — dragging a slider re-runs this loop and opens
// nothing.
//
// Deliberately not part of `blend`. That module combines **overlapping model outputs** into one image, and the whole
// of its argument is about seams. What is here has no ramp, no overlap and no seam — it is one photograph and one
// finished result at the same extent, mixed per pixel by a scalar. Filing the two together on the strength of the
// word would make that module mean *things with "blend" in the name*.
//
// Nor does it sit beside the fixed-canvas geometry in `present`. Denoise and sharpen call it too, through `filter`:
// both carry a strength and end exactly this way, and both run on a **tile grid** — they have no square at all. Parked
// under the canvas it would have been promoted a second time by the first family that wanted it without one.
//
// `masked` is the same line with the scalar replaced by a plane: a mask is a strength that differs from pixel to pixel.
// Its one caller is the masked run (`opai`'s `process`), which blends the chain's finished result with the photograph
// it started from, after every model; the plane knows nothing of brushes or files, only a weight at each pixel.

use image::{ImageBuffer, Rgb};

use crate::bilinear::Axis;
use crate::plane::Plane;
use crate::rows::for_each_row;
use crate::tensor::{Channel, Sampler, flattened};

/// The photograph moved towards — or away from — `corrected` by `bias`, in place.
///
/// `source + bias * (corrected - source)`, per channel in unit space and bounded. At **0** the photograph comes back
/// byte for byte. At **1** the whole correction lands. At a **negative** value the photograph moves along the same
/// line in the opposite direction, which is an extrapolation past itself rather than a second model or a run in
/// reverse.
pub fn blended<T: Channel>(
    source: &Sampler<'_>,
    mut corrected: ImageBuffer<Rgb<T>, Vec<T>>,
    bias: f32,
) -> ImageBuffer<Rgb<T>, Vec<T>>
where
    Rgb<T>: image::Pixel<Subpixel = T>,
{
    // The reference's `BlendWithIntensity`, with the alpha it carries dropped for the reason every pipeline in this
    // crate drops it. The three positions fall out of the arithmetic rather than being special-cased: at 0 the term
    // vanishes, and `to_unit` and `from_unit` are exactly inverse at both depths.
    //
    // Taken by value and handed back rather than written into a third buffer: nothing else holds the correction, and
    // on a 24-megapixel photograph a third full-resolution buffer is 144 MB at `u8` to hold a result that is about to
    // replace it anyway.
    for (x, y, pixel) in corrected.enumerate_pixels_mut() {
        let full = source.rgb(x, y);

        *pixel = Rgb([0, 1, 2].map(|channel| {
            let original = full[channel].to_unit();

            T::from_unit(bias.mul_add(pixel.0[channel].to_unit() - original, original))
        }));
    }

    corrected
}

/// The photograph moved towards `corrected` by `weight`, pixel by pixel, in place.
///
/// [`blended`] with the bias read from `weight` at each pixel and bounded to `[0, 1]`: `source + w * (corrected -
/// source)`. `source` is at `corrected`'s size; `weight` is read bilinearly, pixel centres aligned, where its size
/// differs, so a mask rasterised at the photograph's size serves an upscaled result without a plane the result's size.
///
/// Where the weight is exactly **1** the correction is kept untouched, and where it is exactly **0** the photograph
/// comes back byte for byte at the correction's depth, so the parts of a mask that are wholly white or wholly black are
/// not a rounding away from either side. A weight that is not a number is 0.
///
/// # Panics
///
/// Panics where `weight` has no area and `corrected` has some.
pub fn masked<T: Channel + Send>(
    source: &Sampler<'_>,
    mut corrected: ImageBuffer<Rgb<T>, Vec<T>>,
    weight: &Plane,
) -> ImageBuffer<Rgb<T>, Vec<T>>
where
    Rgb<T>: image::Pixel<Subpixel = T>,
{
    let (width, height) = corrected.dimensions();

    if width == 0 || height == 0 {
        return corrected;
    }

    assert!(
        weight.width() > 0 && weight.height() > 0,
        "a {}x{} weight cannot mask a {width}x{height} result",
        weight.width(),
        weight.height()
    );

    // The two whole-frame answers without a pass: a mask the user painted everywhere, or one cleared to nothing and
    // inverted. Bilinear reads of a plane whose every value is 1 (or 0) are that value, so the loop below would give
    // the same pixels; the photograph's side goes through `flattened`, which is the conversion the loop performs.
    match weight.range() {
        (lo, _) if lo >= 1.0 => return corrected,
        (_, hi) if hi <= 0.0 => return flattened(source, width, height),
        _ => {}
    }

    let (columns, rows) = (Axis::new(weight.width(), width), Axis::new(weight.height(), height));
    let stride = weight.width() as usize;
    let plane = weight.as_slice();

    // Row-banded, as the other full-resolution passes are: each row reads its own photograph row and writes only its
    // own slice of the result, so the output is the serial one exactly.
    for_each_row(&mut corrected, 3 * width as usize, height as usize, |y, out| {
        let row = rows.tap(y);
        let (top, bottom) = (&plane[row.lo * stride..][..stride], &plane[row.hi * stride..][..stride]);
        let mut photograph = vec![[0_u16; 3]; width as usize];
        source.row(y as u32, &mut photograph);

        for (x, (pixel, original)) in out.as_chunks_mut::<3>().0.iter_mut().zip(&photograph).enumerate() {
            let column = columns.tap(x);
            let upper = top[column.lo] * (1.0 - column.frac) + top[column.hi] * column.frac;
            let lower = bottom[column.lo] * (1.0 - column.frac) + bottom[column.hi] * column.frac;
            let w = upper * (1.0 - row.frac) + lower * row.frac;
            // A weight that is not a number is none, as `range` counts it: `clamp` would hand NaN on.
            let w = if w.is_nan() { 0.0 } else { w.clamp(0.0, 1.0) };

            if w >= 1.0 {
                continue;
            }

            for (channel, original) in pixel.iter_mut().zip(original) {
                let original = original.to_unit();

                *channel = if w <= 0.0 {
                    T::from_unit(original)
                } else {
                    T::from_unit(w.mul_add(channel.to_unit() - original, original))
                };
            }
        }
    });

    corrected
}

#[cfg(test)]
mod tests {
    use image::DynamicImage;

    use super::*;

    use crate::test_support::photograph;

    /// A flat correction at `value` in every channel, over an extent, for the blend tests.
    fn flat(width: u32, height: u32, value: u8) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        ImageBuffer::from_pixel(width, height, Rgb([value; 3]))
    }

    #[test]
    fn a_bias_of_zero_returns_the_photograph_byte_for_byte() {
        // The one place in this file where "unchanged" means exactly that rather than within a level: nothing here
        // goes through a resample, so `to_unit` and `from_unit` are inverse and the blend term is multiplied by zero.
        let source = photograph(23, 17);
        let sampler = Sampler::new(&source);

        let produced = blended(&sampler, flat(23, 17, 200), 0.0);

        for (x, y, pixel) in produced.enumerate_pixels() {
            let [r, g, b] = sampler.rgb(x, y);

            assert_eq!(
                pixel.0,
                [r, g, b].map(|value| u8::from_unit(value.to_unit())),
                "a bias of zero changed ({x}, {y})"
            );
        }
    }

    #[test]
    fn a_bias_of_one_returns_the_correction_unchanged() {
        let source = photograph(23, 17);
        let correction = flat(23, 17, 200);

        let produced = blended(&Sampler::new(&source), correction.clone(), 1.0);

        assert_eq!(produced.as_raw(), correction.as_raw(), "a bias of one did not return the correction");
    }

    #[test]
    fn a_negative_bias_moves_the_photograph_the_other_way_along_the_same_line() {
        // Not a second model and not a run in reverse: the same adjustment, extrapolated past the photograph. So a
        // correction that brightens must darken at -1, and by the same distance it brightened by at 1.
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(9, 9, Rgb([100_u8; 3])));
        let sampler = Sampler::new(&source);

        let brighter = blended(&sampler, flat(9, 9, 160), 1.0);
        let darker = blended(&sampler, flat(9, 9, 160), -1.0);

        assert_eq!(brighter.get_pixel(4, 4).0, [160; 3], "a bias of one did not reach the correction");
        assert_eq!(darker.get_pixel(4, 4).0, [40; 3], "a bias of minus one did not mirror it about the photograph");

        // Stated as the property rather than as the two numbers: the photograph sits exactly between them.
        for channel in 0..3 {
            let (up, down) =
                (i32::from(brighter.get_pixel(4, 4).0[channel]), i32::from(darker.get_pixel(4, 4).0[channel]));

            assert_eq!(up - 100, 100 - down, "the two directions moved by different distances");
        }
    }

    #[test]
    fn an_extrapolation_past_the_channel_is_bounded_rather_than_wrapped() {
        // The `from_unit` bound, on the input that reaches it: a dark photograph and a bright correction at -1 take
        // the result below zero, which without the bound would wrap to the top of the channel — a black shadow
        // rendered white, and the worst-looking failure this arithmetic has.
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(4, 4, Rgb([10_u8; 3])));

        let under = blended(&Sampler::new(&source), flat(4, 4, 250), -1.0);
        let over = blended(&Sampler::new(&source), flat(4, 4, 250), 1.0);

        assert_eq!(under.get_pixel(0, 0).0, [0; 3], "an underflowing blend did not bound to black");
        assert_eq!(over.get_pixel(0, 0).0, [250; 3]);
    }

    #[test]
    fn a_bias_between_the_endpoints_moves_the_photograph_proportionally() {
        // The middle of the range, which neither endpoint test covers and which is what a slider spends its time
        // in: half the bias is half the distance.
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(4, 4, Rgb([60_u8; 3])));

        for (bias, expected) in [(0.25_f32, 75_u8), (0.5, 90), (0.75, 105)] {
            let produced = blended(&Sampler::new(&source), flat(4, 4, 120), bias);

            assert_eq!(produced.get_pixel(0, 0).0, [expected; 3], "a bias of {bias} did not land proportionally");
        }
    }
    /// A plane `width` wide whose left `split` columns hold `left` and whose others hold `right`.
    fn halves(width: u32, height: u32, split: u32, (left, right): (f32, f32)) -> Plane {
        let values = (0..width * height).map(|index| if index % width < split { left } else { right }).collect();

        Plane::from_vec(width, height, values).unwrap()
    }

    #[test]
    fn a_weight_of_zero_returns_the_photograph_and_one_the_correction_byte_for_byte() {
        // Both in one plane, so the pass rather than a whole-frame shortcut decides each pixel.
        let source = photograph(23, 17);
        let sampler = Sampler::new(&source);
        let correction = flat(23, 17, 200);

        let produced = masked(&sampler, correction.clone(), &halves(23, 17, 11, (0.0, 1.0)));

        for (x, y, pixel) in produced.enumerate_pixels() {
            let expected = if x < 11 { source.to_rgb8().get_pixel(x, y).0 } else { correction.get_pixel(x, y).0 };

            assert_eq!(pixel.0, expected, "({x}, {y}) is neither side exactly");
        }
    }

    #[test]
    fn a_weight_that_is_not_a_number_is_the_photograph() {
        let source = photograph(12, 8);
        let sampler = Sampler::new(&source);

        // Wholly NaN takes the whole-frame shortcut; a NaN beside whole weights takes the pass.
        let all = masked(&sampler, flat(12, 8, 200), &Plane::new(12, 8, f32::NAN));
        assert_eq!(all.as_raw(), source.to_rgb8().as_raw(), "a plane of NaN did not give the photograph");

        let beside = halves(12, 8, 6, (f32::NAN, 1.0));
        let produced = masked(&sampler, flat(12, 8, 200), &beside);

        assert_eq!(produced.get_pixel(1, 1).0, source.to_rgb8().get_pixel(1, 1).0);
        assert_eq!(produced.get_pixel(10, 1).0, [200; 3]);
    }

    #[test]
    fn a_plane_of_one_value_is_the_scalar_blend() {
        // The whole-frame answers at both ends and the pass between them agree with `blended` at that bias.
        let source = photograph(19, 13);
        let sampler = Sampler::new(&source);

        for value in [0.0_f32, 0.25, 0.5, 1.0] {
            let produced = masked(&sampler, flat(19, 13, 120), &Plane::new(19, 13, value));
            let expected = blended(&sampler, flat(19, 13, 120), value);

            assert_eq!(produced.as_raw(), expected.as_raw(), "a plane of {value} is not a bias of {value}");
        }
    }

    #[test]
    fn a_half_weight_mixes_proportionally_and_a_weight_out_of_range_is_bounded() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(8, 4, Rgb([60_u8; 3])));
        let sampler = Sampler::new(&source);

        let produced = masked(&sampler, flat(8, 4, 120), &halves(8, 4, 4, (0.5, 0.75)));
        assert_eq!(produced.get_pixel(1, 1).0, [90; 3]);
        assert_eq!(produced.get_pixel(6, 1).0, [105; 3]);

        // A weight a fault put past either end is the end it passed, never an extrapolation.
        let bounded = masked(&sampler, flat(8, 4, 120), &halves(8, 4, 4, (-1.0, 2.0)));
        assert_eq!(bounded.get_pixel(1, 1).0, [60; 3]);
        assert_eq!(bounded.get_pixel(6, 1).0, [120; 3]);
    }

    #[test]
    fn a_quarter_size_plane_is_read_at_the_results_size() {
        // A 2x2 plane over an 8x8 result: each quadrant's inner pixels read their own value, and the columns between
        // two quadrants a mix, as a bilinear read of centres aligned gives.
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(8, 8, Rgb([0_u8; 3])));
        let weight = Plane::from_vec(2, 2, vec![0.0, 1.0, 0.0, 1.0]).unwrap();

        let produced = masked(&Sampler::new(&source), flat(8, 8, 200), &weight);

        assert_eq!(produced.get_pixel(0, 0).0, [0; 3], "the left quadrant's edge is not the photograph");
        assert_eq!(produced.get_pixel(7, 7).0, [200; 3], "the right quadrant's edge is not the correction");

        let across: Vec<u8> = (0..8).map(|x| produced.get_pixel(x, 3).0[0]).collect();
        assert!(across.windows(2).all(|pair| pair[0] <= pair[1]), "the read is not monotonic: {across:?}");
        assert!(across[3] > 0 && across[4] < 200, "no mix between the quadrants: {across:?}");
    }

    #[test]
    fn a_sixteen_bit_correction_keeps_its_depth_and_its_low_bytes() {
        let source = DynamicImage::ImageRgb16(ImageBuffer::from_fn(6, 3, |x, _| Rgb([1000 + x as u16; 3])));
        let correction: ImageBuffer<Rgb<u16>, Vec<u16>> = ImageBuffer::from_pixel(6, 3, Rgb([3001; 3]));

        let produced = masked(&Sampler::new(&source), correction, &halves(6, 3, 3, (0.0, 0.5)));

        assert_eq!(produced.get_pixel(1, 0).0, [1001; 3], "the 16-bit photograph lost its low byte");
        assert_eq!(produced.get_pixel(5, 0).0, [2003; 3], "the 16-bit mix is not halfway");
    }
}
