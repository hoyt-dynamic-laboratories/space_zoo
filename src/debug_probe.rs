//! Opt-in .4231 visual anchor probe. Uses the EXISTING chicken pixel art.
//! No debug grid, no new sprites, no background worker or normal-mode work.
use crate::circadian::Phase;
use crate::habitat::{AnchorKind, Habitat};
use crate::npc::Animal;
use crate::sprite::Species;
use crate::staging;
use crate::world::{Point, Rng};

// A/S = jumbotron roof; D/F/G = coop roof; Z/X/C/V = legs;
// B/N/M/H/J = coop approaches; R/T/Y/Q = environmental landmarks.
pub fn key_anchor(ch:char)->Option<AnchorKind> {
    use AnchorKind::*;
    Some(match ch.to_ascii_lowercase() {
        'a'=>BillboardLeftPerch,  's'=>BillboardRightPerch,
        'd'=>CoopRoofLeft,       'f'=>CoopRoofRidge,   'g'=>CoopRoofRight,
        'z'=>BillboardLeftFront, 'x'=>BillboardLeftRear,
        'c'=>BillboardRightFront,'v'=>BillboardRightRear,
        'b'=>CoopFront,          'n'=>CoopRear,       'm'=>CoopDoor,
        'h'=>CoopLeft,           'j'=>CoopRight,
        'r'=>RockSide,           't'=>TreeBase,       'y'=>TreePerch,
        'q'=>Clearing,
        _=>return None,
    })
}

pub struct DebugProbe {
    pub animal:Animal,
    pub anchor:AnchorKind,
    // Keeps the last chosen destination for diagnostic window title.
    pub manual:bool,
}
impl DebugProbe {
    const WIDTH:i32=20;  // native chicken logical sprite width
    const HEIGHT:i32=12; // native chicken logical sprite height

    pub fn new(habitat:&Habitat, height:u16)->Self {
        let mut rng=Rng::new(0x4231);
        let mut animal=Animal::new(1000, Species::Chicken, Point{x:0,y:0},
            Point{x:0,y:0},staging::band(Species::Chicken,height,crate::world::DisplayMode::Pixel),
            Phase::Day,&mut rng);
        // Make the one debug chicken identifiable: existing ivory color variant.
        animal.palette=0;
        let mut probe=Self{animal,anchor:AnchorKind::CoopFront,manual:false};
        probe.snap_to(habitat,AnchorKind::CoopFront);
        probe
    }
    pub fn feet(&self)->Point {
        Point{x:self.animal.pos.x+Self::WIDTH/2,
              y:self.animal.pos.y+Self::HEIGHT}
    }
    pub fn snap_to(&mut self,habitat:&Habitat,anchor:AnchorKind)->bool {
        let Some(feet)=habitat.anchor(anchor) else {return false;};
        self.animal.pos=Point{x:feet.x-Self::WIDTH/2, y:feet.y-Self::HEIGHT};
        self.animal.inside_coop=false;
        self.anchor=anchor;
        self.manual=false;
        true
    }
    // Arrows use the SAME real footprint resolver as ordinary residents;
    // anchor snaps are intentional direct-placement tests, so bypass it.
    pub fn nudge(&mut self,habitat:&Habitat, dx:i32,dy:i32,width:u16,height:u16) {
        let before=self.animal.pos;
        let wanted=Point{x:(before.x+dx).clamp(0,(width as i32-Self::WIDTH).max(0)),
            y:(before.y+dy).clamp(0,(height as i32-Self::HEIGHT).max(0))};
        self.animal.pos=habitat.safe_step(before,wanted,Species::Chicken,false);
        self.manual=true;
    }
    pub fn status(&self)->String {
        let p=self.feet();
        format!("DEBUG {:?}{} feet ({},{})",self.anchor,
            if self.manual {" + arrows"} else {""},p.x,p.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::DisplayMode;
    #[test]
    fn all_keys_resolve_to_existing_pixel_anchors() {
        let h=Habitat::new(320,200,DisplayMode::Pixel);
        for key in "asdfgzxcvbnmhjrtyq".chars() {
            let kind=key_anchor(key).expect("mapped");
            assert!(h.anchor(kind).is_some(),"{key:?} -> {kind:?}");
        }
        assert!(key_anchor('w').is_none());
        assert_eq!(key_anchor('A'),key_anchor('a'));
    }
    #[test]
    fn chicken_feet_land_exactly_at_each_anchor_without_ai_drift() {
        let h=Habitat::new(320,200,DisplayMode::Pixel);
        let mut probe=DebugProbe::new(&h,200);
        assert_eq!(probe.animal.species,Species::Chicken);
        for ch in "asdfgzxcvbnmhjrtyq".chars() {
            let kind=key_anchor(ch).unwrap();
            assert!(probe.snap_to(&h,kind));
            assert_eq!(probe.feet(),h.anchor(kind).unwrap());
        }
    }
    #[test]
    fn arrow_moves_in_one_pixel_steps() {
        let h=Habitat::new(320,200,DisplayMode::Pixel);
        let mut probe=DebugProbe::new(&h,200);
        probe.snap_to(&h,AnchorKind::Clearing);
        let before=probe.feet();
        probe.nudge(&h,1,0,320,200);
        assert_eq!(probe.feet(),Point{x:before.x+1,y:before.y});
    }
}
