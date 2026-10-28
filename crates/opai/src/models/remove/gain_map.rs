//! A removal's Ultra HDR gain map: the gain under the removed region filled from the gain around it, through the
//! feather the colour paste used, at the gain map's own resolution.

// Why a fill of the map's own and not LaMa (the series design's D10): a gain map is a smooth field of how much brighter
// each part of the scene is in the HDR rendition, with no texture for an inpainting model to continue, and the removed
// object's own gain — a lamp's, a white shirt's — is exactly what must not survive under the fill, or an HDR display
// lifts a bright ghost of the object over the background LaMa painted. A push-pull membrane over the cells the hole
// does not touch continues the surrounding gain smoothly across it.
//
// The fill is in the map's encoded values (the log-gain, after its gamma), which is the space the field is smooth in;
// nothing here reads the map's parameters, so no gain map is refused for them.
//
// The region is the colour paste's, cell for cell: a cell is part of the hole the fill does not read where any
// photograph pixel under it is within `HOLE` of a mark, and takes the fill by the mean of `fill_weight` over the
// pixels under it, so a map at the photograph's resolution gets exactly the colour's feather and a quarter-resolution
// one the same feather averaged over each cell. A cell with no pixel inside the hole has weight 0 and keeps its byte.

use image::{ColorType, DynamicImage, GrayImage, RgbImage};
use imaging::fill::filled;
use imaging::plane::Plane;

use super::region::{Rect, fill_weight, in_hole};
use crate::image::GainMap;

/// One region a removal filled: the crop it was inferred over, and the squared distance of every pixel of the crop
/// from the marks, in rows of the crop's width (`region::distances`).
pub(super) struct Filled {
    pub(super) crop: Rect,
    pub(super) squared: Vec<u16>,
}

/// `gain_map`, of a `photograph`-sized base, with the gain under each of `regions` filled from the gain around it.
///
/// # Errors
///
/// The reason, worded to follow "could not rewrite the HDR gain map:", where the gain map is larger than the photograph
/// on either axis (it covers the photograph's scene, so its cells could not be laid over its pixels) or is not 8-bit
/// grayscale or RGB.
pub(super) fn rewritten(gain_map: &GainMap, photograph: (u32, u32), regions: &[Filled]) -> Result<GainMap, String> {
    let pixels = gain_map.pixels();
    let (map_width, map_height) = (pixels.width(), pixels.height());
    let (width, height) = photograph;

    if map_width == 0 || map_height == 0 || map_width > width || map_height > height {
        return Err(format!("it is {map_width}x{map_height}, which does not fit the {width}x{height} photograph"));
    }

    let channels = match pixels.color() {
        ColorType::L8 => 1,
        ColorType::Rgb8 => 3,
        other => return Err(format!("it is stored as {other:?}, not 8-bit grayscale or RGB")),
    };

    let source = pixels.as_bytes();
    let mut out = source.to_vec();
    let scale = Scale { map: (map_width, map_height), photograph };

    for region in regions {
        fill_region(source, &mut out, channels, &scale, region);
    }

    let rewritten = match channels {
        1 => DynamicImage::ImageLuma8(GrayImage::from_raw(map_width, map_height, out).expect("the map's own size")),
        _ => DynamicImage::ImageRgb8(RgbImage::from_raw(map_width, map_height, out).expect("the map's own size")),
    };
    Ok(gain_map.rewritten(rewritten))
}

/// How a gain map's cells lie over the photograph's pixels: by their sizes, since the map covers the same scene.
struct Scale {
    map: (u32, u32),
    photograph: (u32, u32),
}

impl Scale {
    /// The photograph pixels under cell `cell` of an axis `cells` long over `pixels`: from the floor of its start to the
    /// ceiling of its end, at least one.
    fn footprint(cell: u32, cells: u32, pixels: u32) -> std::ops::Range<u32> {
        let (cell, cells, pixels) = (u64::from(cell), u64::from(cells), u64::from(pixels));
        let start = cell * pixels / cells;
        let end = ((cell + 1) * pixels).div_ceil(cells).max(start + 1).min(pixels);

        // Both within `pixels`, a `u32`.
        start as u32..end as u32
    }

    /// The cells of an axis `cells` long over `pixels` that lie over any of `from..to`.
    fn covering(from: u32, to: u32, cells: u32, pixels: u32) -> std::ops::Range<u32> {
        let (from, to, cells64, pixels) = (u64::from(from), u64::from(to), u64::from(cells), u64::from(pixels));
        let start = from * cells64 / pixels;
        let end = (to * cells64).div_ceil(pixels).min(cells64);

        start as u32..end as u32
    }
}

/// Fills `region`'s hole in `out`, the gain map's bytes, from `source`, the bytes as they were: one push-pull over the
/// cells under the region's crop, which reaches well past the hole on every side the frame allows.
fn fill_region(source: &[u8], out: &mut [u8], channels: usize, scale: &Scale, region: &Filled) {
    let Filled { crop, squared } = region;
    let ((map_width, map_height), (width, height)) = (scale.map, scale.photograph);
    let columns = Scale::covering(crop.x, crop.right(), map_width, width);
    let rows = Scale::covering(crop.y, crop.bottom(), map_height, height);
    let (cells_wide, cells_high) = (columns.len(), rows.len());

    // Per cell under the crop: whether the fill may read it, and how much of the fill it takes.
    let mut known = Vec::with_capacity(cells_wide * cells_high);
    let mut weight = Vec::with_capacity(cells_wide * cells_high);
    for row in rows.clone() {
        let ys = Scale::footprint(row, map_height, height);

        for column in columns.clone() {
            let xs = Scale::footprint(column, map_width, width);
            let (mut hole, mut sum, mut count) = (false, 0.0_f32, 0_u32);

            for y in ys.clone() {
                for x in xs.clone() {
                    count += 1;
                    // A pixel outside the crop is far from every mark: the crop holds the hole with context to spare.
                    if !(crop.x..crop.right()).contains(&x) || !(crop.y..crop.bottom()).contains(&y) {
                        continue;
                    }
                    let squared = squared[((y - crop.y) * crop.width + (x - crop.x)) as usize];
                    hole |= in_hole(squared);
                    sum += fill_weight(squared);
                }
            }

            known.push(if hole { 0.0 } else { 1.0 });
            weight.push(sum / count.max(1) as f32);
        }
    }

    if !known.contains(&0.0) {
        return;
    }

    let stride = map_width as usize * channels;
    let at = |row: u32, column: u32| row as usize * stride + column as usize * channels;
    let mut values = Vec::with_capacity(channels * cells_wide * cells_high);
    for row in rows.clone() {
        let start = at(row, columns.start);
        values.extend(source[start..start + cells_wide * channels].iter().map(|byte| f32::from(*byte) / 255.0));
    }

    let (cells_wide32, cells_high32) = (cells_wide as u32, cells_high as u32);
    let known = Plane::from_vec(cells_wide32, cells_high32, known).expect("one weight a cell");
    let fill = filled(&values, channels, cells_wide32, cells_high32, &known);

    for (local_row, row) in rows.enumerate() {
        for (local_column, column) in columns.clone().enumerate() {
            let cell = local_row * cells_wide + local_column;
            let weight = weight[cell];
            if weight <= 0.0 {
                continue;
            }

            let start = at(row, column);
            for channel in 0..channels {
                let own = f32::from(source[start + channel]) / 255.0;
                let mixed = weight.mul_add(fill[cell * channels + channel] - own, own);
                // In [0, 1] and rounded to a byte.
                out[start + channel] = (mixed.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::remove::region::{GROW, HOLE, Marks, crops, distances, regions};

    /// The regions a removal of `marked` fills in a `width` x `height` photograph, as the pipeline computes them.
    fn filled_regions(width: u32, height: u32, marked: Rect) -> Vec<Filled> {
        let mut plane = Plane::new(width, height, 0.0);
        for y in marked.y..marked.bottom() {
            for x in marked.x..marked.right() {
                plane.as_mut_slice()[(y * width + x) as usize] = 1.0;
            }
        }
        let marks = Marks::of(&plane);

        crops(&regions(&marks), width, height)
            .into_iter()
            .map(|crop| Filled { crop, squared: distances(&marks, crop) })
            .collect()
    }

    /// A gain map whose gain rises smoothly left to right, with a bright square of 250 over `bright` (in its own cells).
    fn gain_map(width: u32, height: u32, base: (u32, u32), bright: Rect, channels: u8) -> GainMap {
        let value = |x: u32, y: u32| {
            if (bright.x..bright.right()).contains(&x) && (bright.y..bright.bottom()).contains(&y) {
                250
            } else {
                (40 + x * 120 / width) as u8
            }
        };
        let pixels = match channels {
            1 => DynamicImage::ImageLuma8(GrayImage::from_fn(width, height, |x, y| image::Luma([value(x, y)]))),
            _ => DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
                let v = value(x, y);
                image::Rgb([v, v / 2, v / 3])
            })),
        };
        GainMap::holding(pixels, base)
    }

    /// The nearest a pixel is to `marked`, squared.
    fn squared_distance(x: u32, y: u32, marked: Rect) -> u32 {
        let dx = if x < marked.x { marked.x - x } else { x.saturating_sub(marked.right() - 1) };
        let dy = if y < marked.y { marked.y - y } else { y.saturating_sub(marked.bottom() - 1) };
        dx * dx + dy * dy
    }

    const fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
        Rect { x, y, width, height }
    }

    #[test]
    fn a_full_resolution_map_is_filled_from_its_surroundings_and_nothing_past_the_hole_changes() {
        for channels in [1, 3] {
            let (width, height) = (200, 150);
            let marked = rect(90, 60, 12, 20);
            // The bright square is the removed object's gain, a little wider than the marks.
            let map = gain_map(width, height, (width, height), rect(88, 58, 16, 24), channels);

            let rewritten = rewritten(&map, (width, height), &filled_regions(width, height, marked)).unwrap();
            let (before, after) = (map.pixels().to_rgb8(), rewritten.pixels().to_rgb8());
            assert_eq!(rewritten.pixels().color(), map.pixels().color(), "{channels}: the colour type changed");

            for (x, y, pixel) in after.enumerate_pixels() {
                let squared = squared_distance(x, y, marked);
                let surrounding = i32::from((40 + x * 120 / width) as u8);

                if squared <= GROW * GROW {
                    // The surrounding gradient, not the object's 250.
                    let got = i32::from(pixel.0[0]);
                    assert!((got - surrounding).abs() <= 8, "{channels}: ({x}, {y}) is {got}, around it {surrounding}");
                } else if squared >= HOLE * HOLE {
                    assert_eq!(pixel, before.get_pixel(x, y), "{channels}: ({x}, {y}) past the hole changed");
                }
            }
            assert!(rewritten.source().is_none(), "a rewritten map still claims the source's bytes");
            assert_eq!(rewritten.metadata().xmp, map.metadata().xmp, "its own metadata is kept");
        }
    }

    #[test]
    fn a_quarter_resolution_map_is_filled_in_its_own_cells() {
        let (width, height) = (200, 152);
        let marked = rect(90, 60, 12, 20);
        // Cells of 4x4 pixels: the marks are cells 22..26 by 15..20; the object's gain a cell wider.
        let map = gain_map(50, 38, (width, height), rect(21, 14, 6, 7), 3);

        let rewritten = rewritten(&map, (width, height), &filled_regions(width, height, marked)).unwrap();
        let (before, after) = (map.pixels().to_rgb8(), rewritten.pixels().to_rgb8());
        assert_eq!((after.width(), after.height()), (50, 38), "the map was resampled");

        for (column, row, pixel) in after.enumerate_pixels() {
            // The nearest and farthest a pixel under this cell is from the marks.
            let xs = column * 4..column * 4 + 4;
            let ys = row * 4..row * 4 + 4;
            let distances: Vec<u32> =
                ys.flat_map(|y| xs.clone().map(move |x| squared_distance(x, y, marked))).collect();
            let (nearest, farthest) = (*distances.iter().min().unwrap(), *distances.iter().max().unwrap());
            let surrounding = i32::from((40 + column * 120 / 50) as u8);

            if farthest <= GROW * GROW {
                let got = i32::from(pixel.0[0]);
                assert!((got - surrounding).abs() <= 10, "cell ({column}, {row}) is {got}, around it {surrounding}");
            } else if nearest >= HOLE * HOLE {
                assert_eq!(pixel, before.get_pixel(column, row), "cell ({column}, {row}) past the hole changed");
            }
        }
        // And the object's gain is gone from every cell under the marks.
        assert!(after.get_pixel(23, 17).0[0] < 200, "the object's gain survived");
    }

    #[test]
    fn a_map_larger_than_the_photograph_is_refused() {
        let (width, height) = (60, 40);
        let regions = filled_regions(width, height, rect(28, 18, 4, 4));

        let larger = gain_map(80, 40, (80, 40), rect(0, 0, 0, 0), 1);
        let reason = rewritten(&larger, (width, height), &regions).unwrap_err();
        assert!(reason.contains("80x40") && reason.contains("60x40"), "{reason}");
    }

    #[test]
    fn nothing_to_fill_keeps_every_byte() {
        let map = gain_map(60, 40, (60, 40), rect(10, 10, 4, 4), 3);

        let kept = rewritten(&map, (60, 40), &[]).unwrap();

        assert_eq!(kept.pixels().as_bytes(), map.pixels().as_bytes());
    }

    #[test]
    fn a_cell_lies_over_the_pixels_its_share_of_the_scene_covers() {
        assert_eq!(Scale::footprint(0, 50, 200), 0..4);
        assert_eq!(Scale::footprint(49, 50, 200), 196..200);
        // Three cells over ten pixels: overlapping by the pixel each boundary cuts.
        assert_eq!(Scale::footprint(1, 3, 10), 3..7);
        assert_eq!(Scale::covering(90, 102, 50, 200), 22..26);
        assert_eq!(Scale::covering(0, 200, 50, 200), 0..50);
        assert_eq!(Scale::footprint(5, 10, 10), 5..6, "a full-resolution cell is its pixel");
    }
}
