//! Shared world landmarks for pixels, ASCII, NPC curiosity and bedtime.
//! Geometry lives here; each renderer decides how to depict it.
use crate::sprite::{self, Species};
use crate::coop::Coop;
use crate::billboard::Billboard;
use crate::world::{DisplayMode, Point};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place { Pine, Scratch, Rock, Clearing }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorKind {
    BillboardLeftPerch, BillboardRightPerch,
    BillboardLeftFront, BillboardLeftRear, BillboardRightFront, BillboardRightRear,
    CoopRoofLeft, CoopRoofRidge, CoopRoofRight,
    CoopDoor, CoopFront, CoopRear, CoopLeft, CoopRight,
    TreeBase, TreePerch, RockSide, Clearing,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor { pub kind:AnchorKind, pub point:Point }

// Tiny foot-level obstruction, not a tall opaque wall or a physics collider.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct Footprint {pub x0:i32,pub x1:i32,pub y0:i32,pub y1:i32}
impl Footprint {
    pub fn contains(self, point:Point)->bool{
        point.x>=self.x0 && point.x<=self.x1 && point.y>=self.y0 && point.y<=self.y1
    }
}


// A two-corner obstacle detour, requiring no pathfinding graph or allocations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Detour { pub via:Point, pub exit:Point, pub stage:u8 }

#[derive(Debug, Clone, Copy)]
pub struct Landmark { pub place: Place, pub point: Point }

pub struct Habitat {
    pub landmarks: Vec<Landmark>,
    pub anchors: Vec<Anchor>,
    pub footprints: Vec<Footprint>,
    pub coop: Coop,
    width: u16,
    height: u16,
    mode: DisplayMode,
}

impl Habitat {
    pub fn new(width: u16, height: u16, mode: DisplayMode) -> Self {
        let mut h = Self { landmarks: Vec::new(), anchors:Vec::new(), footprints:Vec::new(), coop: Coop::new(width,height,mode), width, height, mode };
        h.resize(width, height);
        h
    }
    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.coop = Coop::new(width, height, self.mode);
        let w = width as i32;
        let h = height as i32;
        let base = if self.mode == DisplayMode::Pixel { 4*h/5 } else { 3*h/4 };
        self.landmarks.clear();
        let pines: &[i32] = if self.mode == DisplayMode::Pixel { &[5,20,60,96] } else { &[10,80] };
        for percent in pines {
            self.landmarks.push(Landmark { place: Place::Pine,
                point: Point { x: w*(*percent)/100, y: base } });
        }
        for (place,x,y) in [(Place::Scratch,34,77),(Place::Rock,68,76),(Place::Clearing,55,81)] {
            self.landmarks.push(Landmark { place, point: Point { x:w*x/100, y:h*y/100 } });
        }
        self.anchors.clear();
        self.footprints.clear();
        if self.mode==DisplayMode::Pixel {
            let [left,right]=Billboard::perches();
            for (kind,point) in [(AnchorKind::BillboardLeftPerch,left),
                (AnchorKind::BillboardRightPerch,right)] {
                self.anchors.push(Anchor{kind,point});
            }
            let [left,right]=Billboard::stand_feet(h);
            for (front,rear,foot) in [
                (AnchorKind::BillboardLeftFront,AnchorKind::BillboardLeftRear,left),
                (AnchorKind::BillboardRightFront,AnchorKind::BillboardRightRear,right)] {
                self.anchors.push(Anchor{kind:front,point:Point{x:foot.x,y:foot.y+4}});
                self.anchors.push(Anchor{kind:rear,point:Point{x:foot.x,y:foot.y-4}});
                self.footprints.push(Footprint{x0:foot.x-2,x1:foot.x+2,
                    y0:foot.y-2,y1:foot.y+2});
            }
            let [left,ridge,right]=self.coop.roof_perches();
            for (kind,point) in [(AnchorKind::CoopRoofLeft,left),
                (AnchorKind::CoopRoofRidge,ridge),(AnchorKind::CoopRoofRight,right)] {
                self.anchors.push(Anchor{kind,point});
            }
            let (x0,x1,y0,y1)=self.coop.ground_bounds();
            self.footprints.push(Footprint{x0,x1,y0,y1});
        }
        for (kind,point) in [
            (AnchorKind::CoopDoor,self.coop.door),
            (AnchorKind::CoopFront,self.coop.front_approach()),
            (AnchorKind::CoopRear,self.coop.rear_approach()),
            (AnchorKind::CoopLeft,self.coop.left_approach()),
            (AnchorKind::CoopRight,self.coop.right_approach())] {
            self.anchors.push(Anchor{kind,point});
        }
        for landmark in &self.landmarks {
            match landmark.place {
                Place::Pine => {
                    self.anchors.push(Anchor{kind:AnchorKind::TreeBase,point:landmark.point});
                    if self.mode==DisplayMode::Pixel {
                        let x=landmark.point.x;
                        self.anchors.push(Anchor{kind:AnchorKind::TreePerch,
                            point:Point{x,y:landmark.point.y-(16+x%5*2)/2}});
                    }
                }
                Place::Rock => self.anchors.push(Anchor{kind:AnchorKind::RockSide,
                    point:Point{x:landmark.point.x+4,y:landmark.point.y}}),
                Place::Clearing => self.anchors.push(Anchor{kind:AnchorKind::Clearing,
                    point:landmark.point}),
                Place::Scratch => {},
            }
        }
    }
    pub fn anchor(&self, kind: AnchorKind)->Option<Point> {
        self.anchors.iter().find(|a|a.kind==kind).map(|a|a.point)
    }
    pub fn pixel_mode(&self)->bool {self.mode==DisplayMode::Pixel}
    // One authoritative logical-pixel footprint for NPCs, probe and painters.
    pub fn animal_size(species:Species)->(i32,i32) {
        match species {
            Species::Chicken=>(20,12), Species::GuineaFowl=>(20,13),
            Species::GuineaChick=>(18,10), Species::Shepherd=>(60,34),
            Species::Cat=>(13,9),
        }
    }
    pub fn size(&self,species:Species)->(i32,i32) {
        if self.pixel_mode() {Self::animal_size(species)} else {sprite::dimensions(species)}
    }
    pub fn at_feet(&self, feet:Point, species:Species)->Point {
        if self.mode==DisplayMode::Ascii {return feet;}
        let (w,h)=Self::animal_size(species);
        Point{x:feet.x-w/2,y:feet.y-h}
    }
    pub fn feet(&self,pos:Point,species:Species)->Point {
        if self.mode==DisplayMode::Ascii {return pos;}
        let (w,h)=Self::animal_size(species);
        Point{x:pos.x+w/2,y:pos.y+h}
    }
    // Targeted movement takes a deterministic two-corner detour around ground
    // footprints. The painter still sorts by feet; no 3D engine or nav graph.
    pub fn route_step(&self,pos:Point,target:Point,species:Species,
        detour:&mut Option<Detour>, entering_coop:bool)->Point {
        if self.mode!=DisplayMode::Pixel {
            // Preserve the pre-.4232 ASCII stopping threshold.
            let dx=target.x-pos.x;
            let dy=target.y-pos.y;
            return Point{x:pos.x+if dx.abs()>2 {dx.signum()} else {0},
                y:pos.y+if dy.abs()>2 {dy.signum()} else {0}};
        }
        let mut finished=false;
        if let Some(d)=detour.as_mut() {
            let corner=if d.stage==0 {d.via} else {d.exit};
            if pos==corner {
                if d.stage==0 {d.stage=1;} else {finished=true;}
            }
        }
        if finished {*detour=None;}
        let next=detour.map_or(target,|d|if d.stage==0 {d.via} else {d.exit});
        let candidate=step_toward(pos,next);
        let safe=self.safe_step(pos,candidate,species,entering_coop && detour.is_none());
        if candidate==safe || detour.is_some() {return safe;}
        let hit=self.feet(candidate,species);
        let Some(block)=self.footprints.iter().find(|f|f.contains(hit)) else {return safe;};
        let here=self.feet(pos,species);
        let goal=self.feet(target,species);
        let top=block.y0-2;
        let bottom=block.y1+2;
        let left=block.x0-2;
        let right=block.x1+2;
        let (w,_)=Self::animal_size(species);
        let right_is_reachable=right+w/2<=self.width as i32;
        let left_is_reachable=left-w/2>=0;
        // For a left/right crossing, go to a front/rear safe lane first.
        let (via,exit)=if here.x<block.x0 || here.x>block.x1 {
            let lane=if goal.y<=top {top} else if goal.y>=bottom {bottom}
                else if (here.y-top).abs()<=(here.y-bottom).abs() {top} else {bottom};
            let far_x=if here.x<block.x0 {right.min(goal.x.max(block.x0))}
                else {left.max(goal.x.min(block.x1))};
            (Point{x:here.x,y:lane},Point{x:far_x,y:lane})
        } else {
            // For a front/rear crossing, first get around the nearest side.
            let side=if !right_is_reachable {left}
                else if !left_is_reachable {right}
                else if goal.x<=left {left} else if goal.x>=right {right}
                else if (here.x-left).abs()<=(here.x-right).abs(){left}else{right};
            let far_y=if here.y<block.y0 {bottom} else {top};
            (Point{x:side,y:here.y},Point{x:side,y:far_y})
        };
        let via=self.at_feet(via,species);
        let exit=self.at_feet(exit,species);
        if via==pos && exit==pos {return safe;}
        *detour=Some(Detour{via,exit,stage:0});
        self.safe_step(pos,step_toward(pos,via),species,false)
    }
    // This helper is also used for native debug/probe movement.
    // Doorway entry is allowed by the existing GoHome state; other creatures
    // take a shallow front/rear detour rather than walking through the wall.
    pub fn is_solid(&self, feet: Point)->bool {
        self.footprints.iter().any(|f|f.contains(feet))
    }
    pub fn safe_step(&self, previous:Point, next:Point, species:Species,
        entering_coop:bool)->Point {
        if self.mode!=DisplayMode::Pixel {return next;}
        let foot=|p:Point| self.feet(p,species);
        if !self.is_solid(foot(next)) {return next;}
        // Only an actual bird at the coop DOOR can enter the footprint.
        // GoHome never disables either billboard support or the other walls.
        if entering_coop {
            let now=foot(next);
            let prev=foot(previous);
            if (now.x-self.coop.door.x).abs()<=3 &&
                (prev.x-self.coop.door.x).abs()<=4 &&
                now.y>=self.coop.door.y-3 && now.y<=self.coop.door.y+3 &&
                prev.y>=self.coop.door.y-3 {return next;}
        }
        // No teleport: resolve collisions one logical pixel at a time.
        // A horizontal crossing detours into the nearer front/rear lane;
        // a vertical crossing rounds the nearer side. Since footprints only
        // occupy the GROUND, taller roof/wall overhangs remain pass-behind.
        let hit=foot(next);
        let Some(f)=self.footprints.iter().find(|f|f.contains(hit)) else {return next};
        let old=foot(previous);
        if f.contains(old) {return previous;}
        let candidates=if next.x!=previous.x {
            let up=Point{x:previous.x,y:previous.y-1};
            let down=Point{x:previous.x,y:previous.y+1};
            if old.y-f.y0 <= f.y1-old.y { [up,down] } else { [down,up] }
        } else {
            let left=Point{x:previous.x-1,y:previous.y};
            let right=Point{x:previous.x+1,y:previous.y};
            if old.x-f.x0 <= f.x1-old.x { [left,right] } else { [right,left] }
        };
        for p in candidates {
            let feet=foot(p);
            if feet.x>=0 && feet.x<self.width as i32
                && feet.y>=0 && feet.y<self.height as i32 && !self.is_solid(feet) {
                return p;
            }
        }
        previous
    }
    pub fn home(&self, species: Species, index: usize) -> Point {
        let w = self.width as i32;
        let h = self.height as i32;
        // Family groups rest in consistent neighborhoods rather than teleporting.
        let (base_x, base_y) = match species {
            Species::Chicken | Species::GuineaFowl | Species::GuineaChick => {
                let (sw,sh)=self.size(species);
                return self.coop.doorway_for_sprite(sw,sh);
            },
            Species::Shepherd => (w*48/100, h*81/100),
            Species::Cat => (w*15/100, h*67/100),
        };
        let spread = 7;
        let (sw,sh) = self.size(species);
        Point { x: (base_x + (index % 5) as i32 * spread - 2*spread).clamp(0,(w-sw).max(0)),
            y: (base_y + (index % 3) as i32 * 2).clamp(0,(h-sh).max(0)) }
    }
}

fn step_toward(pos:Point,target:Point)->Point {
    let dx=target.x-pos.x;
    let dy=target.y-pos.y;
    Point {x:pos.x+dx.signum(),
           y:pos.y+dy.signum()}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coop_target_from_behind_navigates_around_solid_footprint() {
        let habitat=Habitat::new(320,200,DisplayMode::Pixel);
        let door=habitat.coop.door;
        let mut pos=habitat.at_feet(Point{x:door.x,y:door.y-22},Species::Chicken);
        let target=habitat.at_feet(habitat.coop.front_approach(),Species::Chicken);
        let mut detour=None;
        let mut arrived=false;
        for _ in 0..240 {
            pos=habitat.route_step(pos,target,Species::Chicken,&mut detour,false);
            assert!(!habitat.is_solid(habitat.feet(pos,Species::Chicken)),
                "walked through coop wall at {:?}",pos);
            if (pos.x-target.x).abs()<=3 && (pos.y-target.y).abs()<=3 {
                arrived=true;break;
            }
        }
        assert!(arrived,"chicken could not reach front of coop from behind");
    }
    #[test]
    fn roost_entry_only_unlocks_actual_coop_door_not_billboard_feet() {
        let habitat=Habitat::new(320,200,DisplayMode::Pixel);
        let door=habitat.coop.door;
        let prev=habitat.at_feet(Point{x:door.x,y:door.y+3},Species::Chicken);
        let next=habitat.at_feet(Point{x:door.x,y:door.y+2},Species::Chicken);
        assert_eq!(habitat.safe_step(prev,next,Species::Chicken,true),next,
            "a returning bird should enter at the doorway");
        let sign=habitat.anchor(AnchorKind::BillboardLeftFront).unwrap();
        let before=habitat.at_feet(sign,Species::Chicken);
        let into=habitat.at_feet(Point{x:sign.x,y:sign.y-2},Species::Chicken);
        let stepped=habitat.safe_step(before,into,Species::Chicken,true);
        assert!(!habitat.is_solid(habitat.feet(stepped,Species::Chicken)));
    }
    #[test]
    fn exactly_two_display_and_three_roof_perches() {
        let habitat=Habitat::new(320,200,DisplayMode::Pixel);
        let board:Vec<_>=habitat.anchors.iter().filter(|a|matches!(a.kind,
            AnchorKind::BillboardLeftPerch|AnchorKind::BillboardRightPerch)).collect();
        let roof:Vec<_>=habitat.anchors.iter().filter(|a|matches!(a.kind,
            AnchorKind::CoopRoofLeft|AnchorKind::CoopRoofRidge|AnchorKind::CoopRoofRight)).collect();
        assert_eq!(board.len(),2);
        assert_eq!(roof.len(),3);
        assert!(board[0].point.x<board[1].point.x);
        assert!(roof[0].point.x<roof[1].point.x && roof[1].point.x<roof[2].point.x);
        assert_eq!(habitat.footprints.len(),3); // two leg feet and coop ground
    }
    #[test]
    fn solid_ground_never_accepts_an_illegal_step() {
        let habitat=Habitat::new(320,200,DisplayMode::Pixel);
        let coop=habitat.coop;
        let prev=Point{x:coop.x-8,y:coop.door.y-12};
        // Begin OUTSIDE the left wall: chicken feet sit one pixel left
        // of the footprint, then the proposed step crosses into it.
        let next=Point{x:prev.x+1,y:prev.y};
        let corrected=habitat.safe_step(prev,next,Species::Chicken,false);
        assert!(!habitat.is_solid(Point{x:corrected.x+10,y:corrected.y+12}));
        assert!((corrected.x-prev.x).abs()<=1);
        assert!((corrected.y-prev.y).abs()<=1);
        assert_ne!(habitat.safe_step(prev,next,Species::Chicken,true),next,
            "GoHome must not ghost through the side wall");
    }
    #[test]
    fn coop_has_separate_front_and_rear_lanes() {
        let habitat=Habitat::new(320,200,DisplayMode::Pixel);
        assert!(!habitat.is_solid(habitat.coop.front_approach()));
        assert!(!habitat.is_solid(habitat.coop.rear_approach()));
        assert!(habitat.is_solid(Point{x:habitat.coop.x+10,y:habitat.coop.door.y}));
    }
    #[test]
    fn landmarks_stay_within_world() {
        for mode in [DisplayMode::Pixel, DisplayMode::Ascii] {
            let h = Habitat::new(120,50,mode);
            assert!(h.landmarks.iter().all(|p| p.point.x>=0 && p.point.x<120 && p.point.y>=0 && p.point.y<50));
        }
    }
}
