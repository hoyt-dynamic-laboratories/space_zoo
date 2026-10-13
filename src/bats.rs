//! Two tiny eastern-red-bat silhouettes. The existing billboard neon and its
//! left/right perches are the only attractors: no fixtures, lamps or particles.
use crate::circadian::Phase;
use crate::habitat::{AnchorKind, Habitat};
use crate::pixel::Canvas;
use crate::world::{Point,FPS};
use crossterm::style::Color;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum BatAction { Roost, Flutter, Orbit, Swoop }
#[derive(Clone,Copy,Debug)]
pub struct Bat {pub pos:Point,pub action:BatAction,pub visible:bool}
pub struct BatPair {pub pair:[Bat;2],night:bool,began:u64}
impl BatPair {
    pub fn new()->Self {Self {pair:[Bat{pos:Point{x:0,y:0},action:BatAction::Roost,visible:false};2],
        night:false,began:0}}
    pub fn advance(&mut self,tick:u64,phase:Phase,width:u16,height:u16,habitat:&Habitat) {
        if phase!=Phase::Night {
            self.night=false;
            for bat in &mut self.pair {bat.visible=false;}
            return;
        }
        if !self.night {self.night=true;self.began=tick;}
        let left=habitat.anchor(AnchorKind::BillboardLeftPerch).unwrap_or(Point{x:width as i32/4,y:height as i32/4});
        let right=habitat.anchor(AnchorKind::BillboardRightPerch).unwrap_or(Point{x:width as i32/3,y:height as i32/4});
        let perches=[left,right];
        for (i,bat) in self.pair.iter_mut().enumerate() {
            bat.visible=true;
            let elapsed=tick.saturating_sub(self.began)+i as u64*FPS*4;
            let cycle=elapsed%(FPS*60);
            let perch=perches[i];
            if cycle<FPS*5 {
                bat.action=BatAction::Roost;
                bat.pos=Point{x:perch.x+if i==0 {-7}else{7},y:perch.y-9};
            } else if cycle<FPS*10 {
                bat.action=BatAction::Flutter;
                let t=(cycle-FPS*5) as f32/(FPS*5) as f32;
                bat.pos=Point{x:perch.x+(t*12.0) as i32,y:perch.y-10-(t*12.0) as i32};
            } else if cycle<FPS*47 {
                bat.action=BatAction::Orbit;
                let angle=(elapsed as f32/FPS as f32)*1.05+i as f32*2.7;
                // Orbit along the sign edges; bats can pass behind its face.
                bat.pos=Point{x:(right.x as f32+10.0+30.0*angle.cos()) as i32,
                    y:(right.y as f32-3.0+16.0*angle.sin()) as i32};
            } else {
                bat.action=BatAction::Swoop;
                let p=(cycle-FPS*47) as f32/(FPS*13) as f32;
                let center=right.x+14;
                bat.pos=Point{x:center+((p*2.0-1.0)*38.0) as i32,
                    y:right.y-3+(p*std::f32::consts::PI).sin().mul_add(24.0,0.0) as i32};
            }
            bat.pos.x=bat.pos.x.clamp(-12,width as i32+12);
            bat.pos.y=bat.pos.y.clamp(3,(height as i32-8).max(3));
        }
    }
    pub fn draw_pixel(&self,canvas:&mut Canvas,tick:u64) {
        use crate::living_animals::{draw,atlas::*};
        for (i,bat) in self.pair.iter().enumerate() {
            if !bat.visible {continue;}
            let clip=if bat.action==BatAction::Roost {&BAT_REST} else {&BAT_FLY};
            draw(canvas,clip,bat.pos.x-7,bat.pos.y-4,false,(tick/2+i as u64*3) as usize,255);
        }
    }
    pub fn ascii_sprite(&self,i:usize)->Option<(Point,&'static [&'static str],Color)> {
        let bat=self.pair.get(i)?;
        if !bat.visible {return None;}
        let art=if bat.action==BatAction::Roost {&ROOST_TEXT[..]}
            else if bat.action==BatAction::Swoop {&SILHOUETTE_LOW[..]}
            else {&SILHOUETTE[..]};
        Some((Point{x:bat.pos.x-4,y:bat.pos.y-1},art,Color::DarkRed))
    }
}
// Black / red-brown wing silhouettes, two discrete flight shapes.
const WINGS_UP:&[&str]=&[
    "k        k",
    "kkk kk kkk",
    " kkbrrbkk ",
    "  kbbbbk  ",
    "    kk    ",
];
const WINGS_DOWN:&[&str]=&[
    "   kkkk   ",
    " kkbrrbkk ",
    "kk kbbk kk",
    "k   kk   k",
    "           ",
];
const ROOST:&[&str]=&[
    "    kk    ",
    "   kbbk   ",
    "   kbbrk   ",
    "    bb    ",
    "    kk    ",
];
const ROOST_TEXT:&[&str]=&[
    r"  /\  ",
    r" (oo) ",
    r"  \/  ",
];
const SILHOUETTE:&[&str]=&[r"\ /\  /",r" (oo)  "];
const SILHOUETTE_LOW:&[&str]=&[r"_ (oo) _",r" \/  \/ "];
#[cfg(test)]mod tests {
    use super::*;
    use crate::world::DisplayMode;
    #[test] fn two_bats_follow_existing_perches_and_hide_at_dawn() {
        let habitat=Habitat::new(320,200,DisplayMode::Pixel);
        let mut bats=BatPair::new();
        for t in 1..FPS*80 {bats.advance(t,Phase::Night,320,200,&habitat);}
        assert!(bats.pair.iter().all(|b|b.visible));
        bats.advance(FPS*81,Phase::Dawn,320,200,&habitat);
        assert!(bats.pair.iter().all(|b|!b.visible));
    }
}
