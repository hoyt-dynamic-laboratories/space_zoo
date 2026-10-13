//! Independent timed visitors. Keypresses never influence this schedule.
use crossterm::style::Color;
use crate::world::{Point, Rng, FPS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisitorKind { Ufo, Plane, Helicopter, Comet }

const UFO: &[&str] = &[
    r"       .----.       ",
    r"   ___/ o  o \___   ",
    r"  <____ U F O ____> ",
    r"      \______/      ",
];
const PLANE: &[&str] = &[
    r"           __|__       ",
    r"     _____/_____/_____ ",
    r"  __/  o    o       /  ",
    r" <_________________/   ",
];
const HELI_A: &[&str] = &[
    r"     ============       ",
    r"          |             ",
    r"    .-----+--.   ___    ",
    r" ___|  o  o  |__/  /    ",
    r" \___________/----'     ",
    r"       /___\            ",
];
const HELI_B: &[&str] = &[
    r"        ====            ",
    r"          |             ",
    r"    .-----+--.   ___    ",
    r" ___|  o  o  |__/  /    ",
    r" \___________/----'     ",
    r"       /___\            ",
];
const COMET: &[&str] = &[
    r"     .      *        ",
    r"  .    .---.          ",
    r"   ----( @ )>        ",
    r"  '    '---'          ",
];

pub struct VisitorVisual {
    pub pos: Point,
    pub lines: &'static [&'static str],
    pub color: Color,
}

struct ActiveVisitor {
    kind: VisitorKind,
    started: u64,
    length: u64,
    y: i32,
}

pub struct EventManager {
    rng: Rng,
    next: u64,
    active: Option<ActiveVisitor>,
    manual: Option<ActiveVisitor>,
    manual_ready: u64,
}

impl EventManager {
    pub fn new(seed: u64) -> Self {
        let rng = Rng::new(seed);
        let next = FPS * 3600; // First automatic visitor after one elapsed hour
        Self { rng, next, active: None, manual: None, manual_ready: 0 }
    }

    pub fn summon_helicopter(&mut self, tick: u64, height: u16) -> bool {
        if tick < self.manual_ready || self.manual.is_some() { return false; }
        self.manual = Some(ActiveVisitor { kind: VisitorKind::Helicopter,
            started: tick, length: FPS * 10, y: (height as i32 / 7).max(1) });
        self.manual_ready = tick + FPS * 15;
        true
    }

    pub fn advance(&mut self, tick: u64, width: u16, height: u16) -> Option<VisitorKind> {
        if self.manual.as_ref().is_some_and(|m| tick >= m.started + m.length) {
            self.manual = None;
        }
        if let Some(a) = &self.active {
            if tick < a.started + a.length { return None; }
            self.active = None;
        }
        if tick < self.next || width < 35 || height < 12 { return None; }
        let kind = match self.rng.range(4) {
            0 => VisitorKind::Ufo,
            1 => VisitorKind::Plane,
            2 => VisitorKind::Helicopter,
            _ => VisitorKind::Comet,
        };
        // Fly across the whole terminal in ~7 terminal cells/second.
        let length = FPS * ((width as u64 / 7).max(8) + 3);
        let y = 1 + self.rng.range((height as u32 / 3).max(1)) as i32;
        self.active = Some(ActiveVisitor { kind, started: tick, length, y });
        // One automatic event per rolling 3600-second runtime period, measured
        // start-to-start rather than adding flight time to each interval.
        self.next=tick+FPS*3600;
        Some(kind)
    }

    pub fn visual(&self, tick: u64, width: u16) -> Option<VisitorVisual> {
        let active = self.active.as_ref().or(self.manual.as_ref())?;
        let (lines, color) = match active.kind {
            VisitorKind::Ufo => (UFO, Color::Cyan),
            VisitorKind::Plane => (PLANE, Color::White),
            VisitorKind::Helicopter => (
                if (tick / 5) % 2 == 0 { HELI_A } else { HELI_B },
                Color::Yellow,
            ),
            VisitorKind::Comet => (COMET, Color::Magenta),
        };
        let art_width = lines.iter().map(|s| s.len()).max().unwrap_or(0) as i32;
        let progress = (tick.saturating_sub(active.started)) as f64 / active.length as f64;
        let x = ((width as i32 + art_width) as f64 * progress) as i32 - art_width;
        Some(VisitorVisual { pos: Point { x, y: active.y }, lines, color })
    }

    pub fn visible_kind(&self) -> Option<VisitorKind> {
        self.active.as_ref().or(self.manual.as_ref()).map(|v| v.kind)
    }

    #[cfg(test)]
    pub fn next_at(&self) -> u64 { self.next }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_visits_are_hourly_without_changing_manual_helicopter() {
        let mut manager=EventManager::new(13);
        assert_eq!(manager.next_at(),FPS*3600);
        assert_eq!(manager.advance(FPS*3600-1,240,150),None);
        assert!(manager.summon_helicopter(FPS*20,150));
        assert_eq!(manager.next_at(),FPS*3600);
        assert!(manager.advance(FPS*3600,240,150).is_some());
        assert_eq!(manager.next_at(),FPS*7200);
        let next=manager.advance(FPS*3600+FPS*120,240,150);
        assert_eq!(next,None);
        assert_eq!(manager.next_at(),FPS*7200);
    }
}
