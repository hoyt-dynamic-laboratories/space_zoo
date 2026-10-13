//! Sacred original twenty cyan blinking stars, plus unobtrusive scenery.
use crate::world::{Point, Rng};

const STAR_COUNT: usize = 20;
const STAR_REFRESH_TICKS: u64 = 2; // Original ~60ms Bash feel.

pub struct Background {
    rng: Rng,
    stars: Vec<Point>,
    refresh_at: u64,
}

impl Background {
    pub fn new(seed: u64) -> Self {
        Self { rng: Rng::new(seed), stars: Vec::new(), refresh_at: 0 }
    }
    pub fn force_refresh(&mut self) { self.refresh_at = 0; }
    pub fn advance(&mut self, tick: u64, width: u16, height: u16) {
        if tick < self.refresh_at { return; }
        self.stars.clear();
        if width == 0 || height == 0 { return; }
        for _ in 0..STAR_COUNT {
            self.stars.push(Point {
                x: self.rng.range(width as u32) as i32,
                y: self.rng.range(height as u32) as i32,
            });
        }
        self.refresh_at = tick + STAR_REFRESH_TICKS;
    }
    pub fn stars(&self) -> &[Point] { &self.stars }
}
