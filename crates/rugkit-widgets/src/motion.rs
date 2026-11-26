//! Springs and animated values.
//!
//! Every visual property in rugkit (position, size, colour, radius, elevation, scale) is an
//! [`Animated`] value. Setting a new target doesn't jump: the value springs towards the target
//! and keeps its current velocity. That makes animations interruptible. Click a button five
//! times quickly and it flows instead of restarting.

use rugkit_render::{Color, Rect};

/// A damped spring, described the way designers think about it.
///
/// `response` is roughly how long one oscillation takes, in seconds (lower is snappier).
/// `damping` is the damping ratio: `1.0` settles without overshoot, lower values bounce.
///
/// ```
/// use rugkit_widgets::Spring;
///
/// let gentle = Spring::new(0.5, 1.0);
/// assert!(Spring::BOUNCY.damping < gentle.damping);
/// ```
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Spring {
    /// Period of the undamped oscillation, in seconds.
    pub response: f64,
    /// Damping ratio. `< 1` bounces, `1` is critically damped, `> 1` is sluggish.
    pub damping: f64,
}

impl Spring {
    /// rugkit's signature playful spring: quick, with a visible overshoot. Default for layout
    /// and scale.
    pub const BOUNCY: Self = Self::new(0.45, 0.62);
    /// Quick and barely overshooting. Default for colours and hover states.
    pub const SNAPPY: Self = Self::new(0.28, 0.86);
    /// Calm, no overshoot.
    pub const SMOOTH: Self = Self::new(0.5, 1.0);
    /// What colours use when the OS asks for reduced motion: a fast fade, no bounce.
    pub const REDUCED: Self = Self::new(0.18, 1.0);

    /// Creates a spring.
    pub const fn new(response: f64, damping: f64) -> Self {
        Self { response, damping }
    }

    /// Advances a displacement `x` (value minus target) with velocity `v` by `dt` seconds,
    /// using the exact solution of the damped harmonic oscillator. Stable for any `dt`.
    pub fn step(&self, x: f64, v: f64, dt: f64) -> (f64, f64) {
        let omega = std::f64::consts::TAU / self.response.max(1e-4);
        let zeta = self.damping.max(0.0);
        if (zeta - 1.0).abs() < 1e-4 {
            let e = (-omega * dt).exp();
            let c = v + omega * x;
            ((x + c * dt) * e, (v - omega * c * dt) * e)
        } else if zeta < 1.0 {
            let wd = omega * (1.0 - zeta * zeta).sqrt();
            let e = (-zeta * omega * dt).exp();
            let (sin, cos) = (wd * dt).sin_cos();
            let new_x = e * (x * cos + (v + zeta * omega * x) / wd * sin);
            let new_v = e * (v * cos - (zeta * omega * v + omega * omega * x) / wd * sin);
            (new_x, new_v)
        } else {
            let s = (zeta * zeta - 1.0).sqrt();
            let r1 = -omega * (zeta - s);
            let r2 = -omega * (zeta + s);
            let c1 = (v - r2 * x) / (r1 - r2);
            let c2 = x - c1;
            let (e1, e2) = ((r1 * dt).exp(), (r2 * dt).exp());
            (c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
        }
    }
}

impl Default for Spring {
    fn default() -> Self {
        Self::BOUNCY
    }
}

/// A value that can be animated by a spring, as up to four `f64` components.
pub trait Animatable: Copy + PartialEq {
    /// Below this distance and speed (per component) the animation counts as settled.
    const EPSILON: f64;
    /// Splits the value into components.
    fn to_components(self) -> [f64; 4];
    /// Rebuilds the value from components.
    fn from_components(c: [f64; 4]) -> Self;
}

impl Animatable for f64 {
    const EPSILON: f64 = 1e-3;
    fn to_components(self) -> [f64; 4] {
        [self, 0.0, 0.0, 0.0]
    }
    fn from_components(c: [f64; 4]) -> Self {
        c[0]
    }
}

impl Animatable for Color {
    const EPSILON: f64 = 5e-4;
    fn to_components(self) -> [f64; 4] {
        [
            f64::from(self.r),
            f64::from(self.g),
            f64::from(self.b),
            f64::from(self.a),
        ]
    }
    fn from_components(c: [f64; 4]) -> Self {
        // Springs overshoot; keep channels in range.
        let ch = |v: f64| v.clamp(0.0, 1.0) as f32;
        Color::rgba(ch(c[0]), ch(c[1]), ch(c[2]), ch(c[3]))
    }
}

impl Animatable for Rect {
    const EPSILON: f64 = 0.01;
    fn to_components(self) -> [f64; 4] {
        [self.x0, self.y0, self.x1, self.y1]
    }
    fn from_components(c: [f64; 4]) -> Self {
        Rect::new(c[0], c[1], c[2], c[3])
    }
}

/// How an [`Animated`] value moves towards new targets.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Motion {
    /// Spring towards the target.
    Spring(Spring),
    /// Jump straight to the target.
    Instant,
}

impl From<Spring> for Motion {
    fn from(s: Spring) -> Self {
        Self::Spring(s)
    }
}

/// A value that springs towards its target. Interruptible: retargeting keeps the velocity.
#[derive(Clone, Debug)]
pub struct Animated<T: Animatable> {
    current: [f64; 4],
    velocity: [f64; 4],
    target: T,
    settled: bool,
    /// Geometry (position/size/scale) snaps under Reduce Motion. Colours fade instead.
    pub(crate) geometry: bool,
}

impl<T: Animatable> Animated<T> {
    /// A settled value at `value`.
    pub fn new(value: T) -> Self {
        Self {
            current: value.to_components(),
            velocity: [0.0; 4],
            target: value,
            settled: true,
            geometry: false,
        }
    }

    /// Marks this as a geometric property (snaps instead of fading under Reduce Motion).
    pub(crate) fn geometry(mut self) -> Self {
        self.geometry = true;
        self
    }

    /// The current (animated) value.
    pub fn value(&self) -> T {
        T::from_components(self.current)
    }

    /// The value being animated towards.
    pub fn target(&self) -> T {
        self.target
    }

    /// Whether the value has come to rest at its target.
    pub fn is_settled(&self) -> bool {
        self.settled
    }

    /// Sets a new target. The value keeps its current position and velocity.
    pub fn set_target(&mut self, target: T) {
        if target != self.target {
            self.target = target;
            self.settled = false;
        }
    }

    /// Jumps to `value` with no animation.
    pub fn snap(&mut self, value: T) {
        self.target = value;
        self.current = value.to_components();
        self.velocity = [0.0; 4];
        self.settled = true;
    }

    /// Advances the animation by `dt` seconds. Returns `true` while still moving.
    pub fn step(&mut self, dt: f64, motion: Motion, reduce_motion: bool) -> bool {
        if self.settled {
            return false;
        }
        let spring = match (motion, reduce_motion) {
            (Motion::Instant, _) => None,
            (Motion::Spring(_), true) if self.geometry => None,
            (Motion::Spring(_), true) => Some(Spring::REDUCED),
            (Motion::Spring(s), false) => Some(s),
        };
        let Some(spring) = spring else {
            self.snap(self.target);
            return false;
        };
        let target = self.target.to_components();
        let mut settled = true;
        for (i, &t) in target.iter().enumerate() {
            let (x, v) = spring.step(self.current[i] - t, self.velocity[i], dt);
            self.current[i] = t + x;
            self.velocity[i] = v;
            if x.abs() > T::EPSILON || v.abs() > T::EPSILON * 10.0 {
                settled = false;
            }
        }
        if settled {
            self.snap(self.target);
        }
        !settled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference: integrate x'' = -ω²x - 2ζωx' with tiny semi-implicit Euler steps.
    fn integrate(spring: Spring, x0: f64, v0: f64, t: f64) -> (f64, f64) {
        let omega = std::f64::consts::TAU / spring.response;
        let (mut x, mut v) = (x0, v0);
        let h = 1e-6;
        let steps = (t / h) as usize;
        for _ in 0..steps {
            let a = -omega * omega * x - 2.0 * spring.damping * omega * v;
            v += a * h;
            x += v * h;
        }
        (x, v)
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-2
    }

    #[test]
    fn analytic_matches_integration_in_all_damping_regimes() {
        for damping in [0.3, 0.62, 1.0, 1.8] {
            let s = Spring::new(0.4, damping);
            let exact = s.step(1.0, -2.0, 0.1);
            let reference = integrate(s, 1.0, -2.0, 0.1);
            assert!(
                close(exact, reference),
                "damping {damping}: {exact:?} vs {reference:?}"
            );
        }
    }

    #[test]
    fn stepping_is_frame_rate_independent() {
        let s = Spring::BOUNCY;
        let one = s.step(1.0, 0.0, 0.1);
        let mut many = (1.0, 0.0);
        for _ in 0..10 {
            many = s.step(many.0, many.1, 0.01);
        }
        assert!(close(one, many));
    }

    #[test]
    fn underdamped_overshoots_critical_does_not() {
        let mut min_bouncy = f64::MAX;
        let mut min_smooth = f64::MAX;
        let (mut b, mut c) = ((1.0, 0.0), (1.0, 0.0));
        for _ in 0..200 {
            b = Spring::BOUNCY.step(b.0, b.1, 1.0 / 120.0);
            c = Spring::SMOOTH.step(c.0, c.1, 1.0 / 120.0);
            min_bouncy = min_bouncy.min(b.0);
            min_smooth = min_smooth.min(c.0);
        }
        assert!(
            min_bouncy < -0.05,
            "bouncy should overshoot past the target"
        );
        assert!(min_smooth >= -1e-9, "critically damped must not overshoot");
    }

    #[test]
    fn animated_settles_exactly_on_target() {
        let mut a = Animated::new(0.0);
        a.set_target(100.0);
        let mut frames = 0;
        while a.step(1.0 / 120.0, Motion::Spring(Spring::BOUNCY), false) {
            frames += 1;
            assert!(frames < 2000, "never settled");
        }
        assert_eq!(a.value(), 100.0);
        assert!(a.is_settled());
    }

    #[test]
    fn retargeting_preserves_velocity() {
        let mut a = Animated::new(0.0);
        a.set_target(100.0);
        for _ in 0..6 {
            a.step(1.0 / 120.0, Motion::Spring(Spring::BOUNCY), false);
        }
        let before = a.value();
        a.set_target(0.0);
        a.step(1.0 / 120.0, Motion::Spring(Spring::BOUNCY), false);
        assert!(
            a.value() > before,
            "momentum should carry it forward for a moment after retargeting"
        );
    }

    #[test]
    fn reduce_motion_snaps_geometry_and_fades_colour() {
        let mut geo = Animated::new(0.0).geometry();
        geo.set_target(50.0);
        assert!(!geo.step(0.001, Motion::Spring(Spring::BOUNCY), true));
        assert_eq!(geo.value(), 50.0);

        let mut col = Animated::new(Color::BLACK);
        col.set_target(Color::WHITE);
        assert!(col.step(0.016, Motion::Spring(Spring::BOUNCY), true));
        let v = col.value();
        assert!(v.r > 0.0 && v.r < 1.0, "colour fades rather than jumping");
    }

    #[test]
    fn colour_overshoot_is_clamped() {
        let c = Color::from_components([1.3, -0.2, 0.5, 1.0]);
        assert_eq!((c.r, c.g), (1.0, 0.0));
    }
}
