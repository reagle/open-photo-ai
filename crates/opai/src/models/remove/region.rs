//! Where a removal fills, and with what weight: the marks, the regions they split into, the crop each region is inferred
//! over, the distance of every pixel from the marks, and the two resamplings between a crop and the model's square.
//!
//! Pure arithmetic over planes and rectangles, with no session and no photograph, so every rule here is a test.

// Family tier, under `remove/`, because only the remove family grows marks into a hole and feathers a fill into a
// photograph. Face recovery's feathered paste is a fixed ellipse on an aligned square, private to that family, and
// shares nothing with a distance from an arbitrary painted region; were a second family to need these, they would move
// to `imaging`, which is where a plane-level primitive belongs.
//
// The figures are the prototype's (`runB.py` in the fork's `infill` notes), which the comparison of 2026-10-27 chose:
// marks grown by 8 px before they are filled, the model's hole 8 px wider again, and context of `0.6 * longer side + 64`
// px around each region. The one departure is the feather (`fill_weight`).

use imaging::plane::Plane;
use imaging::rows::for_each_row;

/// The weight at or above which a pixel of a rasterised removal mask is marked for removal.
///
/// A removal is all or nothing per pixel: what softens its edge is [`fill_weight`], not a grey in the mask.
pub(crate) const MARKED: f32 = 0.5;

/// How far the marks are grown before the fill replaces the photograph outright, in pixels.
///
/// What it covers is a pixel-scale property of the capture — the object's anti-aliased edge, its halo, the ringing JPEG
/// leaves around it — so it is in pixels of the photograph rather than a fraction of it.
pub(crate) const GROW: u32 = 8;

/// How far beyond the marks the model is shown a hole, in pixels: the grown marks and the band the fill is feathered
/// over. Nothing at this distance or beyond is changed.
pub(crate) const HOLE: u32 = 16;

/// The context around a region, as a fraction of its longer side, on every side.
const CONTEXT_FRACTION: f64 = 0.6;

/// The context around a region beyond [`CONTEXT_FRACTION`], in pixels: what a small region gets at the least.
const CONTEXT_PIXELS: u32 = 64;

/// Past this many separate regions the marks are inferred as one: a mask of thousands of specks (a noisy file read at
/// 0.5) would otherwise be thousands of inferences, and a merge whose cost grows with their square.
const MAX_REGIONS: usize = 256;

/// The cap every distance is held at: one past the hole, which is all any caller asks.
const CAP: u32 = HOLE + 1;

/// A rectangle of pixels, `x..x + width` by `y..y + height`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl Rect {
    /// One past the last column.
    pub(crate) const fn right(self) -> u32 {
        self.x + self.width
    }

    /// One past the last row.
    pub(crate) const fn bottom(self) -> u32 {
        self.y + self.height
    }

    /// The smallest rectangle holding both.
    fn union(self, other: Self) -> Self {
        let (x, y) = (self.x.min(other.x), self.y.min(other.y));

        Self { x, y, width: self.right().max(other.right()) - x, height: self.bottom().max(other.bottom()) - y }
    }

    /// Whether the two share a pixel.
    fn intersects(self, other: Self) -> bool {
        self.x < other.right() && other.x < self.right() && self.y < other.bottom() && other.y < self.bottom()
    }

    /// This rectangle with `by` pixels added on every side, clamped to a `width` x `height` frame.
    fn grown(self, by: u32, width: u32, height: u32) -> Self {
        let (x, y) = (self.x.saturating_sub(by), self.y.saturating_sub(by));

        Self { x, y, width: (self.right() + by).min(width) - x, height: (self.bottom() + by).min(height) - y }
    }
}

/// Which pixels of a `width` x `height` picture are marked for removal, one flag per pixel in rows.
pub(crate) struct Marks {
    width: u32,
    height: u32,
    flags: Vec<bool>,
}

impl Marks {
    /// The pixels of `weight` at or above [`MARKED`].
    pub(crate) fn of(weight: &Plane) -> Self {
        Self {
            width: weight.width(),
            height: weight.height(),
            flags: weight.as_slice().iter().map(|value| *value >= MARKED).collect(),
        }
    }

    /// Whether `(x, y)` is marked.
    pub(crate) fn get(&self, x: u32, y: u32) -> bool {
        self.flags[(y as usize) * (self.width as usize) + x as usize]
    }
}

/// The bounding boxes of the 8-connected regions of `marks`, in the order their first pixel is met reading the rows.
pub(crate) fn regions(marks: &Marks) -> Vec<Rect> {
    // A flood fill from each unvisited mark, with an explicit stack: a region can be millions of pixels, far past what a
    // recursive one would survive. 8-connected, so a stroke drawn as a diagonal staircase of pixels is one region.
    let (width, height) = (marks.width as usize, marks.height as usize);
    let mut seen = vec![false; marks.flags.len()];
    let mut stack = Vec::new();
    let mut boxes = Vec::new();

    for start in 0..marks.flags.len() {
        if !marks.flags[start] || seen[start] {
            continue;
        }

        seen[start] = true;
        stack.push(start);
        let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);

        while let Some(index) = stack.pop() {
            let (x, y) = (index % width, index / width);
            (left, top, right, bottom) = (left.min(x), top.min(y), right.max(x), bottom.max(y));

            for ny in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    let neighbour = ny * width + nx;

                    if marks.flags[neighbour] && !seen[neighbour] {
                        seen[neighbour] = true;
                        stack.push(neighbour);
                    }
                }
            }
        }

        // Every coordinate is inside a picture whose sides are `u32`s.
        boxes.push(Rect {
            x: left as u32,
            y: top as u32,
            width: (right - left + 1) as u32,
            height: (bottom - top + 1) as u32,
        });
    }

    boxes
}

/// The crop a region whose marks span `bounds` is inferred over, in a `width` x `height` frame: the marks grown by the
/// fill, then [`CONTEXT_FRACTION`] of that box's longer side plus [`CONTEXT_PIXELS`] on every side, clamped to the
/// frame.
fn context(bounds: Rect, width: u32, height: u32) -> Rect {
    let grown = bounds.grown(GROW, width, height);

    // A fraction of a side of a picture whose sides are `u32`s, rounded: positive, and it fits.
    let pad = (CONTEXT_FRACTION * f64::from(grown.width.max(grown.height))).round() as u32 + CONTEXT_PIXELS;

    grown.grown(pad, width, height)
}

/// The crops `regions` are inferred over in a `width` x `height` frame: one per region with its own context, regions
/// whose crops would overlap merged into one, in reading order of their top-left corners.
///
/// No two crops overlap, so no pixel is pasted twice and no crop holds another region's marks unfilled; and every mark
/// is inside the crop of its region, at least [`HOLE`] pixels from its edge unless the frame's own edge is nearer.
pub(crate) fn crops(regions: &[Rect], width: u32, height: u32) -> Vec<Rect> {
    // The merge works on the marks' bounds and recomputes the context from their union, so a merged crop has the context
    // its whole extent calls for rather than the union of two smaller ones.
    let mut bounds: Vec<Rect> = if regions.len() > MAX_REGIONS {
        regions.iter().copied().reduce(Rect::union).into_iter().collect()
    } else {
        regions.to_vec()
    };

    'merging: loop {
        for first in 0..bounds.len() {
            for second in first + 1..bounds.len() {
                if context(bounds[first], width, height).intersects(context(bounds[second], width, height)) {
                    bounds[first] = bounds[first].union(bounds[second]);
                    bounds.swap_remove(second);
                    continue 'merging;
                }
            }
        }

        break;
    }

    let mut crops: Vec<Rect> = bounds.into_iter().map(|bounds| context(bounds, width, height)).collect();
    crops.sort_by_key(|crop| (crop.y, crop.x));
    crops
}

/// The squared distance from every pixel of `crop` to the nearest mark inside it, in rows of `crop.width`, capped at
/// `(HOLE + 1)²`.
///
/// Exact Euclidean distance between pixel centres, so the region a distance bounds is a disc and not a square.
pub(crate) fn distances(marks: &Marks, crop: Rect) -> Vec<u16> {
    // Two passes, each separable in the way a capped Euclidean distance is: per row, the distance along the row to the
    // nearest mark in it; then per pixel, the least `dy² + row²` over the rows within the cap above and below. The second
    // pass reads the first immutably and writes its own row, so it runs across cores with the serial result exactly.
    let (width, height) = (crop.width as usize, crop.height as usize);
    let mut along = vec![CAP as u8; width * height];

    for (row, out) in along.chunks_mut(width).enumerate() {
        let y = crop.y + row as u32;

        // Left to right, then right to left, each carrying the distance since the last mark, capped.
        let mut since = CAP;
        for (column, value) in out.iter_mut().enumerate() {
            since = if marks.get(crop.x + column as u32, y) { 0 } else { (since + 1).min(CAP) };
            *value = since as u8;
        }

        let mut since = CAP;
        for value in out.iter_mut().rev() {
            since = if *value == 0 { 0 } else { (since + 1).min(CAP) };
            *value = (*value).min(since as u8);
        }
    }

    let cap = (CAP * CAP) as u16;
    let mut squared = vec![cap; width * height];
    let reach = HOLE as usize;

    for_each_row(&mut squared, width, height, |row, out| {
        for dy in row.saturating_sub(reach)..=(row + reach).min(height - 1) {
            let vertical = (dy.abs_diff(row) * dy.abs_diff(row)) as u16;
            let source = &along[dy * width..(dy + 1) * width];

            for (value, horizontal) in out.iter_mut().zip(source) {
                let horizontal = u16::from(*horizontal);
                *value = (*value).min(vertical + horizontal * horizontal);
            }
        }

        for value in out.iter_mut() {
            *value = (*value).min(cap);
        }
    });

    squared
}

/// Whether a pixel at `squared` distance from the marks is inside the hole the model is shown.
pub(crate) const fn in_hole(squared: u16) -> bool {
    squared as u32 <= HOLE * HOLE
}

/// How much of the fill a pixel at `squared` distance from the marks takes: 1 within [`GROW`], falling by a smoothstep
/// to 0 at [`HOLE`], and 0 beyond.
pub(crate) fn fill_weight(squared: u16) -> f32 {
    // The prototype pasted through `max(gauss(grown, 4), grown)`, which steps from 1 to about 0.5 at the grown marks'
    // edge. A smoothstep over the band between the grown marks and the hole has no step, reaches 0 exactly where the
    // hole ends — so nothing outside the hole changes by even a level — and is the band the model filled anyway.
    let distance = f32::from(squared).sqrt();
    let (grow, hole) = (GROW as f32, HOLE as f32);

    if distance <= grow {
        return 1.0;
    }

    let t = ((distance - grow) / (hole - grow)).min(1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// The hole a `crop_width` x `crop_height` crop's `squared` distances describe, at the `width` x `height` the crop is
/// resampled to for the model: 1 where any crop pixel under a model pixel is in the hole, 0 elsewhere, in rows.
///
/// Any rather than nearest, so a mark narrower than the reduction (a string a pixel wide, in a crop four thousand
/// across) is still shown to the model as a hole.
pub(crate) fn hole_plane(squared: &[u16], crop_width: u32, crop_height: u32, width: u32, height: u32) -> Vec<f32> {
    // Rows first: each model row is the OR of the crop rows under it, then each model pixel the OR of the columns
    // under it in that row.
    let columns = footprints(crop_width, width);
    let mut plane = vec![0.0_f32; (width as usize) * (height as usize)];

    for (row, (top, bottom)) in footprints(crop_height, height).into_iter().enumerate() {
        let mut any = vec![false; crop_width as usize];
        for y in top..bottom {
            let source = &squared[(y as usize) * (crop_width as usize)..][..crop_width as usize];
            any.iter_mut().zip(source).for_each(|(any, squared)| *any |= in_hole(*squared));
        }

        let out = &mut plane[row * (width as usize)..][..width as usize];
        for (value, (left, right)) in out.iter_mut().zip(&columns) {
            if any[*left as usize..*right as usize].contains(&true) {
                *value = 1.0;
            }
        }
    }

    plane
}

/// For each of `to` cells along an axis `from` pixels long, the pixels under it: at least one, and together covering
/// the axis.
fn footprints(from: u32, to: u32) -> Vec<(u32, u32)> {
    (0..to)
        .map(|cell| {
            let start = (u64::from(cell) * u64::from(from) / u64::from(to)) as u32;
            let end = (u64::from(cell + 1) * u64::from(from)).div_ceil(u64::from(to)) as u32;

            (start.min(from - 1), end.clamp(start.min(from - 1) + 1, from))
        })
        .collect()
}

/// OpenCV's bicubic coefficient, which the prototype's `cv2.resize(..., INTER_CUBIC)` read the fill back with.
const CUBIC: f32 = -0.75;

/// The four weights of a cubic convolution at fractional offset `t` from the second of four taps.
fn cubic_weights(t: f32) -> [f32; 4] {
    let a = CUBIC;
    let near = |x: f32| ((a + 2.0) * x - (a + 3.0)) * x * x + 1.0;
    let far = |x: f32| ((a * x - 5.0 * a) * x + 8.0 * a) * x - 4.0 * a;

    [far(1.0 + t), near(t), near(1.0 - t), far(2.0 - t)]
}

/// Reads planar `planes` (each `side` square) bicubically over their top-left `width` x `height`, as though that part
/// had been resized to `to_width` x `to_height`.
pub(crate) struct Bicubic<'a> {
    planes: &'a [f32],
    side: usize,
    width: u32,
    height: u32,
    scale_x: f32,
    scale_y: f32,
}

impl<'a> Bicubic<'a> {
    pub(crate) fn new(
        planes: &'a [f32],
        side: u32,
        (width, height): (u32, u32),
        (to_width, to_height): (u32, u32),
    ) -> Self {
        Self {
            planes,
            side: side as usize,
            width,
            height,
            scale_x: width as f32 / to_width as f32,
            scale_y: height as f32 / to_height as f32,
        }
    }

    /// The three planes' values at `(x, y)` of the resized picture: pixel centres aligned, edge taps clamped into the
    /// read part, as OpenCV's resize does.
    pub(crate) fn rgb(&self, x: u32, y: u32) -> [f32; 3] {
        let u = (x as f32 + 0.5).mul_add(self.scale_x, -0.5);
        let v = (y as f32 + 0.5).mul_add(self.scale_y, -0.5);
        let (u0, v0) = (u.floor(), v.floor());
        let (wx, wy) = (cubic_weights(u - u0), cubic_weights(v - v0));

        let clamp = |at: f32, length: u32| (at as i64).clamp(0, i64::from(length) - 1) as usize;
        let columns: [usize; 4] = std::array::from_fn(|k| clamp(u0 + k as f32 - 1.0, self.width));
        let rows: [usize; 4] = std::array::from_fn(|k| clamp(v0 + k as f32 - 1.0, self.height));

        let area = self.side * self.side;
        std::array::from_fn(|plane| {
            let plane = &self.planes[plane * area..][..area];

            rows.iter().zip(wy).fold(0.0, |sum, (row, wy)| {
                let line = &plane[row * self.side..];
                let across = columns.iter().zip(wx).fold(0.0, |sum, (column, wx)| wx.mul_add(line[*column], sum));
                wy.mul_add(across, sum)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `width` x `height` set of marks, marked at `points`.
    fn marked(width: u32, height: u32, points: &[(u32, u32)]) -> Marks {
        let mut plane = Plane::new(width, height, 0.0);
        for (x, y) in points {
            plane.as_mut_slice()[(*y as usize) * (width as usize) + *x as usize] = 1.0;
        }

        Marks::of(&plane)
    }

    /// A rectangle.
    const fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
        Rect { x, y, width, height }
    }

    #[test]
    fn a_pixel_is_marked_from_half_weight_up_and_not_below() {
        let plane = Plane::from_vec(4, 1, vec![0.0, 0.49, 0.5, 1.0]).unwrap();
        let marks = Marks::of(&plane);

        assert_eq!([0, 1, 2, 3].map(|x| marks.get(x, 0)), [false, false, true, true]);
        assert!(regions(&Marks::of(&Plane::new(3, 3, 0.49))).is_empty());
    }

    #[test]
    fn regions_are_eight_connected_and_boxed() {
        // A diagonal staircase is one region; a pixel two columns away is another. Met in reading order: (10, 1) is on
        // the first row with a mark.
        let marks = marked(20, 10, &[(2, 2), (3, 3), (4, 4), (4, 5), (10, 1), (19, 9)]);

        assert_eq!(regions(&marks), vec![rect(10, 1, 1, 1), rect(2, 2, 3, 4), rect(19, 9, 1, 1)]);
        assert!(regions(&marked(5, 5, &[])).is_empty());
    }

    #[test]
    fn a_crop_is_the_grown_region_with_its_context_clamped_to_the_frame() {
        // A 20 x 40 region grown by 8 is 36 x 56; its context is 0.6 * 56 + 64 = 97.6, rounded to 98, on every side.
        let crop = crops(&[rect(1000, 1000, 20, 40)], 4000, 3000);
        let pad = GROW + 98;
        assert_eq!(crop, vec![rect(1000 - pad, 1000 - pad, 20 + 2 * pad, 40 + 2 * pad)]);

        // Against the frame's corner, clamped rather than shifted: grown to 18 x 18 (nothing to grow into on the left
        // and above), and 0.6 * 18 + 64 = 74.8, rounded to 75, of context to the right and below.
        let corner = crops(&[rect(0, 0, 10, 10)], 500, 400);
        assert_eq!(corner, vec![rect(0, 0, 18 + 75, 18 + 75)]);
    }

    #[test]
    fn regions_far_apart_keep_their_own_crops_and_near_ones_share_one() {
        let far = crops(&[rect(100, 100, 10, 10), rect(3800, 2800, 10, 10)], 4000, 3000);
        assert_eq!(far.len(), 2, "{far:?}");
        assert!(!far[0].intersects(far[1]));

        // 40 px apart: each crop reaches well past the other's marks, so they are one crop holding both.
        let near = crops(&[rect(1000, 1000, 10, 10), rect(1050, 1000, 10, 10)], 4000, 3000);
        assert_eq!(near.len(), 1, "{near:?}");
        assert!(near[0].x < 1000 && near[0].right() > 1060);
    }

    #[test]
    fn a_merge_can_bring_in_a_region_neither_crop_reached_before() {
        // A and C are too far apart to merge, B is near both: merging A and B grows the crop enough to reach C.
        let regions = [rect(500, 500, 50, 50), rect(700, 500, 50, 50), rect(1000, 500, 50, 50)];
        let merged = crops(&regions, 4000, 3000);

        assert_eq!(merged.len(), 1, "{merged:?}");
        for region in regions {
            assert!(merged[0].x + HOLE <= region.x && region.right() + HOLE <= merged[0].right(), "{region:?}");
        }
    }

    #[test]
    fn crops_never_overlap_and_come_in_reading_order() {
        let regions: Vec<Rect> = (0..12).map(|i| rect(150 + 330 * (i % 6), 200 + 900 * (i / 6), 12, 30)).collect();
        let crops = crops(&regions, 2200, 2000);

        for (index, first) in crops.iter().enumerate() {
            for second in &crops[index + 1..] {
                assert!(!first.intersects(*second), "{first:?} overlaps {second:?}");
            }
        }
        assert!(crops.is_sorted_by_key(|crop| (crop.y, crop.x)));
    }

    #[test]
    fn a_mask_of_more_specks_than_the_regions_allowed_is_one_crop() {
        let specks: Vec<Rect> = (0..=MAX_REGIONS as u32).map(|i| rect(i * 37 % 4000, i * 53 % 3000, 1, 1)).collect();

        assert_eq!(crops(&specks, 4000, 3000).len(), 1);
    }

    #[test]
    fn the_distance_is_euclidean_between_pixel_centres_and_capped() {
        let marks = marked(60, 60, &[(30, 30)]);
        let crop = rect(0, 0, 60, 60);
        let squared = distances(&marks, crop);
        let at = |x: usize, y: usize| squared[y * 60 + x];

        assert_eq!(at(30, 30), 0);
        assert_eq!(at(33, 34), 25, "a 3-4-5 triangle");
        assert_eq!(at(30, 30 + 16), 256);
        assert_eq!(at(30 + 12, 30 + 12), 288);
        assert_eq!(at(0, 0), (CAP * CAP) as u16, "far away is held at the cap");
        assert_eq!(at(30 + 17, 30), (CAP * CAP) as u16);
    }

    #[test]
    fn the_distance_inside_a_crop_reads_the_crops_own_marks() {
        let marks = marked(100, 100, &[(70, 70)]);
        let squared = distances(&marks, rect(60, 60, 30, 30));

        assert_eq!(squared[10 * 30 + 10], 0, "the mark at (70, 70) is (10, 10) of the crop");
        assert_eq!(squared[10 * 30 + 14], 16);
    }

    #[test]
    fn the_distance_is_the_brute_force_one_on_a_scatter() {
        let points = [(5, 7), (20, 3), (21, 3), (11, 30), (35, 36), (0, 39)];
        let marks = marked(40, 40, &points);
        let squared = distances(&marks, rect(0, 0, 40, 40));

        for y in 0..40_i64 {
            for x in 0..40_i64 {
                let nearest = points
                    .iter()
                    .map(|(px, py)| (x - i64::from(*px)).pow(2) + (y - i64::from(*py)).pow(2))
                    .min()
                    .unwrap();
                let expected = nearest.min(i64::from(CAP * CAP));

                assert_eq!(i64::from(squared[(y * 40 + x) as usize]), expected, "({x}, {y})");
            }
        }
    }

    #[test]
    fn the_fill_replaces_within_the_grow_feathers_to_the_hole_and_stops_there() {
        assert_eq!(fill_weight(0), 1.0);
        assert_eq!(fill_weight((GROW * GROW) as u16), 1.0);
        assert_eq!(fill_weight((HOLE * HOLE) as u16), 0.0);
        assert_eq!(fill_weight((CAP * CAP) as u16), 0.0);
        assert!((fill_weight(12 * 12) - 0.5).abs() < 1e-6, "the middle of the band is half");

        let weights: Vec<f32> = (0..=(CAP * CAP) as u16).map(fill_weight).collect();
        assert!(weights.windows(2).all(|pair| pair[1] <= pair[0]), "the feather rose somewhere");

        assert!(in_hole((HOLE * HOLE) as u16) && !in_hole((HOLE * HOLE + 1) as u16));
    }

    #[test]
    fn a_one_pixel_mark_survives_a_large_reduction() {
        // A crop 2048 across shown to the model at 512: a mark one pixel wide is still a hole, a few cells wide.
        let marks = marked(2048, 64, &[(1001, 32)]);
        let squared = distances(&marks, rect(0, 0, 2048, 64));
        let hole = hole_plane(&squared, 2048, 64, 512, 16);

        let holed: Vec<usize> = (0..512).filter(|x| hole[8 * 512 + x] == 1.0).collect();
        assert!(!holed.is_empty() && holed.contains(&250), "{holed:?}");
        assert!(holed.len() <= 10, "the hole grew past the 16 px it is: {holed:?}");
        assert!(hole.iter().all(|value| *value == 0.0 || *value == 1.0));
    }

    #[test]
    fn an_enlarged_hole_keeps_its_place() {
        // A crop smaller than the square, enlarged onto it: each model pixel reads the one crop pixel under it.
        let marks = marked(100, 100, &[(50, 50)]);
        let squared = distances(&marks, rect(0, 0, 100, 100));
        let hole = hole_plane(&squared, 100, 100, 400, 400);

        assert_eq!(hole[200 * 400 + 200], 1.0);
        assert_eq!(hole[200 * 400 + (50 + 17) * 4], 0.0);
        assert_eq!(hole[10], 0.0);
    }

    #[test]
    fn every_footprint_has_a_pixel_and_together_they_cover_the_axis() {
        for (from, to) in [(2048, 512), (100, 400), (513, 512), (1, 512), (7, 3)] {
            let cells = footprints(from, to);

            assert_eq!(cells.first().unwrap().0, 0);
            assert_eq!(cells.last().unwrap().1, from);
            assert!(cells.iter().all(|(start, end)| start < end && *end <= from), "{from} -> {to}: {cells:?}");
            assert!(cells.windows(2).all(|pair| pair[1].0 <= pair[0].1), "{from} -> {to} left a gap");
        }
    }

    /// Three planes of a `side` square, each `value(x, y)` scaled by the plane's index plus one.
    fn planes(side: usize, value: impl Fn(usize, usize) -> f32) -> Vec<f32> {
        (0..3)
            .flat_map(|plane| {
                (0..side * side).map(|i| (plane + 1) as f32 * value(i % side, i / side)).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    }

    #[test]
    fn the_bicubic_read_of_a_flat_plane_is_flat_and_of_a_ramp_is_the_ramp() {
        let flat = planes(16, |_, _| 0.25);
        let read = Bicubic::new(&flat, 16, (12, 10), (37, 29));
        for (x, y) in [(0, 0), (36, 28), (18, 3)] {
            let rgb = read.rgb(x, y);
            assert!((rgb[0] - 0.25).abs() < 1e-6 && (rgb[2] - 0.75).abs() < 1e-6, "({x}, {y}): {rgb:?}");
        }

        // A ramp is read back as the ramp, rising, away from the clamped edge: to within OpenCV's coefficient, which
        // reproduces a line only to about 0.04 of a source pixel (-0.5 would reproduce it exactly, and read softer).
        let ramp = planes(16, |x, _| x as f32 / 16.0);
        let read = Bicubic::new(&ramp, 16, (16, 16), (64, 64));
        let values: Vec<f32> = (10..50).map(|x| read.rgb(x, 20)[0]).collect();
        for (x, value) in (10..50).zip(&values) {
            let expected = ((x as f32 + 0.5) / 4.0 - 0.5) / 16.0;
            assert!((value - expected).abs() < 0.003, "{x}: {value} against {expected}");
        }
        assert!(values.windows(2).all(|pair| pair[1] > pair[0]), "{values:?}");
    }

    #[test]
    fn a_bicubic_read_at_the_same_size_is_the_plane_itself() {
        let noise = planes(8, |x, y| ((x * 7 + y * 13) % 11) as f32 / 10.0);
        let read = Bicubic::new(&noise, 8, (8, 6), (8, 6));

        for y in 0..6 {
            for x in 0..8 {
                assert!((read.rgb(x, y)[1] - noise[64 + (y as usize) * 8 + x as usize]).abs() < 1e-6);
            }
        }
    }
}
