//! Keyboard-triggered animal moments. This module chooses antics, not artwork.
//! Input cadence is capped: more keys give variety, never a bigger reward ramp.
use crate::animations::AnticKind;
use crate::npc::Animal;
use crate::sprite::Species;
use crate::world::{Rng, FPS};

pub struct Interactions {
    rng: Rng,
    next: u64,
    cursor: usize,
}
impl Interactions {
    pub fn new(seed: u64) -> Self { Self {rng:Rng::new(seed),next:0,cursor:0} }
    pub fn trigger(&mut self, animals: &mut [Animal], tick: u64) -> bool {
        if tick < self.next || animals.is_empty() {return false;}
        self.next = tick + FPS / 5; // bounded ~5 interactions/sec, not proportional fireworks
        let len=animals.len();
        let mut candidate=None;
        for offset in 0..len {
            let i=(self.cursor+offset)%len;
            let a=&animals[i];
            if a.visible() && a.can_antic(tick) {candidate=Some(i);break;}
        }
        let Some(i)=candidate else {return false;};
        self.cursor=(i+1)%len;
        let kind=match animals[i].species {
            Species::GuineaFowl => if self.rng.range(3)==0 {AnticKind::Flutter} else {AnticKind::Dash},
            Species::GuineaChick => AnticKind::Hop,
            Species::Chicken => match self.rng.range(3) {0=>AnticKind::Dash,1=>AnticKind::Flutter,_=>AnticKind::Hop},
            Species::Shepherd => if self.rng.range(3)==0 {AnticKind::Dash} else {AnticKind::Perk},
            Species::Cat => if self.rng.range(2)==0 {AnticKind::Pounce} else {AnticKind::Dash},
        };
        let direction=if self.rng.range(2)==0 {-1} else {1};
        animals[i].start_antic(kind,tick,direction)
    }

    // Follow-on flock glances happen because existing NPCs observe moving animals.
    // This director does not override the independent event scheduler.
    pub fn small_bird_visit(&mut self) -> bool {self.rng.range(5)==0}
    pub fn select_bluejay(&mut self) -> bool {self.rng.range(2)==0}
}
