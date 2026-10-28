//! From a MoGe-style point map to distances in metres: the shift along the optical axis and the focal length, recovered
//! by least squares against the image plane, then the model's metric scale.

// MoGe-2's graph returns points that are right up to a shift along z — the network predicts the shape of the scene,
// not where the camera stands — and a metric scale. The shift is recovered exactly as the model's own
// `recover_focal_shift` recovers it: project the points with a trial shift, fit the one focal length that best maps the
// projection onto the image plane (a closed form), and choose the shift whose fit leaves the least residual. One
// unknown, so the Levenberg-Marquardt is written out here rather than taken from a solver crate.
//
// The reference stops on SciPy's `ftol=1e-3`, a relative change in the residual; this stops on a relative change of
// the mean distance of 1e-3, after the step has been taken, which converges at least as far: Gauss-Newton on one
// unknown is quadratic near the minimum, so the shift is then good to about a part in a million.

/// Half the diagonal of a 36 x 24 mm frame, in millimetres: what a focal length in half-diagonals is multiplied by to
/// read as a 35 mm-equivalent one.
const HALF_DIAGONAL_35MM: f64 = 21.633;

/// How many rows and columns of the map the fit samples: 64 x 64, the reference's own.
const SAMPLES: usize = 64;

/// The fewest valid samples the fit is attempted on. One per sampled row: fewer is a photograph the model could place
/// almost nothing of, and a shift fitted to it would be a guess presented as a measurement.
const MIN_SAMPLES: usize = SAMPLES;

/// The relative change of the mean distance at which the fit stops.
const TOLERANCE: f64 = 1e-3;

/// The most steps the fit takes, which a well-posed problem never approaches.
const MAX_STEPS: usize = 100;

/// What a point map recovers to: distances in metres at the map's own resolution, and the focal length.
#[derive(Debug, Clone)]
pub(crate) struct Recovered {
    /// Distances in metres, row-major, every one finite and positive.
    pub(crate) metres: Vec<f32>,
    /// The shift along z the fit chose, in the map's own units.
    pub(crate) shift: f64,
    /// The focal length, in half-diagonals of the frame the map covers.
    pub(crate) focal: f64,
}

impl Recovered {
    /// The focal length as a 35 mm-equivalent one, in millimetres, or `None` where the fit's is not a positive number.
    ///
    /// Measured against the diagonal of the frame the map covers, which is the photograph's when the photograph filled
    /// that frame.
    pub(crate) fn focal_35mm(&self) -> Option<f32> {
        (self.focal.is_finite() && self.focal > 0.0).then_some((self.focal * HALF_DIAGONAL_35MM) as f32)
    }
}

/// Why a point map has no distances to recover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unmeasurable {
    /// Too few of the sampled points were valid.
    TooFewPoints {
        /// How many of the sampled points were valid.
        valid: usize,
    },
    /// The model's metric scale is not a positive, finite number, so there is no reading of the points in metres.
    NoMetricScale,
}

/// A point map as the graph returns it, `width` by `height`.
pub(crate) struct PointMap<'a> {
    /// `x, y, z` per pixel, row-major.
    pub(crate) points: &'a [f32],
    /// Per pixel, valid where above one half.
    pub(crate) mask: &'a [f32],
    /// What the points are multiplied by to read in metres.
    pub(crate) metric_scale: f32,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

/// One sampled point: its image-plane position and the point the model placed there.
#[derive(Debug, Clone, Copy)]
struct Sample {
    u: f64,
    v: f64,
    x: f64,
    y: f64,
    z: f64,
}

/// The distances `map` stands for, in metres, and the focal length it implies.
///
/// # Errors
///
/// [`Unmeasurable`] where fewer than 64 of the sampled points are valid, or the metric scale is not a positive number;
/// the two are told apart, because a scale that is not a number says nothing about how much of the photograph was placed.
pub(crate) fn recover(map: &PointMap<'_>) -> Result<Recovered, Unmeasurable> {
    let samples = samples(map);

    if !(map.metric_scale.is_finite() && map.metric_scale > 0.0) {
        return Err(Unmeasurable::NoMetricScale);
    }

    if samples.len() < MIN_SAMPLES {
        return Err(Unmeasurable::TooFewPoints { valid: samples.len() });
    }

    let shift = fit_shift(&samples);
    let (focal, _) = focal_and_cost(&samples, shift);
    let scale = f64::from(map.metric_scale);

    // Every pixel the model placed, at its distance; every other one, and any the shift left behind the camera, at
    // `NAN` until the farthest placed distance is known.
    let mut metres: Vec<f32> = (0..map.width * map.height)
        .map(|pixel| {
            let z = f64::from(map.points[pixel * 3 + 2]) + shift;
            let placed = map.mask[pixel] > 0.5 && z.is_finite() && z > 0.0;

            if placed { (z * scale) as f32 } else { f32::NAN }
        })
        .collect();

    // The sky, mostly: what the model could not place is taken to be as far as the farthest thing it could, so it
    // blurs as the background rather than as a hole.
    let farthest = metres.iter().copied().filter(|distance| distance.is_finite()).fold(0.0_f32, f32::max);
    for distance in &mut metres {
        if !distance.is_finite() || *distance <= 0.0 {
            *distance = farthest;
        }
    }

    Ok(Recovered { metres, shift, focal })
}

/// The valid points of a 64 x 64 nearest-neighbour subsample of `map`, each beside its image-plane position.
fn samples(map: &PointMap<'_>) -> Vec<Sample> {
    // MoGe-2's normalised image plane: the frame's centre at the origin and its corners at a distance of one, so a
    // focal length comes out in half-diagonals. Pixel centres, as the reference's `view_plane_uv` places them.
    let diagonal = (map.width as f64).hypot(map.height as f64);
    let plane = |index: usize, size: usize| (2.0 * index as f64 + 1.0 - size as f64) / diagonal;

    // `linspace(0, size - 1, 64).round()`, which never lands on a half for the sizes the graphs have.
    let spaced = |size: usize| -> Vec<usize> {
        (0..SAMPLES)
            .map(|k| (k as f64 * (size - 1) as f64 / (SAMPLES - 1) as f64).round() as usize)
            .collect()
    };
    let (rows, columns) = (spaced(map.height), spaced(map.width));

    rows.iter()
        .flat_map(|&row| columns.iter().map(move |&column| (row, column)))
        .filter_map(|(row, column)| {
            let pixel = row * map.width + column;
            let [x, y, z] = [0, 1, 2].map(|axis| f64::from(map.points[pixel * 3 + axis]));

            (map.mask[pixel] > 0.5 && x.is_finite() && y.is_finite() && z.is_finite()).then(|| Sample {
                u: plane(column, map.width),
                v: plane(row, map.height),
                x,
                y,
                z,
            })
        })
        .collect()
}

/// The focal length that best maps `samples` projected with `shift` onto the image plane, and the residual it leaves.
///
/// The closed form: for the projection `p = (x, y) / (z + shift)`, the `f` minimising `sum |f p - uv|^2` is
/// `sum(p . uv) / sum(p . p)`.
fn focal_and_cost(samples: &[Sample], shift: f64) -> (f64, f64) {
    let (mut along, mut power) = (0.0, 0.0);
    for sample in samples {
        let depth = sample.z + shift;
        let (px, py) = (sample.x / depth, sample.y / depth);
        along += px * sample.u + py * sample.v;
        power += px * px + py * py;
    }

    let focal = along / power;
    let cost = samples
        .iter()
        .map(|sample| {
            let depth = sample.z + shift;
            (focal * sample.x / depth - sample.u).powi(2) + (focal * sample.y / depth - sample.v).powi(2)
        })
        .sum();

    (focal, cost)
}

/// The gradient and the Gauss-Newton curvature of the residual at `shift`, the focal length refitted with it.
fn gradient_and_curvature(samples: &[Sample], shift: f64) -> (f64, f64) {
    // The focal length is the closed form above, so it moves with the shift: `f = a / b`, and its derivative is
    // carried into each residual's.
    let (mut a, mut b, mut da, mut db) = (0.0, 0.0, 0.0, 0.0);
    for sample in samples {
        let depth = sample.z + shift;
        let (px, py) = (sample.x / depth, sample.y / depth);
        // d/ds of x / (z + s) is -x / (z + s)^2, which is -p / (z + s).
        let (dpx, dpy) = (-px / depth, -py / depth);
        a += px * sample.u + py * sample.v;
        b += px * px + py * py;
        da += dpx * sample.u + dpy * sample.v;
        db += 2.0 * (px * dpx + py * dpy);
    }

    let focal = a / b;
    let dfocal = (da * b - a * db) / (b * b);

    let (mut gradient, mut curvature) = (0.0, 0.0);
    for sample in samples {
        let depth = sample.z + shift;
        let (px, py) = (sample.x / depth, sample.y / depth);
        let (dpx, dpy) = (-px / depth, -py / depth);
        let (rx, ry) = (focal * px - sample.u, focal * py - sample.v);
        let (jx, jy) = (dfocal * px + focal * dpx, dfocal * py + focal * dpy);
        gradient += jx * rx + jy * ry;
        curvature += jx * jx + jy * jy;
    }

    (gradient, curvature)
}

/// The shift along z that leaves the least residual, every sample kept in front of the camera.
fn fit_shift(samples: &[Sample]) -> f64 {
    let nearest = samples.iter().map(|sample| sample.z).fold(f64::INFINITY, f64::min);
    let farthest = samples.iter().map(|sample| sample.z).fold(f64::NEG_INFINITY, f64::max);
    let mean = samples.iter().map(|sample| sample.z).sum::<f64>() / samples.len() as f64;
    let in_front = |shift: f64| nearest + shift > 0.0;

    // From zero, as the reference starts; from just in front of the nearest sample where zero would put it behind
    // the camera, which the reference never meets because MoGe-2's points have positive z.
    let mut shift = if in_front(0.0) { 0.0 } else { -nearest + TOLERANCE * (farthest - nearest).max(f64::EPSILON) };
    let mut damping = 1e-3;
    let (_, mut cost) = focal_and_cost(samples, shift);

    for _ in 0..MAX_STEPS {
        let (gradient, curvature) = gradient_and_curvature(samples, shift);
        if curvature <= 0.0 || !curvature.is_finite() {
            break;
        }

        // The Gauss-Newton step, which is what says how far from the minimum the fit is: the damped one shrinks with
        // the damping, so a fit whose damping had been raised to get past a refused step would read as arrived while
        // still a long way off.
        let newton = -gradient / curvature;
        let step = newton / (1.0 + damping);
        let candidate = shift + step;
        let (_, candidate_cost) = focal_and_cost(samples, candidate);

        // A step that would put a sample behind the camera, or that does not lower the residual, is refused and the
        // next one damped harder: the Levenberg-Marquardt half of the method.
        if !in_front(candidate) || candidate_cost.is_nan() || candidate_cost > cost {
            damping *= 10.0;
            if damping > 1e12 {
                break;
            }
            continue;
        }

        shift = candidate;
        cost = candidate_cost;
        damping = (damping * 0.1).max(1e-12);

        if newton.abs() <= TOLERANCE * (mean + shift).abs() {
            break;
        }
    }

    shift
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A point map of a scene seen through a lens of focal `focal` (in half-diagonals), with distances `distance(x,
    /// y)`, as MoGe-2 would return it: right up to `-shift` along z, in units `1 / scale` of a metre.
    pub(crate) fn synthetic(
        width: usize,
        height: usize,
        focal: f64,
        shift: f64,
        scale: f64,
        distance: impl Fn(usize, usize) -> f64,
    ) -> Vec<f32> {
        let diagonal = (width as f64).hypot(height as f64);
        let mut points = Vec::with_capacity(width * height * 3);

        for y in 0..height {
            for x in 0..width {
                let z = distance(x, y) / scale;
                let u = (2.0 * x as f64 + 1.0 - width as f64) / diagonal;
                let v = (2.0 * y as f64 + 1.0 - height as f64) / diagonal;
                points.extend([(u * z / focal) as f32, (v * z / focal) as f32, (z - shift) as f32]);
            }
        }

        points
    }

    /// A scene with depth to fit: a slope from two to eight metres across, and a bump.
    fn scene(width: usize, height: usize) -> impl Fn(usize, usize) -> f64 {
        move |x, y| 2.0 + 6.0 * x as f64 / width as f64 + (y as f64 / height as f64 * 3.0).sin()
    }

    #[test]
    fn a_known_focal_length_and_shift_are_recovered_within_a_tenth_of_a_percent() {
        for (width, height) in [(160, 128), (128, 160), (300, 200)] {
            for (focal, shift) in [(1.2, 0.8), (0.9, -0.5), (2.5, 3.0)] {
                let points = synthetic(width, height, focal, shift, 2.0, scene(width, height));
                let mask = vec![1.0; width * height];
                let map = PointMap { points: &points, mask: &mask, metric_scale: 2.0, width, height };

                let recovered = recover(&map).expect("a scene with depth is measurable");

                assert!(
                    (recovered.shift - shift).abs() <= 1e-3 * shift.abs(),
                    "{width}x{height}: shift {shift} came back as {}",
                    recovered.shift
                );
                assert!(
                    (recovered.focal - focal).abs() <= 1e-3 * focal,
                    "{width}x{height}: focal {focal} came back as {}",
                    recovered.focal
                );

                // And the distances are the scene's, in metres.
                let expected = scene(width, height)(width / 3, height / 2);
                let found = f64::from(recovered.metres[(height / 2) * width + width / 3]);
                assert!((found - expected).abs() <= 1e-3 * expected, "{expected} m came back as {found} m");
            }
        }
    }

    #[test]
    fn the_35mm_focal_length_is_the_half_diagonal_scaled() {
        let recovered = Recovered { metres: Vec::new(), shift: 0.0, focal: 1.664 };

        // 1.664 half-diagonals is a 36 mm lens on a full frame.
        assert!((recovered.focal_35mm().unwrap() - 36.0).abs() < 0.01);
        assert_eq!(Recovered { focal: -1.0, ..recovered.clone() }.focal_35mm(), None);
        assert_eq!(Recovered { focal: f64::NAN, ..recovered }.focal_35mm(), None);
    }

    #[test]
    fn what_the_model_could_not_place_is_as_far_as_the_farthest_it_could() {
        let (width, height) = (96, 64);
        let points = synthetic(width, height, 1.0, 0.3, 1.0, scene(width, height));
        // The top quarter is sky.
        let mask: Vec<f32> =
            (0..width * height).map(|pixel| if pixel < width * height / 4 { 0.0 } else { 1.0 }).collect();
        let map = PointMap { points: &points, mask: &mask, metric_scale: 1.0, width, height };

        let recovered = recover(&map).expect("three quarters of a scene is measurable");
        let farthest = recovered.metres[width * height / 4..].iter().copied().fold(0.0_f32, f32::max);

        assert!(recovered.metres[..width * height / 4].iter().all(|distance| *distance == farthest));
        assert!(recovered.metres.iter().all(|distance| distance.is_finite() && *distance > 0.0));
    }

    #[test]
    fn a_map_the_model_placed_almost_nothing_of_is_refused() {
        let (width, height) = (128, 128);
        let points = synthetic(width, height, 1.0, 0.0, 1.0, scene(width, height));
        let mask = vec![0.0; width * height];

        let refused = recover(&PointMap { points: &points, mask: &mask, metric_scale: 1.0, width, height });
        assert_eq!(refused.unwrap_err(), Unmeasurable::TooFewPoints { valid: 0 });

        // One sampled row is the floor: 63 valid samples are refused.
        let mut mask = vec![0.0; width * height];
        mask[..63 * 2].fill(1.0);
        assert!(recover(&PointMap { points: &points, mask: &mask, metric_scale: 1.0, width, height }).is_err());

        let unscaled = vec![1.0; width * height];
        let map = PointMap { points: &points, mask: &unscaled, metric_scale: 0.0, width, height };
        assert!(recover(&map).is_err(), "a map with no metric scale was read in metres");
    }

    #[test]
    fn a_metric_scale_that_is_not_a_positive_number_is_not_reported_as_too_few_points() {
        // A fully placed map, so the only thing wrong with it is the scale: reporting that as "0 of the points" would
        // send a reader looking for a sky that is not there.
        let (width, height) = (128, 128);
        let points = synthetic(width, height, 1.0, 0.0, 1.0, scene(width, height));
        let mask = vec![1.0; width * height];

        for metric_scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let map = PointMap { points: &points, mask: &mask, metric_scale, width, height };

            assert_eq!(recover(&map).unwrap_err(), Unmeasurable::NoMetricScale, "a scale of {metric_scale}");
        }
    }

    #[test]
    fn a_shift_is_never_one_that_puts_a_sample_behind_the_camera() {
        // Points whose z is negative at zero shift: the fit starts in front of the nearest instead, and ends at the
        // true shift.
        let (width, height) = (128, 96);
        let points = synthetic(width, height, 1.1, 4.0, 1.0, scene(width, height));
        let mask = vec![1.0; width * height];

        let recovered = recover(&PointMap { points: &points, mask: &mask, metric_scale: 1.0, width, height })
            .expect("a scene with depth is measurable");

        assert!((recovered.shift - 4.0).abs() < 4e-3, "{}", recovered.shift);
    }

    #[test]
    fn a_fit_that_damping_slowed_goes_on_to_converge() {
        // Started just in front of the nearest sample and a long way from the true shift, so the first steps overshoot
        // behind the camera and are refused with heavier damping. The fit stops on the undamped step, which says how far
        // it is from the minimum; the damped one it takes is small whenever the damping is, and would read as arrival.
        // No scene tried made the damped criterion stop early (the refused steps are cheap and the damping falls a
        // tenth per accepted one), so this pins the tolerance rather than reproducing a stall.
        let (width, height) = (128, 96);
        let points = synthetic(width, height, 1.1, 40.0, 1.0, scene(width, height));
        let mask = vec![1.0; width * height];

        let recovered = recover(&PointMap { points: &points, mask: &mask, metric_scale: 1.0, width, height })
            .expect("a scene with depth is measurable");

        assert!((recovered.shift - 40.0).abs() < 40.0 * 1e-4, "{}", recovered.shift);
    }
}
