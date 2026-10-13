//! Shared simulation for both ASCII and pixel modes. No renderer-specific AI.
use crate::background::Background;
use crate::deer::DeerHerd;
use crate::bats::BatPair;
use crate::mythic::{MythicVisitors,MythKind};
use crate::debug_probe::DebugProbe;
use crate::habitat::AnchorKind;
use crate::birds::{BirdKind, BirdVisits};
use crate::interactions::Interactions;
use crate::animations::AnticKind;
use crate::circadian::CircadianClock;
use crate::habitat::Habitat;
use crate::events::{EventManager, VisitorKind};
use crate::npc::Animal;
use crate::flock::Neighbor;
use crate::staging;
use crate::sprite::Species;
use crate::sky::SkyState;

pub const FPS: u64 = 30;
fn a_distance(a: Point, b: Point) -> i32 { (a.x-b.x).abs()+(a.y-b.y).abs() }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point { pub x: i32, pub y: i32 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMode { Ascii, Pixel }

pub struct Rng { state: u64 }
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: if seed == 0 { 0x9E3779B97F4A7C15 } else { seed } }
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    pub fn range(&mut self, ceiling: u32) -> u32 {
        if ceiling == 0 { 0 } else { (self.next_u64() % ceiling as u64) as u32 }
    }
}

#[derive(Clone, Copy)]
pub struct Attention { pub point: Point, pub until: u64 }

pub struct World {
    pub width: u16,
    pub height: u16,
    pub tick: u64,
    pub animals: Vec<Animal>,
    pub background: Background,
    pub clock: CircadianClock,
    pub habitat: Habitat,
    pub events: EventManager,
    pub birds: BirdVisits,
    pub deer: DeerHerd,
    pub bats: BatPair,
    pub mythic: MythicVisitors,
    pub sky: SkyState,
    pub debug_probe:Option<DebugProbe>,
    interactions: Interactions,
    pub attention: Option<Attention>,
    mode: DisplayMode,
    npc_rng: Rng,
}

fn population(mode: DisplayMode, width: u16, height: u16) -> Vec<Species> {
    use Species::*;
    // Guaranteed baseline species, then work toward 8 hens / 5 adult guineas /
    // 3 keets / 1 shepherd / 1 cat as space allows.
    let roster = [
        Shepherd, Cat, GuineaFowl, Chicken, GuineaChick,
        Chicken, Chicken, GuineaFowl, Chicken, GuineaFowl,
        Chicken, GuineaChick, Chicken, GuineaFowl, Chicken,
        Chicken, GuineaFowl, GuineaChick,
    ];
    let cap = if mode == DisplayMode::Pixel { roster.len() }
        else { ((width as usize * height as usize) / 340).clamp(5, roster.len()) };
    roster[..cap].to_vec()
}

impl World {
    pub fn new(width: u16, height: u16, seed: u64, mode: DisplayMode) -> Self {
        Self::with_hour(width, height, seed, mode, None)
    }
    pub fn with_hour(width: u16, height: u16, seed: u64, mode: DisplayMode, hour: Option<u8>) -> Self {
        let clock = CircadianClock::new(hour);
        let habitat = Habitat::new(width, height, mode);
        let mut npc_rng = Rng::new(seed ^ 0x5B8C_4172_D0A6_112B);
        let roster = population(mode, width, height);
        let animals = roster.into_iter().enumerate().map(|(i,species)| {
            let pos=staging::spawn(species,i,width,height,mode,&mut npc_rng);
            Animal::new(i,species,pos,habitat.home(species,i),
                staging::band(species,height,mode),clock.phase(),&mut npc_rng)
        }).collect();
        let mut world = Self {
            width, height, tick: 0, animals,
            background: Background::new(seed ^ 0x22C3_B875_1A06_44DA),
            clock, habitat,
            events: EventManager::new(seed ^ 0x94ED_8F07_BBC1_36A2),
            birds: BirdVisits::new(),
            deer: DeerHerd::new(),bats:BatPair::new(),mythic:MythicVisitors::new(),
            sky: SkyState::new(), debug_probe:None,
            interactions: Interactions::new(seed ^ 0xCCBA_0BA4_01EC_7A12),
            attention: None, mode, npc_rng,
        };
        world.resize(width, height);
        world.background.advance(0, width, height);
        world
    }
    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.habitat.resize(width, height);
        self.deer.resize(width,height);
        if let Some(probe)=&mut self.debug_probe {
            let target=probe.anchor;
            probe.snap_to(&self.habitat,target);
        }
        for (i, animal) in self.animals.iter_mut().enumerate() {
            animal.home = self.habitat.home(animal.species, i);
            animal.band = staging::band(animal.species,height,self.mode);
            animal.keep_on_screen(width, height, self.mode);
        }
        if let Some(attention) = &mut self.attention {
            attention.point.x = attention.point.x.clamp(0, (width as i32 - 1).max(0));
            attention.point.y = attention.point.y.clamp(0, (height as i32 - 1).max(0));
        }
        self.background.force_refresh();
    }
    /// Activates only for --debug, using existing wildlife sprite and anchors.
    pub fn enable_debug(&mut self) {
        self.debug_probe=Some(DebugProbe::new(&self.habitat,self.height));
    }
    pub fn debug_snap(&mut self,anchor:AnchorKind) {
        if let Some(probe)=&mut self.debug_probe {probe.snap_to(&self.habitat,anchor);}
    }
    pub fn debug_nudge(&mut self,dx:i32,dy:i32) {
        if let Some(probe)=&mut self.debug_probe {
            probe.nudge(&self.habitat,dx,dy,self.width,self.height);
        }
    }
    pub fn notice(&mut self) {
        let point = self.attention.map_or(Point { x: self.width as i32 / 2,
            y: self.height as i32 * 2 / 3 }, |a| a.point);
        self.attention = Some(Attention { point, until: self.tick + FPS * 2 });
        let mut nearest: Vec<usize>=self.animals.iter().enumerate()
            .filter(|(_,a)|a.visible()).map(|(i,_)|i).collect();
        nearest.sort_by_key(|&i|a_distance(self.animals[i].pos,point));
        for i in nearest.into_iter().take(2) {
            self.animals[i].notice_player(point,self.tick);
        }
        self.interactions.trigger(&mut self.animals,self.tick);
        if self.interactions.small_bird_visit() {
            let kind=if self.interactions.select_bluejay() {BirdKind::BlueJay}
                else {BirdKind::Cardinal};
            self.birds.summon_on_anchors(kind,self.tick,self.width,self.height,&self.habitat);
        }
    }
    pub fn space(&mut self) {
        let point = self.attention.map_or(Point { x: self.width as i32 / 2,
            y: self.height as i32 * 2 / 3 }, |a| a.point);
        if let Some(animal) = self.animals.iter_mut().filter(|a|a.visible()).min_by_key(|a| {
            (a.pos.x - point.x).abs() + (a.pos.y - point.y).abs()
        }) {
            animal.glance(point,self.tick);
            let dir=if animal.pos.x<point.x {1} else {-1};
            animal.start_antic(AnticKind::Perk,self.tick,dir);
        }
    }
    pub fn greet(&mut self) {
        let point = self.attention.map_or(Point { x: self.width as i32 / 2,
            y: self.height as i32 * 2 / 3 }, |a| a.point);
        self.attention = Some(Attention { point, until: self.tick + FPS * 4 });
        let mut nearest: Vec<usize>=self.animals.iter().enumerate()
            .filter(|(_,a)|a.visible()).map(|(i,_)|i).collect();
        nearest.sort_by_key(|&i|a_distance(self.animals[i].pos,point));
        for i in nearest.into_iter().take(3) {
            self.animals[i].greet(point,self.tick);
        }
        let occupied=self.animals.iter().find_map(|a|a.roof_point(&self.habitat));
        self.birds.summon_on_anchors_avoiding(BirdKind::BlueHeron,self.tick,self.width,
            self.height,&self.habitat,occupied);
    }
    /// C key: open the coop for hens in daylight. Does not affect night roosting.
    pub fn release_chickens(&mut self) -> usize {
        if !matches!(self.clock.phase(),crate::circadian::Phase::Dawn|crate::circadian::Phase::Day) {
            return 0;
        }
        let mut queued=0;
        for chicken in self.animals.iter_mut().filter(|a|
            a.species==Species::Chicken && a.inside_coop) {
            chicken.schedule_daytime_release(self.tick);
            queued+=1;
        }
        queued
    }
    pub fn steer_dog(&mut self, dx: i32, dy: i32) {
        if let Some(dog) = self.animals.iter_mut().find(|a| a.species == Species::Shepherd) {
            let previous=dog.pos;
            dog.steer(dx, dy, self.tick, self.width, self.height,self.mode);
            let desired=dog.pos;
            dog.pos=previous;
            let steps=(desired.x-previous.x).abs().max((desired.y-previous.y).abs());
            for i in 0..steps {
                let next=Point{x:dog.pos.x+if i<(desired.x-previous.x).abs() {
                    (desired.x-previous.x).signum()} else {0},
                    y:dog.pos.y+if i<(desired.y-previous.y).abs() {
                    (desired.y-previous.y).signum()} else {0}};
                dog.pos=self.habitat.safe_step(dog.pos,next,Species::Shepherd,false);
            }
            let dog_pos=dog.pos;
            self.attention=Some(Attention {point:dog_pos,until:self.tick+FPS*2});
            // Steering is its own feedback: nearby birds briefly look at the dog.
            if self.tick % 9 == 0 {
                for animal in self.animals.iter_mut().filter(|a|a.species!=Species::Shepherd) {
                    if a_distance(animal.pos,dog_pos)<25 { animal.notice_dog(dog_pos,self.tick); }
                }
            }
        }
    }
    /// One extra manual encounter of EACH mythical visitor per actual night.
    pub fn summon_mythic(&mut self,kind:MythKind)->bool {
        self.mythic.request(kind,self.clock.phase())
    }
    pub fn helicopter(&mut self) {
        if self.events.summon_helicopter(self.tick, self.height) {
            for animal in &mut self.animals {
                animal.notice_visitor(VisitorKind::Helicopter, self.tick, &mut self.npc_rng);
            }
        }
    }
    pub fn advance(&mut self) {
        self.tick += 1;
        self.clock.advance(self.tick);
        self.sky.advance(self.tick, self.clock.hour(), self.width, self.height);
        self.background.advance(self.tick, self.width, self.height);
        self.birds.advance(self.tick);
        if let Some(kind) = self.events.advance(self.tick, self.width, self.height) {
            for animal in &mut self.animals {
                animal.notice_visitor(kind, self.tick, &mut self.npc_rng);
            }
        }
        if self.attention.is_some_and(|a| a.until <= self.tick) { self.attention = None; }
        // Debug is a stationary, player-controlled geometry probe. It should
        // never roost, wander or be dragged away by neighbor social behavior.
        if self.debug_probe.is_some() {return;}
        let alarm=self.mythic.advance(self.tick,self.clock.phase(),self.width,self.height,&self.habitat);
        if alarm {self.deer.alarm(self.tick);}
        self.deer.advance(self.tick,self.clock.phase(),self.width,self.height,
            &self.habitat,&mut self.npc_rng);
        self.bats.advance(self.tick,self.clock.phase(),self.width,self.height,&self.habitat);
        let neighbors: Vec<Neighbor> = self.animals.iter().filter(|a|a.visible())
            .map(|a| Neighbor {id:a.id, species:a.species, pos:a.pos,
                direction:if a.facing==crate::npc::Facing::Left {-1} else {1}}).collect();
        // Reserve one roof landing from the beginning of the approach. The
        // live reservation is updated after each NPC, so two guineas cannot
        // claim it on the same simulation tick.
        let roof=self.habitat.anchor(AnchorKind::CoopRoofLeft);
        let mut roof_busy=roof.is_some_and(|p|self.birds.perch_occupied(p)) ||
            self.animals.iter().any(Animal::roof_reserved);
        for animal in &mut self.animals {
            let already_reserved=animal.roof_reserved();
            animal.advance(self.tick, self.width, self.height, self.clock.phase(),
                self.clock.hour(), &neighbors, &self.habitat,
                !roof_busy || already_reserved, &mut self.npc_rng);
            if animal.roof_reserved() {roof_busy=true;}
        }
    }
    pub fn mode(&self) -> DisplayMode { self.mode }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_occupants_use_multiple_ground_planes() {
        let w=World::with_hour(320,200,123,DisplayMode::Pixel,Some(13));
        let guineas:Vec<i32>=w.animals.iter().filter(|a|a.species==Species::GuineaFowl)
            .map(|a|a.pos.y).collect();
        let chickens:Vec<i32>=w.animals.iter().filter(|a|a.species==Species::Chicken)
            .map(|a|a.pos.y).collect();
        assert!(!guineas.is_empty() && !chickens.is_empty());
        assert!(guineas.iter().max().unwrap()<chickens.iter().min().unwrap());
    }
    #[test]
    fn ordinary_key_triggers_visible_antics_without_stacking() {
        let mut w=World::with_hour(320,200,177,DisplayMode::Pixel,Some(13));
        w.notice();
        assert!(w.animals.iter().any(|a| a.antic.is_some()), "keypress must cause an antic");
        let count=w.animals.iter().filter(|a|a.antic.is_some()).count();
        w.notice(); // same frame: cooldown prevents an antic pileup
        assert_eq!(w.animals.iter().filter(|a|a.antic.is_some()).count(),count);
    }
    #[test]
    fn enter_invites_only_one_blue_heron_at_a_time() {
        let mut w=World::with_hour(320,200,31,DisplayMode::Pixel,Some(13));
        w.greet();
        assert_eq!(w.birds.count(),1);
        w.greet();
        assert_eq!(w.birds.count(),1);
    }
    #[test]
    fn pixel_roster_is_complete() {
        let w = World::new(240, 150, 2, DisplayMode::Pixel);
        for (kind, expected) in [(Species::Chicken, 8), (Species::GuineaFowl, 5),
            (Species::GuineaChick, 3), (Species::Shepherd, 1), (Species::Cat, 1)] {
            assert_eq!(w.animals.iter().filter(|a| a.species == kind).count(), expected);
        }
    }
    #[test]
    fn night_keets_and_guineas_have_roosts() {
        let w = World::with_hour(240,150,11,DisplayMode::Pixel,Some(22));
        assert_eq!(w.clock.phase(), crate::circadian::Phase::Night);
        let homes: Vec<Point> = w.animals.iter()
            .filter(|a| matches!(a.species, Species::GuineaFowl|Species::GuineaChick))
            .map(|a|a.home).collect();
        assert_eq!(homes.len(),8);
        assert!(homes.iter().all(|p|p.x>=0 && p.y>=0));
    }
    #[test]
    fn night_birds_reach_the_coop() {
        let mut w=World::with_hour(320,200,77,DisplayMode::Pixel,Some(22));
        for _ in 0..(FPS*95) { w.advance(); }
        let still_outside=w.animals.iter().filter(|a|
            matches!(a.species,Species::Chicken|Species::GuineaFowl|Species::GuineaChick)
            && a.visible()).count();
        assert_eq!(still_outside,0,"all birds should enter the coop by full night");
        assert_eq!(w.animals.iter().filter(|a| a.species==Species::Shepherd).count(),1);
    }
    #[test]
    fn daytime_chickens_release_in_order_without_random_retry() {
        let mut w=World::with_hour(320,200,777,DisplayMode::Pixel,Some(7));
        let first=w.animals.iter().position(|a|a.species==Species::Chicken).unwrap();
        let first_id=w.animals[first].id;
        w.animals[first].inside_coop=true;
        for _ in 0..(crate::coop::morning_release_delay(first_id)-1) {w.advance();}
        assert!(w.animals[first].inside_coop);
        w.advance(); w.advance();
        assert!(w.animals[first].visible());
        assert!(!w.habitat.is_solid(w.habitat.feet(w.animals[first].pos,Species::Chicken)));
    }
    #[test]
    fn chicken_key_is_daytime_only_and_staggered() {
        let mut day=World::with_hour(320,200,777,DisplayMode::Pixel,Some(13));
        for a in day.animals.iter_mut().filter(|a|a.species==Species::Chicken) {
            a.inside_coop=true;
        }
        assert_eq!(day.release_chickens(),8);
        day.advance();
        assert_eq!(day.animals.iter().filter(|a|a.species==Species::Chicken && a.visible()).count(),0);
        for _ in 0..(FPS*9) {day.advance();}
        assert!(day.animals.iter().any(|a|a.species==Species::Chicken && a.visible()));
        let mut night=World::with_hour(320,200,777,DisplayMode::Pixel,Some(22));
        for a in night.animals.iter_mut().filter(|a|a.species==Species::Chicken) {
            a.inside_coop=true;
        }
        assert_eq!(night.release_chickens(),0);
        for _ in 0..(FPS*40) {night.advance();}
        assert!(night.animals.iter().filter(|a|a.species==Species::Chicken)
            .all(|a|!a.visible()));
    }
    #[test]
    fn full_zoo_keeps_the_daytime_population_untouched() {
        let mut day=World::with_hour(320,200,44,DisplayMode::Pixel,Some(13));
        day.advance();
        assert_eq!(day.animals.len(),18);
        assert!(day.deer.residents.iter().all(|d|!d.visible));
        assert!(day.bats.pair.iter().all(|b|!b.visible));
        assert!(day.mythic.visitors.iter().all(|v|!v.visible()));
        let mut night=World::with_hour(320,200,44,DisplayMode::Pixel,Some(22));
        night.advance();
        assert_eq!(night.deer.residents.iter().filter(|d|d.visible).count(),4);
        assert_eq!(night.bats.pair.iter().filter(|b|b.visible).count(),2);
        assert_eq!(night.animals.len(),18);
    }
    #[test]
    fn full_zoo_manual_encounters_are_once_each_per_night() {
        let mut w=World::with_hour(320,200,44,DisplayMode::Pixel,Some(22));
        w.advance();
        assert!(w.summon_mythic(MythKind::Chupacabra));
        assert!(!w.summon_mythic(MythKind::Chupacabra));
        assert!(w.summon_mythic(MythKind::Sasquatch));
        assert!(!w.summon_mythic(MythKind::Sasquatch));
    }
    #[test]
    fn star_count_unchanged() {
        let mut w=World::with_hour(320,200,99,DisplayMode::Pixel,Some(18));
        for _ in 0..300 {
            w.advance();
            assert_eq!(w.background.stars().len(),20);
        }
    }
    #[test]
    fn visitor_schedule_independent_of_keyboard() {
        let mut a = World::new(120, 40, 456, DisplayMode::Ascii);
        let mut b = World::new(120, 40, 456, DisplayMode::Ascii);
        for i in 0..15000 {
            a.advance(); b.advance();
            if i % 20 == 0 { b.notice(); b.greet(); b.steer_dog(1, 0); }
        }
        assert_eq!(a.events.next_at(), b.events.next_at());
    }
}
