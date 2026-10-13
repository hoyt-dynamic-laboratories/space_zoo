//! .44: mythical VISITS, never members of the ordinary resident population.
//! One automatic entry per creature per night and one independent manual
//! allowance each. Counters reset on the Dawn/Day/Dusk -> Night transition,
//! never on midnight; a queued visit waits for the current one to depart.
use crate::circadian::Phase;
use crate::habitat::{AnchorKind,Detour,Habitat};
use crate::pixel::Canvas;
use crate::sprite::Species;
use crate::world::{Point,FPS};
use crossterm::style::Color;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum MythKind { Chupacabra, Sasquatch }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum MythStage { Hidden, Approaching, Observing, Departing }
#[derive(Clone,Copy,Debug)]
pub struct Myth {
    pub kind:MythKind,
    pub stage:MythStage,
    pub feet:Point,
    pub facing_right:bool,
    route:Option<Detour>,
    next_step:u64,
    observe_until:u64,
}
impl Myth {
    fn new(kind:MythKind)->Self {Self {kind,stage:MythStage::Hidden,
        feet:Point{x:0,y:0},facing_right:true,route:None,next_step:0,observe_until:0}}
    pub fn visible(&self)->bool {self.stage!=MythStage::Hidden}
    pub fn size(&self)->(i32,i32) {match self.kind {
        MythKind::Chupacabra=>(21,19),MythKind::Sasquatch=>(24,27)
    }}
    pub fn top_left(&self)->Point {let (w,h)=self.size();Point{x:self.feet.x-w/2,y:self.feet.y-h}}
}
pub struct MythicVisitors {
    pub visitors:[Myth;2],
    night:bool,
    began:u64,
    auto_used:[bool;2],
    manual_used:[bool;2],
    pending_auto:[bool;2],
    pending_manual:[bool;2],
}
impl MythicVisitors {
    pub fn new()->Self {Self {visitors:[Myth::new(MythKind::Chupacabra),Myth::new(MythKind::Sasquatch)],
        night:false,began:0,auto_used:[false;2],manual_used:[false;2],
        pending_auto:[false;2],pending_manual:[false;2]}}
    pub fn request(&mut self,kind:MythKind,phase:Phase)->bool {
        if phase!=Phase::Night || !self.night {return false;}
        let i=if kind==MythKind::Chupacabra {0}else{1};
        if self.manual_used[i] {return false;}
        self.manual_used[i]=true;
        self.pending_manual[i]=true;
        true
    }
    pub fn allowances(&self)->([bool;2],[bool;2]) {(self.auto_used,self.manual_used)}
    fn spawn(&mut self,i:usize,tick:u64,width:u16,height:u16,habitat:&Habitat) {
        let visitor=&mut self.visitors[i];
        visitor.stage=MythStage::Approaching;
        visitor.route=None;
        visitor.next_step=tick;
        visitor.observe_until=0;
        visitor.feet=if visitor.kind==MythKind::Chupacabra {
            Point{x:width as i32+24,y:habitat.coop.door.y+5}
        } else {Point{x:-28,y:height as i32*80/100}};
        visitor.facing_right=visitor.kind==MythKind::Sasquatch;
    }
    /// Returns whether the chupacabra is disturbing the deer on this tick.
    pub fn advance(&mut self,tick:u64,phase:Phase,width:u16,height:u16,habitat:&Habitat)->bool {
        if phase==Phase::Night && !self.night {
            self.night=true;self.began=tick;
            self.auto_used=[false;2];self.manual_used=[false;2];
            self.pending_auto=[false;2];self.pending_manual=[false;2];
        } else if phase!=Phase::Night && self.night {
            self.night=false;
            self.pending_auto=[false;2];self.pending_manual=[false;2];
            for visitor in &mut self.visitors {
                if visitor.visible() {visitor.stage=MythStage::Departing;visitor.route=None;}
            }
        }
        if self.night {
            for (i,delay) in [FPS*14,FPS*70].iter().enumerate() {
                if !self.auto_used[i] && tick.saturating_sub(self.began)>=*delay {
                    self.auto_used[i]=true;self.pending_auto[i]=true;
                }
                if !self.visitors[i].visible() && (self.pending_manual[i] || self.pending_auto[i]) {
                    // An additional user visit is prioritized, never destroys the
                    // scheduled automatic one (which remains pending).
                    if self.pending_manual[i] {self.pending_manual[i]=false;}
                    else {self.pending_auto[i]=false;}
                    self.spawn(i,tick,width,height,habitat);
                }
            }
        }
        for i in 0..self.visitors.len() {
            let visitor=&mut self.visitors[i];
            if !visitor.visible() {continue;}
            if visitor.stage==MythStage::Observing {
                if tick>=visitor.observe_until {
                    visitor.stage=MythStage::Departing;visitor.route=None;
                } else {continue;}
            }
            if tick<visitor.next_step {continue;}
            visitor.next_step=tick+if visitor.kind==MythKind::Chupacabra {2}else{4};
            let dest=match visitor.stage {
                MythStage::Approaching=>if visitor.kind==MythKind::Chupacabra {
                    habitat.anchor(AnchorKind::CoopLeft).unwrap_or(Point{x:width as i32*3/4,y:height as i32*4/5})
                } else {
                    habitat.anchor(AnchorKind::Clearing).unwrap_or(Point{x:width as i32/2,y:height as i32*4/5})
                },
                MythStage::Departing=>Point{x:if visitor.kind==MythKind::Chupacabra {
                        width as i32+36
                    } else {width as i32+40},y:visitor.feet.y},
                _=>continue,
            };
            if visitor.stage==MythStage::Departing && visitor.feet.x>width as i32+30 {
                visitor.stage=MythStage::Hidden;continue;
            }
            let old=visitor.feet;
            let from=habitat.at_feet(old,Species::Cat);
            let to=habitat.at_feet(dest,Species::Cat);
            let next=habitat.route_step(from,to,Species::Cat,&mut visitor.route,false);
            visitor.feet=habitat.feet(next,Species::Cat);
            if visitor.feet.x!=old.x {visitor.facing_right=visitor.feet.x>old.x;}
            if visitor.stage==MythStage::Approaching &&
                (visitor.feet.x-dest.x).abs()+(visitor.feet.y-dest.y).abs()<=5 {
                visitor.stage=MythStage::Observing;
                visitor.observe_until=tick+FPS*if visitor.kind==MythKind::Chupacabra {12}else{9};
                visitor.route=None;
            }
        }
        self.visitors[0].visible() &&
            (self.visitors[0].feet.x-habitat.coop.door.x).abs()<45 &&
            (self.visitors[0].feet.y-habitat.coop.door.y).abs()<30
    }
    pub fn draw_pixel(&self,canvas:&mut Canvas,i:usize,tick:u64) {
        use crate::living_animals::{draw,atlas::*};
        let Some(m)=self.visitors.get(i) else {return;};
        if !m.visible(){return;}
        let moving=m.stage!=MythStage::Observing;
        let clip=match (m.kind,moving) {
            (MythKind::Sasquatch,true)=>&SASQUATCH_WALK,
            (MythKind::Sasquatch,false)=>&SASQUATCH_IDLE,
            (MythKind::Chupacabra,true)=>&CHUPACABRA_WALK,
            (MythKind::Chupacabra,false)=>&CHUPACABRA_IDLE,
        };
        let at=m.top_left();
        let period=if moving {if m.kind==MythKind::Sasquatch {4}else{2}} else {8};
        draw(canvas,clip,at.x,at.y,!m.facing_right,(tick/period) as usize,255);
    }
    pub fn ascii_sprite(&self,i:usize)->Option<(Point,&'static [&'static str],Color,bool)> {
        let myth=self.visitors.get(i)?;
        if !myth.visible(){return None;}
        let lines=if myth.kind==MythKind::Chupacabra {&CHUP_TEXT[..]}else{&SASQ_TEXT[..]};
        Some((Point{x:myth.feet.x-8,y:myth.feet.y-lines.len() as i32},lines,
            if myth.kind==MythKind::Chupacabra {Color::Cyan}else{Color::DarkRed},myth.facing_right))
    }
}
// Pixel silhouettes authored for .44. Chupacabra: teal spines, red jaws,
// dark claws. Sasquatch: shaggy rust outline, enormous swinging arms.
const CHUPACABRA:&[&str]=&[
    " k   k   k  k",
    " kk  k  kk k",
    " kUk kkk Ukk",
    " kuUuUUUuUuk",
    "kuUuUUUUUuuUk",
    "kuUUUkkkkUUUk",
    "kUUUkppwwpkUUk",
    "kUUUkpppppkUUk",
    "kUUUkwwppwkUUk",
    "kUUUUkkkkUUUUk",
    " kUUUUUUUUUUk",
    " kUUUuUUUUUUk",
    " kkkUUUUUkkkk",
    "   kUUUUUUk",
    "  kkUUUUUUkk",
    " kkggk  kggkk",
    " kggk    kggk",
    "kggk      kggk",
    "kkk        kkk",
];
const SASQUATCH:&[&str]=&[
    "        kkkkk",
    "       kBBBBBk",
    "      kBBBhBBBk",
    "      kBBeeBBBk",
    "      kBBBBBBBk",
    "       kBBBBBk",
    "     kkBBBBBBBkk",
    "    kBBBBBBBBBBBk",
    "   kBBBBBBBBBBBBBk",
    "  kBBBbBBBBBBbBBBk",
    " kBBBbBBBBBBBBbBBBk",
    " kBBbkBBBBBBBBkbBBk",
    "kBBbk kBBBBBBk kbBBk",
    "kBBk  kBBBBBBk  kBBk",
    "kBBk  kBBBBBBk  kBBk",
    "kBBk  kBBBBBBk  kBBk",
    "khBk  kBBBBBBk  kBhk",
    "khhk  kBBBBBBk  khhk",
    " kk    kBBBBk    kk",
    "       kBBBBk",
    "      kBBBkBBk",
    "      kBBBkBBk",
    "      khBBkBBhk",
    "     khBBk kBBhk",
    "     kBBk   kBBk",
    "    kBBBk   kBBBk",
    "    kkkkk   kkkkk",
];
const CHUP_TEXT:&[&str]=&[r" /\|/\|/\ ",r"< (O  O) >",r" | /OO\ | ",r" /  ||  \ ",r"/___/  \__\"];
const SASQ_TEXT:&[&str]=&[r"    ___    ",r"   /o o\   ",r" _/|___|\_ ",r"/  |###|  \",r"   |###|   ",r"   /| |\   ",r"  /_| |_\  "];

#[cfg(test)]mod tests {
    use super::*;
    use crate::world::DisplayMode;
    #[test] fn separate_auto_and_manual_allowances_reset_on_night_transition() {
        let h=Habitat::new(320,200,DisplayMode::Pixel);
        let mut v=MythicVisitors::new();
        v.advance(1,Phase::Night,320,200,&h);
        assert!(v.request(MythKind::Chupacabra,Phase::Night));
        assert!(!v.request(MythKind::Chupacabra,Phase::Night));
        for t in 2..FPS*75 {v.advance(t,Phase::Night,320,200,&h);}
        assert_eq!(v.allowances().0,[true,true]);
        assert_eq!(v.allowances().1,[true,false]);
        // Midnight is still the same night: no regrant.
        assert!(!v.request(MythKind::Chupacabra,Phase::Night));
        v.advance(FPS*80,Phase::Dawn,320,200,&h);
        v.advance(FPS*81,Phase::Night,320,200,&h);
        assert!(v.request(MythKind::Chupacabra,Phase::Night));
        assert_eq!(v.allowances().0,[false,false]);
    }
    #[test] fn sprites_fit_logical_footprints() {
        for m in [Myth::new(MythKind::Chupacabra),Myth::new(MythKind::Sasquatch)] {
            let art=if m.kind==MythKind::Chupacabra {CHUPACABRA}else{SASQUATCH};
            assert_eq!(art.len() as i32,m.size().1);
            assert!(art.iter().all(|s|s.len()<=m.size().0 as usize));
        }
    }
}
