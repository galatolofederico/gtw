//! Rain on the windows: how wet the glass is, and how far the water on it has
//! moved. The drops themselves are drawn in plane.wgsl (`rain_water`, in the
//! glass pass); this is the state they are drawn from.
//!
//! Three kinds of water move differently, so each has its own running phase
//! (accumulated here, so a change of speed never makes the pattern jump):
//! beads that land and sit (they come and go at a rate set by the rain),
//! drops that run down the glass under gravity while the airflow is too weak
//! to hold them, and drops and streaks the airflow drives up and back over the
//! glass (their speed rises with the square of the airspeed).

/// Water on the glass, 0..1, and the phases of its three kinds of motion.
pub struct Glass {
    /// how much water there is on the glass, 0 (dry) .. 1 (fully beaded)
    pub wet: f32,
    /// bead turnover, in lifetimes (new drops landing, old ones merging away)
    pub beads: f64,
    /// metres a typical drop has run down under gravity
    pub slide: f64,
    /// metres a typical drop has been driven along the airflow
    pub flow: f64,
}

/// How strongly the airflow drags a drop along, against the surface tension
/// that pins it, 0..1. Matches `rain_mob` in plane.wgsl.
pub fn mobility(kt: f64) -> f64 {
    smooth(8.0, 40.0, kt)
}

/// How fast the airflow drives the drops over the glass, m/s. Matches
/// `rain_flow_speed` in plane.wgsl.
pub fn flow_speed(kt: f64) -> f64 {
    (mobility(kt) * 0.25 * (kt / 60.0).powi(2)).min(6.0)
}

fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Glass {
    /// Glass that has been out in `rain` long enough to settle (dry at 0).
    pub fn new(rain: f32) -> Self {
        let mut g = Self { wet: 0.0, beads: 0.0, slide: 0.0, flow: 0.0 };
        g.wet = g.settled(rain as f64, 0.0) as f32;
        g
    }

    /// Water arriving (per second, as a share of the room left) and leaving
    /// (per second, as a share of what is there): rain wets the glass in some
    /// 25 s; it dries over minutes at rest, and the airflow strips it within
    /// seconds at speed, leaving a thin film where the rain keeps coming.
    fn rates(rain: f64, kt: f64) -> (f64, f64) {
        let gain = rain * 0.045;
        let loss = 1.0 / 300.0 + 0.012 * mobility(kt) + 0.22 * smooth(70.0, 220.0, kt);
        (gain, loss)
    }

    fn settled(&self, rain: f64, kt: f64) -> f64 {
        let (gain, loss) = Self::rates(rain, kt);
        if gain <= 0.0 { 0.0 } else { gain / (gain + loss) }
    }

    /// Advance by `dt` seconds of game time in `rain` (0..1) at `kt` knots
    /// indicated airspeed.
    pub fn step(&mut self, dt: f64, rain: f32, kt: f64) {
        if dt <= 0.0 {
            return;
        }
        let rain = rain.clamp(0.0, 1.0) as f64;
        let kt = kt.max(0.0);
        let (gain, loss) = Self::rates(rain, kt);
        let w = self.wet as f64;
        self.wet = (w + dt * (gain * (1.0 - w) - loss * w)).clamp(0.0, 1.0) as f32;
        if self.wet < 1e-3 && rain == 0.0 {
            self.wet = 0.0;
        }
        // Wrapped well inside f32 precision; a wrap reshuffles the drops once.
        let mob = mobility(kt);
        self.beads = (self.beads + dt * (0.015 + 0.1 * rain)).rem_euclid(4096.0);
        self.slide = (self.slide + dt * 0.035 * (1.0 - mob)).rem_euclid(4096.0);
        self.flow = (self.flow + dt * flow_speed(kt)).rem_euclid(4096.0);
    }

    /// The phases for the glass draw (glow.z, glow.w, tint.w of a window piece).
    pub fn phases(&self) -> [f32; 3] {
        [self.beads as f32, self.slide as f32, self.flow as f32]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wets_and_dries() {
        let mut g = Glass::new(0.0);
        for _ in 0..600 {
            g.step(0.1, 1.0, 0.0);
        }
        assert!(g.wet > 0.8, "{}", g.wet);
        let parked = g.wet;
        // Fast flight in the same rain leaves only a film.
        for _ in 0..300 {
            g.step(0.1, 1.0, 300.0);
        }
        assert!(g.wet < 0.3 && g.wet > 0.05, "{}", g.wet);
        // Rain stops at rest: it takes minutes to dry.
        let mut h = Glass { wet: parked, beads: 0.0, slide: 0.0, flow: 0.0 };
        for _ in 0..600 {
            h.step(0.1, 0.0, 0.0);
        }
        assert!(h.wet > 0.6 * parked && h.wet < parked, "{}", h.wet);
        assert_eq!(Glass::new(0.0).wet, 0.0);
    }
}
