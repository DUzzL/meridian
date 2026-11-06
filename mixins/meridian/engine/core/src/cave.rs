//! When the map switches to a cave layer, and which one.
//!
//! Automatic: when the player is under a solid roof that covers the
//! configured square of columns (1x1, 3x3, 5x5 around them). Manual: a key
//! forces cave mode at the height it was pressed. Changes wait the toggle
//! delay, so walking under a ledge doesn't flicker the map. Layers come in
//! 16-block steps, so a player moving a few blocks up or down keeps the
//! same layer and its tiles.

use std::time::{Duration, Instant};

use crate::config::{AutoCaveMode, CaveSettings};

/// The cave layer (scan start) for a player whose eyes are at `eye_y`.
pub fn layer_for(eye_y: f64) -> i32 {
    (eye_y.floor() as i32).div_euclid(16) * 16 + 15
}

/// Columns either side of the player the roof must cover.
pub fn roof_radius(mode: AutoCaveMode) -> Option<u8> {
    match mode {
        AutoCaveMode::Off => None,
        AutoCaveMode::Roof1x1 => Some(0),
        AutoCaveMode::Roof3x3 => Some(1),
        AutoCaveMode::Roof5x5 => Some(2),
    }
}

#[derive(Debug, Default)]
pub struct CaveControl {
    /// Manual cave mode, at this eye height.
    manual: Option<f64>,
    current: Option<i32>,
    pending: Option<(Option<i32>, Instant)>,
    /// The key was pressed: the next update switches at once.
    immediate: bool,
}

impl CaveControl {
    pub fn manual(&self) -> bool {
        self.manual.is_some()
    }

    /// The key: manual cave mode on (at `eye_y`) or off.
    pub fn toggle_manual(&mut self, eye_y: f64) {
        self.manual = if self.manual.is_some() {
            None
        } else {
            Some(eye_y)
        };
        // A key acts at once.
        self.pending = None;
        self.immediate = true;
    }

    /// The layer to show now. `roof`: the player is under a roof that
    /// covers the configured area; `allowed`: cave maps may show in this
    /// dimension.
    pub fn update(
        &mut self,
        s: &CaveSettings,
        allowed: bool,
        roof: bool,
        eye_y: f64,
        now: Instant,
    ) -> Option<i32> {
        let desired = if !s.allowed || !allowed {
            None
        } else if let Some(y) = self.manual {
            Some(layer_for(y))
        } else if roof && s.auto != AutoCaveMode::Off {
            Some(layer_for(eye_y))
        } else {
            None
        };
        if desired == self.current {
            self.pending = None;
            return self.current;
        }
        let delay = if std::mem::take(&mut self.immediate) {
            Duration::ZERO
        } else {
            Duration::from_millis(s.toggle_delay_ms as u64)
        };
        match self.pending {
            Some((want, since)) if want == desired => {
                if now.duration_since(since) >= delay {
                    self.current = desired;
                    self.pending = None;
                }
            }
            _ => {
                self.pending = Some((desired, now));
                if delay.is_zero() {
                    self.current = desired;
                    self.pending = None;
                }
            }
        }
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_in_sixteen_block_steps() {
        assert_eq!(layer_for(20.5), 31);
        assert_eq!(layer_for(16.0), 31);
        assert_eq!(layer_for(15.9), 15);
        assert_eq!(layer_for(-0.5), -1);
        assert_eq!(layer_for(-17.0), -17);
    }

    #[test]
    fn auto_mode_waits_the_delay() {
        let s = CaveSettings::default();
        let mut c = CaveControl::default();
        let t0 = Instant::now();
        assert_eq!(c.update(&s, true, true, 40.0, t0), None, "not yet");
        assert_eq!(
            c.update(&s, true, true, 40.0, t0 + Duration::from_millis(500)),
            None
        );
        assert_eq!(
            c.update(&s, true, true, 40.0, t0 + Duration::from_millis(1100)),
            Some(47)
        );
        // Out from under the roof: back to the surface after the delay.
        assert_eq!(
            c.update(&s, true, false, 40.0, t0 + Duration::from_millis(1200)),
            Some(47)
        );
        assert_eq!(
            c.update(&s, true, false, 40.0, t0 + Duration::from_millis(2300)),
            None
        );
    }

    #[test]
    fn manual_mode_is_immediate_and_stays() {
        let s = CaveSettings::default();
        let mut c = CaveControl::default();
        let t = Instant::now();
        c.toggle_manual(70.0);
        assert!(c.manual());
        assert_eq!(c.update(&s, true, false, 70.0, t), Some(79));
        assert_eq!(
            c.update(&s, true, false, 200.0, t),
            Some(79),
            "keeps the height it was set at"
        );
        c.toggle_manual(0.0);
        assert_eq!(
            c.update(&s, true, false, 70.0, t + Duration::from_secs(2)),
            None
        );
    }

    #[test]
    fn disallowed_or_off() {
        let mut s = CaveSettings::default();
        let mut c = CaveControl::default();
        let t = Instant::now();
        c.update(&s, false, true, 40.0, t);
        assert_eq!(
            c.update(&s, false, true, 40.0, t + Duration::from_secs(5)),
            None,
            "dimension not allowed"
        );
        s.auto = AutoCaveMode::Off;
        assert_eq!(
            c.update(&s, true, true, 40.0, t + Duration::from_secs(9)),
            None
        );
        assert_eq!(roof_radius(AutoCaveMode::Roof3x3), Some(1));
    }
}
