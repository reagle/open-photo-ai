//! The fill refined at the photograph's own resolution: the hole synthesised again from patches of the photograph around
//! it, guided by the model's fill, so a region that was reduced to reach the model's square comes back with the
//! photograph's own texture rather than a soft enlargement of the model's.
//!
//! Guided PatchMatch (Barnes et al. 2009) inside the expectation-maximisation of Wexler et al. (2007), as Content-Aware
//! Fill is built: every 7 x 7 patch that overlaps the hole is matched to a patch of known photograph in the crop, the
//! hole is voted from the patches matched to it, and the two alternate, coarse to fine over a small pyramid. The match
//! has two terms: the patch as it now is, and the model's fill as a guide, both blurred alike to the detail the model
//! could resolve, so the structure is the model's and the texture the photograph's; and of the result only the detail
//! finer than the model resolved is kept, over the model's own fill.

// The model's fill is the guide rather than the start alone, because a patch search on its own fills a hole with
// whatever matches its rim best, which is how a patch fill continues the wrong thing; the model knows what goes where
// (the line of a kerb, the edge of a shadow) and only lacks the resolution to draw it. Blurred to the model's
// resolution, its term asks a patch to agree with the model about light and structure and leaves the grain free.
//
// Repetition, the failure a patch synthesis is known for — the same tuft of grass stamped across the hole — is held off
// by a reuse penalty: a source patch costs more for every target patch already matched to it, so the search spreads
// over the photograph rather than settling on its single best patch. Coherence is PatchMatch's own: a neighbour's match,
// shifted, is tried first, so the hole is copied in contiguous runs rather than pixel by pixel.
//
// The votes average at the coarse levels, which is what lets the iterations converge, and at the photograph's
// resolution each pixel takes the one patch over it that matched best, because an average of forty-nine patches is
// the very smoothing the refinement exists to undo.
//
// In the family rather than in `imaging`, beside the grain and the feather: nothing else here synthesises texture,
// and no new dependency, the whole of it is a few hundred lines of plain arithmetic over scoped threads.

use super::region::in_hole;

/// The patch's half side: 7 x 7 patches.
const RADIUS: usize = 3;

/// The reduction a region must have been brought to the model's square by before it is refined: below it, the model
/// saw the crop at almost its own resolution, and there is nothing a patch could add that the model left out.
pub(crate) const FROM_REDUCTION: f32 = 1.5;

/// The most levels below the photograph's resolution the pyramid has: three, an eighth of it.
const MOST_LEVELS: usize = 3;

/// The fewest source patches a level must offer before anything is refined: a hole that leaves the crop almost no
/// known photograph is left as the model filled it.
const FEWEST_SOURCES: usize = 256;

/// The weight of the guide's term against the patch's own.
const GUIDE: f32 = 1.0;

/// What a source patch costs for each target patch beyond the first already matched to it, in the patch distance's
/// own units (a sum of squared differences in the unit range over the patch's 147 values, each term counted).
const REUSE: f32 = 0.02;

/// The expectation-maximisation rounds at the coarsest level, and at each finer one.
const ROUNDS: (usize, usize) = (4, 2);

/// How many bands of rows a PatchMatch pass is split into, each on a thread of its own.
const BANDS: usize = 16;

/// A pixel in the unit range.
type Pixel = [f32; 3];

/// One level of the pyramid.
struct Level {
    width: usize,
    height: usize,
    /// The photograph with the current estimate of the hole: what a target patch is compared as.
    image: Vec<Pixel>,
    /// The photograph with the model's fill, blurred to the detail the model resolved: the guide.
    guide: Vec<Pixel>,
    /// Whether each pixel is in the hole.
    hole: Vec<bool>,
    /// Whether a patch is centred at each pixel that overlaps the hole: the targets.
    target: Vec<bool>,
    /// Whether each pixel is the centre of a patch wholly of known photograph inside the crop: the sources.
    source: Vec<bool>,
    /// The sources, as indices.
    sources: Vec<u32>,
}

impl Level {
    /// The level holding `image`, a `width` x `height` composite whose `hole` is marked, its guide blurred by `sigma`.
    fn new(width: usize, height: usize, image: Vec<Pixel>, hole: Vec<bool>, sigma: f32) -> Self {
        let near = window_counts(&hole, width, height, RADIUS);
        let target: Vec<bool> = near.iter().map(|count| *count > 0).collect();

        let inner = |index: usize| {
            let (x, y) = (index % width, index / width);
            (RADIUS..width.saturating_sub(RADIUS)).contains(&x) && (RADIUS..height.saturating_sub(RADIUS)).contains(&y)
        };
        let source: Vec<bool> = near.iter().enumerate().map(|(index, count)| *count == 0 && inner(index)).collect();
        let sources = source.iter().enumerate().filter(|(_, source)| **source).map(|(index, _)| index as u32).collect();
        let guide = blurred(&image, width, height, sigma);

        Self { width, height, image, guide, hole, target, source, sources }
    }

    /// The level below this one, at half its resolution: each pixel the mean of the up to four under it, in the hole
    /// where any of them is. Its guide is blurred by `sigma`, at its own resolution.
    fn reduced(width: usize, height: usize, image: &[Pixel], hole: &[bool], sigma: f32) -> Self {
        let (half_width, half_height) = (width.div_ceil(2), height.div_ceil(2));
        let mut pixels = vec![[0.0; 3]; half_width * half_height];
        let mut holes = vec![false; half_width * half_height];

        for y in 0..half_height {
            for x in 0..half_width {
                let (mut sum, mut count, mut any) = ([0.0_f32; 3], 0.0_f32, false);

                for sy in 2 * y..(2 * y + 2).min(height) {
                    for sx in 2 * x..(2 * x + 2).min(width) {
                        let index = sy * width + sx;
                        sum.iter_mut().zip(image[index]).for_each(|(sum, value)| *sum += value);
                        count += 1.0;
                        any |= hole[index];
                    }
                }

                pixels[y * half_width + x] = sum.map(|sum| sum / count);
                holes[y * half_width + x] = any;
            }
        }

        Self::new(half_width, half_height, pixels, holes, sigma)
    }

    /// The patch distance between the target centred at `target` and the source centred at `source`, `uses` times
    /// already matched, given up as soon as it reaches `best`.
    fn distance(&self, target: usize, source: usize, uses: u16, best: f32) -> f32 {
        let (width, height) = (self.width as isize, self.height as isize);
        let (tx, ty) = ((target % self.width) as isize, (target / self.width) as isize);
        let (sx, sy) = ((source % self.width) as isize, (source / self.width) as isize);
        let radius = RADIUS as isize;
        let mut sum = REUSE * f32::from(uses.saturating_sub(1));

        for dy in -radius..=radius {
            let row = ty + dy;
            if !(0..height).contains(&row) {
                continue;
            }

            for dx in -radius..=radius {
                let column = tx + dx;
                if !(0..width).contains(&column) {
                    continue;
                }

                // The source patch is wholly inside the crop: a source is never within the radius of its edge.
                let (at, from) = ((row * width + column) as usize, ((sy + dy) * width + sx + dx) as usize);
                let (own, guide) = (squared(self.image[at], self.image[from]), squared(self.guide[at], self.guide[from]));
                sum += GUIDE.mul_add(guide, own);
            }

            if sum >= best {
                return sum;
            }
        }

        sum
    }
}

/// The squared distance between two pixels.
fn squared(a: Pixel, b: Pixel) -> f32 {
    a.iter().zip(b).map(|(a, b)| (a - b) * (a - b)).sum()
}

/// How many of `flags` are set in the `2 * radius + 1` square around each pixel of a `width` x `height` plane, the
/// square clipped to the plane: a summed-area table, so the radius costs nothing.
fn window_counts(flags: &[bool], width: usize, height: usize, radius: usize) -> Vec<u32> {
    let stride = width + 1;
    let mut table = vec![0_u32; stride * (height + 1)];

    for y in 0..height {
        let mut across = 0;
        for x in 0..width {
            across += u32::from(flags[y * width + x]);
            table[(y + 1) * stride + x + 1] = table[y * stride + x + 1] + across;
        }
    }

    (0..width * height)
        .map(|index| {
            let (x, y) = (index % width, index / width);
            let (left, top) = (x.saturating_sub(radius), y.saturating_sub(radius));
            let (right, bottom) = ((x + radius + 1).min(width), (y + radius + 1).min(height));

            table[bottom * stride + right] + table[top * stride + left]
                - table[top * stride + right]
                - table[bottom * stride + left]
        })
        .collect()
}

/// `image`, a `width` x `height` plane of pixels, blurred by a Gaussian of `sigma`, the edges clamped. Under a quarter
/// of a pixel it is the plane itself.
fn blurred(image: &[Pixel], width: usize, height: usize, sigma: f32) -> Vec<Pixel> {
    if sigma < 0.25 {
        return image.to_vec();
    }

    let reach = (3.0 * sigma).ceil() as isize;
    let weights: Vec<f32> = (-reach..=reach).map(|offset| (-((offset * offset) as f32) / (2.0 * sigma * sigma)).exp()).collect();
    let total: f32 = weights.iter().sum();
    let weights: Vec<f32> = weights.iter().map(|weight| weight / total).collect();

    let pass = |from: &[Pixel], along_rows: bool| {
        let mut out = vec![[0.0; 3]; width * height];
        imaging::rows::for_each_row(&mut out, width, height, |y, row| {
            for (x, value) in row.iter_mut().enumerate() {
                let mut sum = [0.0_f32; 3];
                for (tap, weight) in weights.iter().enumerate() {
                    let offset = tap as isize - reach;
                    let index = if along_rows {
                        y * width + (x as isize + offset).clamp(0, width as isize - 1) as usize
                    } else {
                        (y as isize + offset).clamp(0, height as isize - 1) as usize * width + x
                    };
                    sum.iter_mut().zip(from[index]).for_each(|(sum, value)| *sum = weight.mul_add(value, *sum));
                }
                *value = sum;
            }
        });
        out
    };

    pass(&pass(image, true), false)
}

/// A small generator for the random search: xorshift, seeded per band and round so a refinement is the same image
/// every time it is run.
struct Random(u64);

impl Random {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A whole number in `-reach..=reach`.
    fn within(&mut self, reach: usize) -> isize {
        (self.next() % (2 * reach as u64 + 1)) as isize - reach as isize
    }
}

/// The nearest-neighbour field of one level: for each target, the source it is matched to and what the match costs.
struct Field {
    matched: Vec<u32>,
    cost: Vec<f32>,
}

impl Field {
    /// Each target of `level` matched to a source drawn at random.
    fn random(level: &Level, seed: u64) -> Self {
        let mut random = Random::new(seed);
        let matched = level
            .target
            .iter()
            .map(|target| if *target { level.sources[random.next() as usize % level.sources.len()] } else { 0 })
            .collect();

        Self::costed(level, matched)
    }

    /// `coarse`'s field over the level below `level`, carried up: each target takes its coarse pixel's match, doubled
    /// and offset as it is, or a random source where that lands on none.
    fn carried(coarse: &Field, below: &Level, level: &Level, seed: u64) -> Self {
        let mut random = Random::new(seed);
        let matched = (0..level.width * level.height)
            .map(|index| {
                if !level.target[index] {
                    return 0;
                }

                let (x, y) = (index % level.width, index / level.width);
                let at = (y / 2).min(below.height - 1) * below.width + (x / 2).min(below.width - 1);
                let from = coarse.matched[at] as usize;
                let (fx, fy) = (2 * (from % below.width) + x % 2, 2 * (from / below.width) + y % 2);
                let candidate = fy.min(level.height - 1) * level.width + fx.min(level.width - 1);

                if below.target[at] && level.source[candidate] {
                    candidate as u32
                } else {
                    level.sources[random.next() as usize % level.sources.len()]
                }
            })
            .collect();

        Self::costed(level, matched)
    }

    /// `matched`, with each target's cost against `level` as it now is, uses not counted.
    fn costed(level: &Level, matched: Vec<u32>) -> Self {
        let mut cost = vec![0.0_f32; matched.len()];
        imaging::rows::for_each_row(&mut cost, level.width, level.height, |y, row| {
            for (x, cost) in row.iter_mut().enumerate() {
                let index = y * level.width + x;
                if level.target[index] {
                    *cost = level.distance(index, matched[index] as usize, 0, f32::INFINITY);
                }
            }
        });

        Self { matched, cost }
    }

    /// How many targets each source is matched to.
    fn uses(&self, level: &Level) -> Vec<u16> {
        let mut uses = vec![0_u16; self.matched.len()];
        for (index, source) in self.matched.iter().enumerate() {
            if level.target[index] {
                uses[*source as usize] = uses[*source as usize].saturating_add(1);
            }
        }

        uses
    }

    /// One PatchMatch pass over `level`: propagation from the left and above (from the right and below where
    /// `reverse`), then a random search at halving radii, each band of rows on its own thread.
    fn pass(&mut self, level: &Level, reverse: bool, seed: u64) {
        // Each band scans its own rows in order and propagates within them; a band's first row reads the row above it
        // as it stood before the pass, which is what lets the bands run at once.
        let uses = self.uses(level);
        let before = self.matched.clone();
        let width = level.width;
        // A fixed number of bands rather than one per core, so the result does not depend on the machine.
        let rows_per_band = level.height.div_ceil(BANDS).max(8);

        std::thread::scope(|scope| {
            let bands = self.matched.chunks_mut(rows_per_band * width).zip(self.cost.chunks_mut(rows_per_band * width));

            for (band, (matched, cost)) in bands.enumerate() {
                let (before, uses) = (&before, &uses);

                scope.spawn(move || {
                    let first = band * rows_per_band;
                    let rows = matched.len() / width;
                    let mut random = Random::new(seed ^ ((band as u64) << 40));

                    for local in 0..rows {
                        let local = if reverse { rows - 1 - local } else { local };
                        for column in 0..width {
                            let column = if reverse { width - 1 - column } else { column };
                            improve(level, (column, first + local), (first, rows), matched, cost, before, uses, reverse, &mut random);
                        }
                    }
                });
            }
        });
    }
}

/// Improves the match of the target at `(x, y)` in a band of `rows` rows from `first`, whose matches and costs are
/// `matched` and `cost`: from its neighbours' matches, shifted, then from a random search.
#[expect(clippy::too_many_arguments, reason = "one band's state, borrowed apart so the bands can run at once")]
fn improve(
    level: &Level,
    (x, y): (usize, usize),
    (first, rows): (usize, usize),
    matched: &mut [u32],
    cost: &mut [f32],
    before: &[u32],
    uses: &[u16],
    reverse: bool,
    random: &mut Random,
) {
    let width = level.width;
    let index = y * width + x;
    if !level.target[index] {
        return;
    }

    let local = (y - first) * width + x;
    let (mut best, mut best_cost) = (matched[local] as usize, cost[local]);
    let try_source = |candidate: usize, best: &mut usize, best_cost: &mut f32| {
        if candidate != *best && level.source[candidate] {
            let candidate_cost = level.distance(index, candidate, uses[candidate], *best_cost);
            if candidate_cost < *best_cost {
                (*best, *best_cost) = (candidate, candidate_cost);
            }
        }
    };

    // Propagation: the neighbour already visited this pass, its match moved by the step between the two.
    let step: isize = if reverse { 1 } else { -1 };
    let neighbours = [(x as isize + step, y as isize, (-step, 0)), (x as isize, y as isize + step, (0, -step))];
    for (nx, ny, (sx, sy)) in neighbours {
        if !(0..width as isize).contains(&nx) || !(0..level.height as isize).contains(&ny) {
            continue;
        }

        let (nx, ny) = (nx as usize, ny as usize);
        let neighbour = ny * width + nx;
        if !level.target[neighbour] {
            continue;
        }

        let from = if (first..first + rows).contains(&ny) { matched[(ny - first) * width + nx] } else { before[neighbour] };
        let (fx, fy) = ((from as usize % width) as isize + sx, (from as usize / width) as isize + sy);
        if (0..width as isize).contains(&fx) && (0..level.height as isize).contains(&fy) {
            try_source(fy as usize * width + fx as usize, &mut best, &mut best_cost);
        }
    }

    // Random search, around the best so far, over the whole crop and then half as far each time.
    let mut reach = width.max(level.height);
    while reach >= 1 {
        let (bx, by) = ((best % width) as isize, (best / width) as isize);
        let cx = (bx + random.within(reach)).clamp(0, width as isize - 1) as usize;
        let cy = (by + random.within(reach)).clamp(0, level.height as isize - 1) as usize;
        try_source(cy * width + cx, &mut best, &mut best_cost);
        reach /= 2;
    }

    (matched[local], cost[local]) = (best as u32, best_cost);
}

/// The hole of `level` voted from `field`: at each hole pixel, the patches over it each give the source pixel at the
/// same place in their match. Averaged, weighted by how well each matched, or, where `best`, the one that matched best.
fn vote(level: &Level, field: &Field, best: bool) -> Vec<Pixel> {
    // Wexler's weight, exp(-cost / 2σ²), with σ² the costs' third quartile: a patch matched as well as most counts
    // fully, and a poor match counts for little.
    let mut costs: Vec<f32> = field.cost.iter().zip(&level.target).filter(|(_, target)| **target).map(|(cost, _)| *cost).collect();
    let quartile = costs.len() * 3 / 4;
    let spread = if costs.is_empty() { 1.0 } else { *costs.select_nth_unstable_by(quartile, f32::total_cmp).1 };
    let spread = 2.0 * spread.max(1e-6);

    let (width, height) = (level.width, level.height);
    let radius = RADIUS as isize;
    let mut out = level.image.clone();

    imaging::rows::for_each_row(&mut out, width, height, |y, row| {
        for (x, value) in row.iter_mut().enumerate() {
            if !level.hole[y * width + x] {
                continue;
            }

            let (mut sum, mut total, mut chosen) = ([0.0_f32; 3], 0.0_f32, (f32::INFINITY, *value));
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let (px, py) = (x as isize - dx, y as isize - dy);
                    if !(0..width as isize).contains(&px) || !(0..height as isize).contains(&py) {
                        continue;
                    }

                    let centre = py as usize * width + px as usize;
                    if !level.target[centre] {
                        continue;
                    }

                    let source = field.matched[centre] as usize;
                    let from = ((source / width) as isize + dy) as usize * width + ((source % width) as isize + dx) as usize;
                    let pixel = level.image[from];
                    let cost = field.cost[centre];

                    if best {
                        if cost < chosen.0 {
                            chosen = (cost, pixel);
                        }
                    } else {
                        let weight = (-cost / spread).exp().max(1e-12);
                        sum.iter_mut().zip(pixel).for_each(|(sum, value)| *sum = weight.mul_add(value, *sum));
                        total += weight;
                    }
                }
            }

            *value = if best {
                chosen.1
            } else if total > 0.0 {
                sum.map(|sum| sum / total)
            } else {
                *value
            };
        }
    });

    out
}

/// Refines the hole of `pixels`, a `width` x `height` composite (the photograph, with the model's fill over the hole
/// its `squared` distances describe), which the model saw reduced by `reduction`. Answers whether it did: not where
/// the reduction is under [`FROM_REDUCTION`], nor where the crop leaves too little known photograph to copy from.
pub(crate) fn refined(pixels: &mut [Pixel], width: usize, height: usize, squared: &[u16], reduction: f32) -> bool {
    if reduction < FROM_REDUCTION || width <= 4 * RADIUS || height <= 4 * RADIUS {
        return false;
    }

    // As many levels as halve the photograph towards the model's own resolution, at most three: the coarsest is near
    // the resolution the model filled at, where its fill is as good as it gets and only the texture is to be found.
    let mut count = (reduction.log2().floor() as usize).clamp(1, MOST_LEVELS);
    while count > 1 && (width >> count <= 4 * RADIUS || height >> count <= 4 * RADIUS) {
        count -= 1;
    }

    // The guide is blurred at each level to the detail the model resolved there: half its reduction, in that level's
    // pixels.
    let sigma = |level: usize| 0.5 * reduction / (1 << level) as f32;
    let hole: Vec<bool> = squared.iter().map(|squared| in_hole(*squared)).collect();
    let mut levels = vec![Level::new(width, height, pixels.to_vec(), hole, sigma(0))];
    for index in 1..=count {
        let finer = &levels[index - 1];
        let level = Level::reduced(finer.width, finer.height, &finer.image, &finer.hole, sigma(index));
        levels.push(level);
    }

    if levels.iter().any(|level| level.sources.len() < FEWEST_SOURCES) {
        return false;
    }

    // Coarsest first; each finer level starts from the coarser one's field, carried up, and its vote.
    let mut field: Option<Field> = None;
    for index in (0..=count).rev() {
        let seed = index as u64 * 1_000;
        let mut current = match &field {
            None => Field::random(&levels[index], seed),
            Some(coarse) => {
                let carried = Field::carried(coarse, &levels[index + 1], &levels[index], seed);
                levels[index].image = vote(&levels[index], &carried, false);
                Field::costed(&levels[index], carried.matched)
            }
        };

        let rounds = if index == count { ROUNDS.0 } else { ROUNDS.1 };
        for round in 0..rounds {
            let level = &levels[index];
            current.pass(level, round % 2 == 1, seed + 2 * round as u64);
            current.pass(level, round % 2 == 0, seed + 2 * round as u64 + 1);

            // The last vote at the photograph's resolution takes each pixel's best patch; every other averages.
            let last = index == 0 && round + 1 == rounds;
            let voted = vote(level, &current, last);
            levels[index].image = voted;

            if !last {
                current = Field::costed(&levels[index], current.matched);
            }
        }

        field = Some(current);
    }

    // Only the synthesis's detail is kept: its own blur taken away and the model's put in its place, so the light,
    // the colour and the structure are the model's to the detail the model resolved, and what the patches add is the
    // finer texture alone. A patch matched by its texture can come from a stone of another shade; without this, it
    // brings the shade with it, and the hole is mottled with patches of the wrong colour.
    let finest = &levels[0];
    let own = blurred(&finest.image, width, height, sigma(0));
    for (index, pixel) in pixels.iter_mut().enumerate() {
        if finest.hole[index] {
            let (refined, own, guide) = (finest.image[index], own[index], finest.guide[index]);
            *pixel = std::array::from_fn(|channel| refined[channel] - own[channel] + guide[channel]);
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::remove::region::HOLE;

    /// A `width` x `height` composite of stripes four pixels wide, alternately dark and light, with a hole of radius
    /// `radius` at the centre filled by `fill`, and its distances.
    fn striped(width: usize, height: usize, radius: f32, fill: Pixel) -> (Vec<Pixel>, Vec<u16>) {
        let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
        let mut pixels = Vec::new();
        let mut squared = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let distance = ((x as f32 - cx).hypot(y as f32 - cy) - radius).max(0.0);
                let distance = (distance * distance).min(((HOLE + 1) * (HOLE + 1)) as f32) as u16;
                squared.push(distance);
                pixels.push(if in_hole(distance) { fill } else if (x / 4) % 2 == 0 { [0.2; 3] } else { [0.8; 3] });
            }
        }

        (pixels, squared)
    }

    #[test]
    fn a_window_count_is_the_brute_force_one() {
        let (width, height) = (13, 9);
        let flags: Vec<bool> = (0..width * height).map(|index| index % 7 == 0 || index % 11 == 3).collect();
        let counts = window_counts(&flags, width, height, 2);

        for y in 0..height {
            for x in 0..width {
                let brute = (y.saturating_sub(2)..(y + 3).min(height))
                    .flat_map(|row| (x.saturating_sub(2)..(x + 3).min(width)).map(move |column| row * width + column))
                    .filter(|index| flags[*index])
                    .count();
                assert_eq!(counts[y * width + x] as usize, brute, "({x}, {y})");
            }
        }
    }

    #[test]
    fn a_blur_keeps_a_flat_plane_flat_and_its_mean() {
        let flat = vec![[0.25, 0.5, 0.75]; 40 * 30];
        let blurred = blurred(&flat, 40, 30, 2.0);

        assert!(blurred.iter().all(|pixel| pixel.iter().zip([0.25, 0.5, 0.75]).all(|(a, b)| (a - b).abs() < 1e-5)));
        assert_eq!(super::blurred(&flat, 40, 30, 0.1), flat, "a blur under a quarter of a pixel changed the plane");
    }

    #[test]
    fn stripes_guided_by_their_own_blur_are_continued_through_the_hole() {
        // The guide is the stripes' mean, as a model that could not resolve them would fill: the refinement draws the
        // stripes back, each pixel dark or light, in phase with the stripes around the hole.
        let (width, height) = (160, 120);
        let (mut pixels, squared) = striped(width, height, 12.0, [0.5; 3]);

        assert!(refined(&mut pixels, width, height, &squared, 4.0), "nothing was refined");

        let (expected, _) = striped(width, height, 12.0, [0.0; 3]);
        let truth = |index: usize| if (index % width / 4) % 2 == 0 { 0.2 } else { 0.8 };
        let inside: Vec<usize> = (0..width * height).filter(|index| in_hole(squared[*index])).collect();
        let error = inside.iter().map(|index| (pixels[*index][0] - truth(*index)).abs()).sum::<f32>() / inside.len() as f32;
        assert!(error < 0.1, "the stripes were not continued: mean error {error}");

        // Outside the hole nothing changed.
        for (index, (pixel, before)) in pixels.iter().zip(&expected).enumerate() {
            if !in_hole(squared[index]) {
                assert_eq!(pixel, before, "{index}");
            }
        }
    }

    #[test]
    fn a_region_the_model_saw_at_almost_its_own_resolution_is_not_refined() {
        let (mut pixels, squared) = striped(100, 100, 10.0, [0.5; 3]);
        let before = pixels.clone();

        assert!(!refined(&mut pixels, 100, 100, &squared, 1.2));
        assert_eq!(pixels, before);
    }

    #[test]
    fn a_hole_that_leaves_too_little_photograph_is_not_refined() {
        // The hole covers all but a rim of the crop.
        let (mut pixels, squared) = striped(60, 60, 26.0, [0.5; 3]);
        let before = pixels.clone();

        assert!(!refined(&mut pixels, 60, 60, &squared, 4.0));
        assert_eq!(pixels, before);
    }

    #[test]
    fn a_refinement_is_the_same_image_every_time() {
        let (mut first, squared) = striped(120, 90, 9.0, [0.5; 3]);
        let mut second = first.clone();

        refined(&mut first, 120, 90, &squared, 3.0);
        refined(&mut second, 120, 90, &squared, 3.0);
        assert_eq!(first, second);
    }
}
