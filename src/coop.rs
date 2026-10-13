//! Geometry and homecoming rules for the red chicken coop. No rendering here.
use crate::circadian::Phase;
use crate::world::{DisplayMode, Point, Rng, FPS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coop {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub door: Point, // foot-level entrance, not sprite top-left
}

impl Coop {
    pub fn new(width: u16, height: u16, mode: DisplayMode) -> Self {
        let w = width as i32;
        let h = height as i32;
        match mode {
            DisplayMode::Pixel => {
                let cw = 62.min((w - 6).max(16));
                let ch = 41.min((h - 8).max(12));
                let x = (w - cw - 6).max(0);
                let ground = h * 82 / 100;
                let y = ground - ch;
                Self { x, y, width: cw, height: ch,
                    door: Point { x: x + cw * 57 / 100, y: ground - 2 } }
            }
            DisplayMode::Ascii => {
                let cw = 12.min((w - 2).max(5));
                let ch = 6.min((h - 2).max(3));
                let x = (w - cw - 2).max(0);
                let ground = h * 3 / 4;
                Self { x, y: ground - ch, width: cw, height: ch,
                    door: Point { x: x + cw * 2 / 3, y: ground - 1 } }
            }
        }
    }
    pub fn roof_perches(&self) -> [Point;3] {
        let roof_h=(self.height/3).max(6);
        [Point{x:self.x+self.width/4,y:self.y+roof_h/2},
         Point{x:self.x+self.width/2,y:self.y-1},
         Point{x:self.x+self.width*3/4,y:self.y+roof_h/2}]
    }
    // This is the actual ground-level solid footprint. The roof and walls
    // are visual overhangs: creatures can pass behind them at smaller feet-Y.
    pub fn ground_bounds(&self) -> (i32,i32,i32,i32) {
        (self.x+3,self.x+self.width-3,self.door.y-3,self.door.y+2)
    }
    pub fn rear_approach(&self) -> Point {
        Point{x:self.x+self.width/2,y:self.door.y-6}
    }
    pub fn left_approach(&self) -> Point {
        Point{x:self.x-5,y:self.door.y+4}
    }
    pub fn right_approach(&self) -> Point {
        Point{x:self.x+self.width+5,y:self.door.y+4}
    }
    pub fn front_approach(&self) -> Point {
        Point{x:self.door.x,y:self.door.y+5}
    }
    pub fn doorway_for_sprite(&self, sprite_w: i32, sprite_h: i32) -> Point {
        Point { x: self.door.x - sprite_w / 2,
            y: self.door.y - sprite_h + 1 }
    }
}

/// Dusk homecoming accelerates at 17/18/19 o'clock; at night all birds return.
pub fn should_roost(phase: Phase, hour: u8, rng: &mut Rng) -> bool {
    match phase {
        Phase::Night => true,
        Phase::Dusk => rng.range(100) < match hour { 17 => 48, 18 => 72, _ => 93 },
        _ => false,
    }
}

/// Deterministic delay after the first dawn/day tick inside the coop.
/// Birds leave singly every two seconds; no RNG retry loop, no night escape.
pub fn morning_release_delay(id: usize) -> u64 {
    FPS * (2 + 2 * (id % 16) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dusk_becomes_more_homebound() {
        let mut counts = [0; 3];
        for (i,h) in [17,18,19].iter().enumerate() {
            let mut rng = Rng::new(77);
            for _ in 0..500 { if should_roost(Phase::Dusk,*h,&mut rng) { counts[i]+=1; } }
        }
        assert!(counts[0]<counts[1] && counts[1]<counts[2]);
    }
    #[test]
    fn staggered_release_is_repeatable_and_bounded() {
        assert_eq!(morning_release_delay(0),FPS*2);
        assert_eq!(morning_release_delay(1),FPS*4);
        assert_eq!(morning_release_delay(15),FPS*32);
        assert_eq!(morning_release_delay(16),FPS*2);
    }
}
