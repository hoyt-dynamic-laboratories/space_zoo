//! Time-bounded antics, independent of rendering backend.
//! An antic owns a short movement/pose; no score, accumulation, or escalation.
use crate::world::FPS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnticKind { Flutter, Dash, Hop, Pounce, Perk }

#[derive(Clone, Copy, Debug)]
pub struct Antic {
    pub kind: AnticKind,
    pub started: u64,
    pub ends: u64,
    pub direction: i32,
}

impl Antic {
    pub fn new(kind: AnticKind, tick: u64, direction: i32) -> Self {
        let frames = match kind {
            AnticKind::Flutter => FPS * 2,
            AnticKind::Dash => FPS * 2,
            AnticKind::Hop => FPS,
            AnticKind::Pounce => FPS * 2,
            AnticKind::Perk => FPS * 2,
        };
        Self { kind, started: tick, ends: tick + frames, direction: direction.signum().max(-1) }
    }

    pub fn active(&self, tick: u64) -> bool { tick < self.ends }
    pub fn y_offset(&self, tick: u64) -> i32 {
        if !self.active(tick) { return 0; }
        let duration = (self.ends - self.started).max(1);
        let elapsed = tick.saturating_sub(self.started).min(duration);
        let triangle = elapsed.min(duration - elapsed) as i32;
        match self.kind {
            AnticKind::Flutter => -(triangle * 23 / (duration as i32 / 2).max(1)),
            AnticKind::Hop | AnticKind::Pounce => -(triangle * 8 / (duration as i32 / 2).max(1)),
            _ => 0,
        }
    }
    pub fn moves_at(&self, tick: u64) -> bool {
        match self.kind {
            AnticKind::Flutter | AnticKind::Dash => tick % 2 == 0,
            AnticKind::Pounce => tick % 3 == 0,
            AnticKind::Hop | AnticKind::Perk => false,
        }
    }
    pub fn stride(&self) -> i32 {
        match self.kind {
            AnticKind::Flutter => 3 * self.direction,
            AnticKind::Dash => 3 * self.direction,
            AnticKind::Pounce => 2 * self.direction,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flutter_returns_to_ground_and_finishes() {
        let a = Antic::new(AnticKind::Flutter, 100, -1);
        assert_eq!(a.y_offset(100), 0);
        assert!(a.y_offset(130) < -5);
        assert!(!a.active(a.ends));
        assert_eq!(a.y_offset(a.ends), 0);
    }
}
