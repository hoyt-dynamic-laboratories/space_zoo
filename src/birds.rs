//! User-invited birds: cardinal, blue jay, and Enter-key blue heron.
//! Their schedules are separate from random UFO/plane/helicopter visits.
use crate::world::{Point, FPS};
use crate::habitat::{AnchorKind,Habitat};
use crossterm::style::Color;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum BirdKind { Cardinal, BlueJay, BlueHeron }

#[derive(Clone,Copy)]
struct Visit {kind:BirdKind,started:u64,duration:u64,landing_x:i32,landing_y:i32,
    perch:Option<Point>}

pub struct BirdVisual {pub kind:BirdKind,pub pos:Point, pub lines:&'static [&'static str],pub color:Color, pub flying:bool, pub wing_frame:u8}

const CARDINAL: &[&str] = &[
    r"   /^\      ",
    r"  (o >      ",
    r" /<RR>\_    ",
    r"   /  /     ",
];
const BLUEJAY: &[&str] = &[
    r"    /^\       ",
    r"  __o  >      ",
    r" <__BBB>---   ",
    r"    /  \      ",
];
const HERON: &[&str] = &[
    r"       ____         ",
    r"    __/o  \___     ",
    r"  _/       __/     ",
    r" /___   __/        ",
    r"     \ /           ",
    r"      ||           ",
    r"      ||           ",
    r"      | \          ",
    r"      |  \         ",
    r"      |   \        ",
];

const HERON_WINGS_UP:&[&str]=&[
    r"          /\\              ",
    r"         //\\        __    ",
    r"      __//  \\___ _/o \_  ",
    r"  ___/           \__   _> ",
    r" <_______________/  \_/   ",
    r"            \\  \\        ",
];
const HERON_WINGS_MID:&[&str]=&[
    r"                          ",
    r"                ___       ",
    r"   ______________/o \__   ",
    r" _/_____________      _>  ",
    r"         /  /   \___/     ",
];
const HERON_WINGS_DOWN:&[&str]=&[
    r"                ___       ",
    r"    ___________/o \___    ",
    r" __/____________    __>   ",
    r"        \\    \\__/       ",
    r"         \\    \\         ",
    r"          \\    \\        ",
];

pub struct BirdVisits {
    active: Vec<Visit>,
    small_ready: u64,
    heron_ready: u64,
    small_perch_cursor: usize,
    coop_perch_cursor: usize,
}
impl BirdVisits {
    pub fn new() -> Self {Self {active:Vec::new(),small_ready:0,heron_ready:0,
        small_perch_cursor:0,coop_perch_cursor:0}}
    pub fn summon(&mut self, kind:BirdKind, tick:u64, width:u16, height:u16) -> bool {
        self.summon_at(kind,tick,width,height,None)
    }
    pub fn perch_occupied(&self,p:Point)->bool {
        self.active.iter().any(|v|v.perch==Some(p))
    }
    // Only the destination differs from the existing flight/visit lifecycle.
    pub fn summon_on_anchors(&mut self,kind:BirdKind,tick:u64,width:u16,
        height:u16,habitat:&Habitat)->bool {
        self.summon_on_anchors_avoiding(kind,tick,width,height,habitat,None)
    }
    pub fn summon_on_anchors_avoiding(&mut self,kind:BirdKind,tick:u64,width:u16,
        height:u16,habitat:&Habitat,occupied:Option<Point>)->bool {
        let is_heron=kind==BirdKind::BlueHeron;
        let cursor=if is_heron {self.coop_perch_cursor} else {self.small_perch_cursor};
        let perches:&[AnchorKind]=if is_heron {
            &[AnchorKind::CoopRoofLeft,AnchorKind::CoopRoofRidge,AnchorKind::CoopRoofRight]
        } else {
            &[AnchorKind::BillboardLeftPerch,AnchorKind::BillboardRightPerch]
        };
        let perch=if habitat.pixel_mode() {
            (0..perches.len()).filter_map(|i|
                habitat.anchor(perches[(cursor+i)%perches.len()]))
                .find(|p|Some(*p)!=occupied && !self.perch_occupied(*p))
        } else {None};
        if habitat.pixel_mode() && perch.is_none() {return false;}
        let placed=self.summon_at(kind,tick,width,height,perch);
        if placed {
            if is_heron {self.coop_perch_cursor+=1;}
            else {self.small_perch_cursor+=1;}
        }
        placed
    }
    fn summon_at(&mut self,kind:BirdKind,tick:u64,width:u16,height:u16,
        perch:Option<Point>)->bool {
        let is_heron=kind==BirdKind::BlueHeron;
        if self.active.len()>=2 || (is_heron && tick<self.heron_ready)
            || (!is_heron && tick<self.small_ready) {return false;}
        // Arrive, perch for a moment, depart. Never an accumulating flock of bonuses.
        let duration=if is_heron {FPS*11} else {FPS*7};
        let landing_x=if is_heron {width as i32*58/100} else {width as i32*20/100};
        let landing_y=if height>=100 {
            height as i32*4/5 - if is_heron {18} else {20}
        } else {
            height as i32*3/4 - if is_heron {10} else {5}
        };
        // Fixed roof edges are foot anchors. The existing bird ART remains
        // unchanged; convert foot positions to its top-left image origin.
        let (landing_x,landing_y)=if let Some(feet)=perch {
            let (w,h)=match kind {
                BirdKind::Cardinal=>(14,8),BirdKind::BlueJay=>(16,8),
                BirdKind::BlueHeron=>(27,18),
            };
            (feet.x-w/2,feet.y-h)
        } else {(landing_x,landing_y)};
        self.active.push(Visit {kind,started:tick,duration,landing_x,landing_y,perch});
        if is_heron {self.heron_ready=tick+duration;} else {self.small_ready=tick+FPS*3;}
        true
    }
    pub fn advance(&mut self,tick:u64){self.active.retain(|v|tick<v.started+v.duration);}
    pub fn visual(&self,tick:u64,width:u16)->impl Iterator<Item=BirdVisual> + '_ {
        self.active.iter().filter_map(move |v| {
            let age=tick.saturating_sub(v.started);
            if age>=v.duration{return None;}
            let (lines,color)=match v.kind {
                BirdKind::Cardinal=>(CARDINAL,Color::Red),
                BirdKind::BlueJay=>(BLUEJAY,Color::Blue),
                BirdKind::BlueHeron=>(HERON,Color::DarkCyan),
            };
            let size=if v.kind==BirdKind::BlueHeron {38} else {
                lines.iter().map(|s|s.len()).max().unwrap_or(0) as i32
            };
            let approach=FPS*2;
            let depart=FPS*2;
            let x=if age<approach {
                -size+(v.landing_x+size)*age as i32/approach as i32
            } else if age>v.duration-depart {
                let delta=age-(v.duration-depart);
                v.landing_x+(width as i32-v.landing_x+size)*delta as i32/depart as i32
            } else {v.landing_x};
            // Flight crosses the actual sky, not just a 7-pixel hop over
            // the yard. The heron descends slowly to its landing place.
            let altitude=if v.kind==BirdKind::BlueHeron {65} else {38};
            let y=if age<approach {v.landing_y-altitude+altitude*age as i32/approach as i32}
                else if age>=v.duration-depart {
                    v.landing_y-altitude*(age-(v.duration-depart)) as i32/depart as i32
                } else {v.landing_y};
            let flying=age<approach || age>=v.duration-depart;
            // Three slow, readable wing poses. No animation while perched.
            let wing_frame=((age/if flying {3}else{8})%8) as u8;
            let lines=if v.kind==BirdKind::BlueHeron && flying {
                match wing_frame {
                    0|1|7=>HERON_WINGS_UP,2|6=>HERON_WINGS_MID,_=>HERON_WINGS_DOWN,
                }
            } else {lines};
            Some(BirdVisual {kind:v.kind,pos:Point{x,y},lines,color,flying,wing_frame})
        })
    }
    pub fn count(&self)->usize{self.active.len()}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_display_perches_are_used_by_small_visitors() {
        let habitat=Habitat::new(320,200,crate::world::DisplayMode::Pixel);
        let mut birds=BirdVisits::new();
        assert!(birds.summon_on_anchors(BirdKind::Cardinal,0,320,200,&habitat));
        let first=birds.visual(FPS*3,320).next().unwrap();
        let p=habitat.anchor(AnchorKind::BillboardLeftPerch).unwrap();
        assert_eq!(Point{x:first.pos.x+7,y:first.pos.y+8},p);
        birds.advance(FPS*9);
        assert!(birds.summon_on_anchors(BirdKind::BlueJay,FPS*9,320,200,&habitat));
        let second=birds.visual(FPS*12,320).next().unwrap();
        let p=habitat.anchor(AnchorKind::BillboardRightPerch).unwrap();
        assert_eq!(Point{x:second.pos.x+8,y:second.pos.y+8},p);
    }
    #[test]
    fn heron_uses_a_real_coop_roof_perch() {
        let habitat=Habitat::new(320,200,crate::world::DisplayMode::Pixel);
        let mut birds=BirdVisits::new();
        assert!(birds.summon_on_anchors(BirdKind::BlueHeron,0,320,200,&habitat));
        let heron=birds.visual(FPS*3,320).next().unwrap();
        let p=habitat.anchor(AnchorKind::CoopRoofLeft).unwrap();
        assert_eq!(Point{x:heron.pos.x+13,y:heron.pos.y+18},p);
        assert!(!heron.flying);
    }
    #[test]
    fn simultaneous_small_visitors_do_not_stack_on_a_perch() {
        let h=Habitat::new(320,200,crate::world::DisplayMode::Pixel);
        let mut visits=BirdVisits::new();
        assert!(visits.summon_on_anchors(BirdKind::Cardinal,0,320,200,&h));
        assert!(visits.summon_on_anchors(BirdKind::BlueJay,FPS*3,320,200,&h));
        assert_eq!(visits.count(),2);
        assert_ne!(visits.active[0].perch,visits.active[1].perch);
        assert_eq!(visits.active[0].perch,h.anchor(AnchorKind::BillboardLeftPerch));
        assert_eq!(visits.active[1].perch,h.anchor(AnchorKind::BillboardRightPerch));
        visits.advance(FPS*12);
        assert!(visits.summon_on_anchors(BirdKind::BlueHeron,FPS*12,320,200,&h));
        assert_eq!(visits.active[0].perch,h.anchor(AnchorKind::CoopRoofLeft));
    }
    #[test]
    fn flying_heron_has_three_distinct_silhouettes() {
        let mut birds=BirdVisits::new();
        assert!(birds.summon(BirdKind::BlueHeron,0,320,200));
        let art:Vec<Vec<&str>>=[0,8,16].iter()
            .map(|&tick|birds.visual(tick,320).next().unwrap().lines.to_vec()).collect();
        assert_ne!(art[0],art[1]);
        assert_ne!(art[1],art[2]);
        let landed=birds.visual(FPS*3,320).next().unwrap();
        assert!(!landed.flying);
    }
    #[test]
    fn heron_is_bounded() {
        let mut birds=BirdVisits::new();
        assert!(birds.summon(BirdKind::BlueHeron,0,320,200));
        assert!(!birds.summon(BirdKind::BlueHeron,10,320,200));
        birds.advance(FPS*12);
        assert_eq!(birds.count(),0);
    }
}
