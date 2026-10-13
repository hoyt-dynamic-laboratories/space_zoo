//! .44: one nocturnal whitetail family, four residents. No new Species variants:
//! this family has its own small state machine, and borrows the existing
//! foot-level obstacle router without touching daytime NPC decisions.
use crate::circadian::Phase;
use crate::habitat::{Detour, Habitat};
use crate::pixel::Canvas;
use crate::sprite::Species;
use crate::world::{Point, Rng, FPS};
use crossterm::style::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeerKind { Stag, Doe, Fawn }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeerAction { Graze, HerdMove, Alert, Rest }

#[derive(Clone, Copy, Debug)]
pub struct Deer {
    pub kind: DeerKind,
    pub action: DeerAction,
    pub feet: Point,
    pub visible: bool,
    pub facing_right: bool,
    route: Option<Detour>,
    next_step: u64,
    leaving: bool,
}
impl Deer {
    fn new(kind: DeerKind) -> Self {
        Self { kind, action: DeerAction::HerdMove,
            feet:Point{x:-70,y:0}, visible:false, facing_right:true,
            route:None, next_step:0, leaving:false }
    }
    pub fn size(&self) -> (i32,i32) {
        match self.kind { DeerKind::Stag=>(29,23),DeerKind::Doe=>(22,16),DeerKind::Fawn=>(16,12) }
    }
    pub fn top_left(&self) -> Point { let (w,h)=self.size();Point{x:self.feet.x-w/2,y:self.feet.y-h} }
}

pub struct DeerHerd {
    pub residents:[Deer;4],
    night:bool,
    herd_goal:Point,
    next_goal:u64,
    alert_until:u64,
}
impl DeerHerd {
    pub fn new() -> Self {
        Self { residents:[Deer::new(DeerKind::Stag),Deer::new(DeerKind::Doe),
            Deer::new(DeerKind::Doe),Deer::new(DeerKind::Fawn)],
            night:false,herd_goal:Point{x:0,y:0},next_goal:0,alert_until:0 }
    }
    pub fn alarm(&mut self, tick:u64) { self.alert_until=self.alert_until.max(tick+FPS*12); }
    fn start(&mut self, tick:u64, width:u16,height:u16) {
        self.night=true;
        self.herd_goal=Point{x:width as i32*49/100,y:height as i32*76/100};
        self.next_goal=tick+FPS*22;
        for (i,deer) in self.residents.iter_mut().enumerate() {
            let (_,sh)=deer.size();
            deer.feet=Point{x:-32-i as i32*12,y:(height as i32*(75+i as i32*2)/100)
                .max(sh+2)};
            deer.visible=true;
            deer.leaving=false;
            deer.action=DeerAction::HerdMove;
            deer.facing_right=true;
            deer.route=None;
            deer.next_step=tick+i as u64*FPS/2;
        }
    }
    pub fn resize(&mut self, width:u16,height:u16) {
        self.herd_goal.x=self.herd_goal.x.clamp(0,(width as i32-1).max(0));
        self.herd_goal.y=self.herd_goal.y.clamp(0,(height as i32-1).max(0));
    }
    pub fn advance(&mut self,tick:u64,phase:Phase,width:u16,height:u16,
        habitat:&Habitat,rng:&mut Rng) {
        if phase==Phase::Night && !self.night {self.start(tick,width,height);}
        if phase!=Phase::Night && self.night {
            self.night=false;
            for deer in &mut self.residents {
                deer.leaving=true; deer.route=None;deer.action=DeerAction::HerdMove;
            }
        }
        if self.night && tick>=self.next_goal {
            self.next_goal=tick+FPS*(12+rng.range(15) as u64);
            self.herd_goal=Point{x:width as i32*(32+rng.range(35) as i32)/100,
                y:height as i32*(73+rng.range(8) as i32)/100};
            for deer in &mut self.residents {deer.route=None;}
        }
        for (i,deer) in self.residents.iter_mut().enumerate() {
            if !deer.visible {continue;}
            if deer.leaving && deer.feet.x< -35-i as i32*12 {
                deer.visible=false;continue;
            }
            let offset=match i {0=>(-23,-2),1=>(6,0),2=>(24,3),_=>(13,8)};
            let dest=if deer.leaving {Point{x:-65-i as i32*12,y:deer.feet.y}}
                else {Point{x:self.herd_goal.x+offset.0,y:self.herd_goal.y+offset.1}};
            let dist=(dest.x-deer.feet.x).abs()+(dest.y-deer.feet.y).abs();
            if !deer.leaving && tick<self.alert_until {
                deer.action=DeerAction::Alert;
                if dist<18 {continue;}
            } else if !deer.leaving && dist<=4 {
                // Deliberate rests and grazes between herd moves.
                deer.action=if (tick/FPS/6+i as u64)%4==0 {DeerAction::Rest}
                    else {DeerAction::Graze};
                continue;
            } else {deer.action=DeerAction::HerdMove;}
            if tick<deer.next_step {continue;}
            deer.next_step=tick+if deer.kind==DeerKind::Fawn {2}else{3};
            let old=deer.feet;
            // Use the authoritative collision router with its cat-sized proxy:
            // translating BOTH endpoints via at_feet means the real whitetail
            // FEET, not the proxy's top-left, follow the same ground geometry.
            let from=habitat.at_feet(old,Species::Cat);
            let to=habitat.at_feet(dest,Species::Cat);
            let next=habitat.route_step(from,to,Species::Cat,&mut deer.route,false);
            deer.feet=habitat.feet(next,Species::Cat);
            if deer.feet.x!=old.x {deer.facing_right=deer.feet.x>old.x;}
        }
    }
    pub fn draw_pixel(&self,canvas:&mut Canvas,i:usize,tick:u64) {
        use crate::living_animals::{draw,atlas::*};
        let Some(d)=self.residents.get(i) else {return;};
        if !d.visible {return;}
        let clip=match (d.kind,d.action) {
            (DeerKind::Stag,DeerAction::HerdMove)=>&STAG_WALK,
            (DeerKind::Stag,DeerAction::Graze)=>&STAG_FORAGE,
            (DeerKind::Stag,DeerAction::Rest)=>&STAG_REST,
            (DeerKind::Stag,_)=>&STAG_LOOK,
            (DeerKind::Doe,DeerAction::HerdMove)=>&DOE_WALK,
            (DeerKind::Doe,DeerAction::Graze)=>&DOE_FORAGE,
            (DeerKind::Doe,DeerAction::Rest)=>&DOE_REST,
            (DeerKind::Doe,_)=>&DOE_LOOK,
            (DeerKind::Fawn,DeerAction::HerdMove)=>&FAWN_WALK,
            (DeerKind::Fawn,DeerAction::Graze)=>&FAWN_FORAGE,
            (DeerKind::Fawn,DeerAction::Rest)=>&FAWN_REST,
            (DeerKind::Fawn,_)=>&FAWN_LOOK,
        };
        let at=d.top_left();
        let period=if d.action==DeerAction::HerdMove {2} else {7};
        draw(canvas,clip,at.x,at.y,!d.facing_right,(tick/period+i as u64*3) as usize,255);
    }
    pub fn ascii_sprite(&self,i:usize)->Option<(Point,&'static [&'static str],Color,bool)> {
        let deer=self.residents.get(i)?;
        if !deer.visible{return None;}
        let lines=match (deer.kind,deer.action) {
            (DeerKind::Stag,DeerAction::Rest)=>STAG_REST_TEXT,
            (DeerKind::Stag,_)=>STAG_TEXT,
            (DeerKind::Doe,DeerAction::Rest)=>DOE_REST_TEXT,
            (DeerKind::Doe,_)=>DOE_TEXT,
            (DeerKind::Fawn,_)=>FAWN_TEXT,
        };
        Some((Point{x:deer.feet.x-6,y:deer.feet.y-lines.len() as i32},lines,
            if deer.kind==DeerKind::Fawn {Color::Yellow}else{Color::DarkYellow},
            deer.facing_right))
    }
}

fn palette(ch:char,kind:DeerKind)->Option<u32> {
    let (body,shade,light)=if kind==DeerKind::Fawn {
        (0xd89150,0xa95733,0xf0bd78)
    }else{(0xab5635,0x713322,0xd18a54)};
    Some(match ch {
        'b'=>body,'s'=>shade,'h'=>light,'a'=>0x693422,
        'o'=>0x2f211e,'e'=>0x211817,'n'=>0x351b17,
        'l'=>shade,'w'=>0xf5e4c5,'p'=>0xf6c98a,
        _=>return None,
    })
}
// All variants share one bounded 29x23 / 22x16 / 16x12 footprint.
// Stag's high branching antlers and the fawn's speckles distinguish family.
const STAG_WALK:&[&str]=&[
    "  a a     a a",
    "  a aa   aa a",
    "   aaa   aaa",
    "    aa   aa",
    "    aa   aa",
    "     a   a",
    "     a   a",
    "    aaaaaaa",
    "   ahbhhhhb",
    "  abbeehbbba",
    "   nbbbsbbbba",
    "    bbbbbbbbbbbbbbbbbww",
    "      bbbbbbbbbbbbbbbbb",
    "       bbbbbhhbbbbbbbbbb",
    "        bbbhhhhbbbbbbbbb",
    "         bbbsbbbbbbbbbb",
    "          sbbbbbbsbbbb",
    "           lll   lll",
    "           ll    ll",
    "           ll     ll",
    "           ll     ll",
    "          lll     lll",
    "          lll     lll",
];
const STAG_GRAZE:&[&str]=&[
    "",
    "",
    "",
    "   a a     a a",
    "    aaa   aaa",
    "     aa   aa",
    "      a   a",
    "      aaaaa",
    "      ahhhh",
    "     aebbhbb",
    "    nbbsbbbbb",
    "     bbbbbbbbbbbbbbbww",
    "      bbbbbbbbbbbbbbbbb",
    "       bbbbbhhbbbbbbbbbb",
    "        bbbhhhhbbbbbbbbb",
    "         bbbsbbbbbbbbbb",
    "          sbbbbbbsbbbb",
    "           lll   lll",
    "           ll    ll",
    "           ll     ll",
    "           ll     ll",
    "          lll     lll",
    "          lll     lll",
];
const STAG_REST:&[&str]=&[
    "",
    "",
    "",
    "    a a    a a",
    "     aaa  aaa",
    "      aa  aa",
    "       a  a",
    "       aaaa",
    "      ahhbba",
    "     abebhbb",
    "      nbbsbb",
    "       bbbbbbbbbbbbbww",
    "        bbbbbbbbbbbbbb",
    "         bbbbbbbbbbbbbb",
    "          bbbbhhbbbbbbb",
    "           bbsbbbbbbbb",
    "            ssssssssss",
    "           llllllllllll",
    "",
    "",
    "",
    "",
    "",
];
const DOE_WALK:&[&str]=&[
    "   bb     bb",
    "  bhbb   bbb",
    "  bhebbbbb",
    "  nbbhbbbbb",
    "   bbbbbbbbbbbwww",
    "    bbbbbbbbbbbbbb",
    "    bbbbhhbbbbbbbbb",
    "     bbbhhbbbbbbbbb",
    "     bbbbbbbbbbbbbb",
    "      sbbbbbbbbbbb",
    "       lll  lll",
    "       ll   ll",
    "       ll    ll",
    "       ll    ll",
    "      lll   lll",
    "      lll   lll",
];
const DOE_GRAZE:&[&str]=&[
    "",
    "",
    "",
    "   bb    bb",
    "  bhebbbbbb",
    "  nbbhbbbbb",
    "    bbbbbbbbbbbww",
    "    bbbbhhbbbbbbbb",
    "     bbbhhbbbbbbbbb",
    "     bbbbbbbbbbbbbb",
    "       lll  lll",
    "       ll   ll",
    "       ll    ll",
    "       ll    ll",
    "      lll   lll",
    "      lll   lll",
];
const DOE_REST:&[&str]=&[
    "",
    "",
    "   bb    bb",
    "  bhbb  bbb",
    "  bhebbbbbb",
    "  nbbhbbbbb",
    "   bbbbbbbbbbbww",
    "    bbbbhhbbbbbbb",
    "     bbbbbbbbbbbbb",
    "      sbbbbbbbbbbb",
    "      llllllllllll",
    "",
    "",
    "",
    "",
    "",
];
const FAWN_WALK:&[&str]=&[
    "  bb  bb",
    "  bhb bbb",
    "  bebbbb",
    " nbbhbbbbbww",
    "  bbbbbbbbbbb",
    "   bbpbpbpbbb",
    "   bbbbbbbbbb",
    "    sbbbsbbb",
    "    ll   ll",
    "    ll   ll",
    "   lll   lll",
    "   lll   lll",
];
const FAWN_REST:&[&str]=&[
    "",
    "",
    "  bb  bb",
    "  bhebbbb",
    "  nebbbbbbww",
    "   bbpbpbbbb",
    "    bbbbbbbb",
    "   lllllllll",
    "",
    "",
    "",
    "",
];
fn art(kind:DeerKind,action:DeerAction)->&'static [&'static str] {
    match (kind,action) {
        (DeerKind::Stag,DeerAction::Rest)=>STAG_REST,
        (DeerKind::Stag,DeerAction::Graze)=>STAG_GRAZE,
        (DeerKind::Stag,_)=>STAG_WALK,
        (DeerKind::Doe,DeerAction::Rest)=>DOE_REST,
        (DeerKind::Doe,DeerAction::Graze)=>DOE_GRAZE,
        (DeerKind::Doe,_)=>DOE_WALK,
        (DeerKind::Fawn,DeerAction::Rest)=>FAWN_REST,
        (DeerKind::Fawn,_)=>FAWN_WALK,
    }
}
const STAG_TEXT:&[&str]=&[r"  Y Y  Y Y",r"  |/ \\|",r"  /-oo-\",r" < (o)  \",r"  /==.=====\",r"  ||    ||",r"  ||    ||"];
const STAG_REST_TEXT:&[&str]=&[r"  Y Y  Y Y",r"  |/ \\|",r"  /-oo-\",r" < (o)  \",r"  /=======\",r"    =====  ",r"           "];
const DOE_TEXT:&[&str]=&[r"  /\  /\  ",r" < (o)\____",r"  |=======\",r"  ||    ||",r"  ||    ||"];
const DOE_REST_TEXT:&[&str]=&[r"  /\  /\  ",r" < (o)\____",r"  |=======\",r"  =========",r"           "];
const FAWN_TEXT:&[&str]=&[r" /\ /\  ",r"<(o)\...\",r" |=...===",r" ||   || "];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::DisplayMode;
    #[test]
    fn whole_herd_arrives_and_retreats_without_disappearing_at_dawn() {
        let h=Habitat::new(320,200,DisplayMode::Pixel);
        let mut herd=DeerHerd::new();let mut rng=Rng::new(7);
        for t in 1..FPS*95 {herd.advance(t,Phase::Night,320,200,&h,&mut rng);}
        assert_eq!(herd.residents.iter().filter(|d|d.visible).count(),4);
        assert!(herd.residents.iter().all(|d|d.feet.x>0));
        herd.advance(FPS*95,Phase::Dawn,320,200,&h,&mut rng);
        assert_eq!(herd.residents.iter().filter(|d|d.visible).count(),4);
        for t in FPS*95+1..FPS*240 {herd.advance(t,Phase::Dawn,320,200,&h,&mut rng);}
        assert!(herd.residents.iter().all(|d|!d.visible));
    }
    #[test] fn sprites_fit_their_fixed_footprints() {
        for kind in [DeerKind::Stag,DeerKind::Doe,DeerKind::Fawn] {
            let d=Deer::new(kind);
            for state in [DeerAction::Graze,DeerAction::HerdMove,DeerAction::Alert,DeerAction::Rest] {
                let art=art(kind,state);
                assert_eq!(art.len() as i32,d.size().1);
                assert!(art.iter().all(|line|line.len()<=d.size().0 as usize));
            }
        }
    }
}
