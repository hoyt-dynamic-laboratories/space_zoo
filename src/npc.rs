//! Individual creature state and small memories. No drawing or terminal code.
use crate::circadian::Phase;
use crate::animations::{Antic, AnticKind};
use crate::coop;
use crate::curiosity;
use crate::events::VisitorKind;
use crate::flock::{self, Neighbor};
use crate::habitat::{Habitat, Place, AnchorKind, Detour};
use crate::routines::{self, Routine};
use crate::staging::Band;
use crate::shepherd::{self, DogRoutine};
use crate::sprite::{self, Pose, Species};
use crate::world::{DisplayMode, Point, Rng, FPS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing { Left, Right }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Activity {
    Wander, Pause, Forage, Observe, Rest, Investigate, Alert, GoHome,
    Stalk, Groom, Scratch, DustBathe, Perch, Hop, Huddle,
}

pub struct Animal {
    pub id: usize,
    pub palette: u8,
    pub species: Species,
    pub pos: Point,
    pub facing: Facing,
    pub home: Point,
    pub band: Band,
    pub inside_coop: bool,
    manual_steering: bool,
    activity: Activity,
    until: u64,
    next_step: u64,
    notice_again: u64,
    target: Option<Point>,
    interest: Option<Place>,
    last_interest: Option<Place>,
    last_phase: Phase,
    // Spatial waypoints belong to navigation; they do not choose behaviors.
    route: Option<Detour>,
    home_approached: bool,
    release_at: Option<u64>,
    // Roof destination is a visual flight above a normal, navigable ground approach.
    pending_perch: Option<Point>,
    perch_at: Option<Point>,
    perch_started: u64,
    pending_scratch: bool,
    pub antic: Option<Antic>,
}

impl Animal {
    pub fn new(id: usize, species: Species, pos: Point, home: Point, band: Band, phase: Phase, rng: &mut Rng) -> Self {
        Self { id, palette: (id%7) as u8, species, pos, home, band, inside_coop: false, manual_steering: false,
            facing: if rng.range(2)==0 { Facing::Left } else { Facing::Right },
            activity: Activity::Pause, until: 20+rng.range(60) as u64,
            next_step: 0, notice_again: 0, target: None, interest: None,
            last_interest: None, last_phase: phase, route:None,
            home_approached:false, release_at:None,
            pending_perch:None, perch_at:None, perch_started:0,
            pending_scratch:false, antic: None }
    }

    fn bird(&self) -> bool {
        matches!(self.species,Species::Chicken|Species::GuineaFowl|Species::GuineaChick)
    }
    pub fn visible(&self) -> bool { !self.inside_coop }
    /// A reservation includes the ground approach, so only one guinea claims
    /// the left roof perch, and a visiting heron cannot land on top of it.
    pub fn is_perching(&self) -> bool {self.activity==Activity::Perch}
    pub fn roof_reserved(&self) -> bool {
        self.pending_perch.is_some() || self.perch_at.is_some()
    }
    pub fn roof_point(&self, habitat:&Habitat) -> Option<Point> {
        if self.roof_reserved() {habitat.anchor(AnchorKind::CoopRoofLeft)} else {None}
    }
    pub fn schedule_daytime_release(&mut self, tick:u64) {
        if self.species==Species::Chicken && self.inside_coop {
            // Key-triggered chicken exit is prompt but still staggered.
            let requested=tick+FPS/2+(self.id as u64%8)*FPS;
            self.release_at=Some(self.release_at.map_or(requested,|at|at.min(requested)));
        }
    }
    pub fn visual_pos(&self, tick: u64) -> Point {
        if self.activity==Activity::Perch {
            if let Some(target)=self.perch_at {
                let approach=FPS*2;
                let age=tick.saturating_sub(self.perch_started);
                let remain=self.until.saturating_sub(tick);
                let fraction=if age<approach {age as i64*1000/approach as i64}
                    else if remain<approach {remain as i64*1000/approach as i64}
                    else {1000};
                let f=fraction.clamp(0,1000) as i32;
                return Point{x:self.pos.x+(target.x-self.pos.x)*f/1000,
                    y:self.pos.y+(target.y-self.pos.y)*f/1000};
            }
        }
        let hop=if self.activity==Activity::Hop && (tick/7)%2==0 {-3} else {0};
        Point {x:self.pos.x, y:self.pos.y+hop+self.antic.map_or(0,|a|a.y_offset(tick))}
    }
    pub fn can_antic(&self, tick: u64) -> bool {
        self.visible() && !matches!(self.activity,Activity::GoHome|Activity::Perch)
            && self.antic.map_or(true, |a| !a.active(tick))
    }
    pub fn start_antic(&mut self, kind: AnticKind, tick: u64, direction: i32) -> bool {
        if !self.can_antic(tick) {return false;}
        let antic=Antic::new(kind,tick,direction);
        self.facing=if direction<0 {Facing::Left} else {Facing::Right};
        self.activity=Activity::Pause;
        self.until=antic.ends+FPS/2;
        self.target=None;
        self.interest=None;
        self.route=None;
        self.pending_perch=None;
        self.perch_at=None;
        self.pending_scratch=false;
        self.antic=Some(antic);
        true
    }

    pub fn keep_on_screen(&mut self, width: u16, height: u16, mode: DisplayMode) {
        let (w,h)=if mode==DisplayMode::Pixel {Habitat::animal_size(self.species)}
            else {sprite::dimensions(self.species)};
        self.pos.x=self.pos.x.clamp(0,(width as i32-w).max(0));
        self.pos.y=self.pos.y.clamp(0,(height as i32-h).max(0));
        // Returning to the coop may cross roaming bands. All other ordinary
        // behavior stays in the animal's chosen depth of the yard.
        if !matches!(self.activity,Activity::GoHome|Activity::Investigate|
            Activity::Perch|Activity::Scratch)
            && self.route.is_none() && !self.inside_coop && !self.manual_steering {
            self.pos.y=self.band.clamp(self.pos.y).clamp(0,(height as i32-h).max(0));
        }
    }
    fn face(&mut self, target: Point) {
        if target.x<self.pos.x {self.facing=Facing::Left;}
        if target.x>self.pos.x {self.facing=Facing::Right;}
    }

    pub fn notice_player(&mut self, point: Point, tick: u64) {
        if self.inside_coop || tick<self.notice_again || matches!(self.activity,Activity::GoHome|Activity::Perch) {return;}
        self.notice_again=tick+FPS*2;
        self.route=None;
        self.pending_perch=None;
        self.pending_scratch=false;
        self.target=Some(point);
        self.interest=None;
        self.face(point);
        let distance=(self.pos.x-point.x).abs()+(self.pos.y-point.y).abs();
        self.activity=if distance<22 {Activity::Observe} else {Activity::Investigate};
        self.until=tick+FPS*3;
    }
    pub fn glance(&mut self, point: Point, tick: u64) {
        if self.inside_coop || matches!(self.activity,Activity::GoHome|Activity::Perch) {return;}
        self.face(point);
        self.activity=Activity::Observe;
        self.until=tick+FPS;
        self.route=None;
        self.pending_perch=None;
        self.pending_scratch=false;
        self.interest=None;
    }
    pub fn greet(&mut self, point: Point, tick: u64) {
        if self.inside_coop || matches!(self.activity,Activity::GoHome|Activity::Perch) {return;}
        self.route=None;
        self.pending_perch=None;
        self.pending_scratch=false;
        self.target=Some(point);
        self.interest=None;
        self.face(point);
        self.activity=Activity::Observe;
        self.until=tick+FPS*4;
        self.notice_again=tick+FPS*2;
    }
    pub fn steer(&mut self, dx: i32, dy: i32, tick: u64, width: u16, height: u16,
        mode:DisplayMode) {
        self.pos.x+=dx*3;
        self.pos.y+=dy*3;
        if dx<0 {self.facing=Facing::Left;}
        if dx>0 {self.facing=Facing::Right;}
        self.manual_steering=true;
        self.activity=Activity::Wander;
        self.until=tick+FPS*3;
        self.next_step=tick+6;
        self.target=None;
        self.interest=None;
        self.route=None;
        self.pending_perch=None;
        self.pending_scratch=false;
        self.antic=None; // Player controls the shepherd, never an unresponsive animation.
        self.keep_on_screen(width,height,mode);
    }
    pub fn notice_dog(&mut self, dog: Point, tick: u64) {
        if self.inside_coop || matches!(self.activity,Activity::GoHome|Activity::Perch|Activity::Rest) { return; }
        self.face(dog);
        self.pending_perch=None;
        self.pending_scratch=false;
        self.activity=Activity::Observe;
        self.until=tick+FPS;
    }
    pub fn notice_visitor(&mut self, visitor: VisitorKind, tick: u64, rng: &mut Rng) {
        if self.inside_coop || self.activity==Activity::Perch ||
            (self.activity==Activity::Rest && rng.range(3)!=0) {return;}
        self.activity=match self.species {
            Species::GuineaFowl | Species::GuineaChick => Activity::Alert,
            Species::Cat if visitor==VisitorKind::Plane => Activity::Rest,
            _ => Activity::Observe,
        };
        self.until=tick+FPS*(2+rng.range(3) as u64);
        self.pending_perch=None;
        self.pending_scratch=false;
        self.target=None;
        self.interest=None;
        self.route=None;
    }

    pub fn advance(&mut self, tick: u64, width: u16, height: u16, phase: Phase,
        hour: u8, neighbors: &[Neighbor], habitat: &Habitat,
        roof_free:bool, rng: &mut Rng) {
        if self.manual_steering && tick >= self.until {
            self.manual_steering = false;
            self.pos.y = self.band.clamp(self.pos.y);
        }
        if self.last_phase!=phase {
            self.last_phase=phase;
            self.until=self.until.min(tick+FPS*(1+rng.range(3) as u64));
        }
        if self.inside_coop {
            if !matches!(phase,Phase::Dawn|Phase::Day) {
                self.release_at=None; // Dusk/night always wins over a queued release.
                return;
            }
            let at=*self.release_at.get_or_insert_with(||
                tick+coop::morning_release_delay(self.id));
            if tick<at {return;}
            self.inside_coop=false;
            self.release_at=None;
            // Emerge at the physical doorway, never through the walls.
            if habitat.pixel_mode() {
                self.pos=habitat.at_feet(habitat.coop.front_approach(),self.species);
            } else {
                self.pos=Point{x:habitat.coop.door.x-8-(self.id%4) as i32*2,
                    y:self.home.y};
            }
            self.route=None;
            self.home_approached=false;
            self.pending_perch=None;
            self.perch_at=None;
            self.pending_scratch=false;
            self.target=None;
            self.activity=Activity::Pause;
            self.until=tick+FPS*(5+rng.range(10) as u64);
            self.next_step=tick+FPS; // Do not immediately walk back into the door.
            self.facing=Facing::Left;
        }
        if let Some(antic)=self.antic {
            if antic.active(tick) {
                if antic.moves_at(tick) {
                    // Dash/flutter strides can be 2–3 pixels: resolve each
                    // substep so a narrow billboard leg cannot be jumped.
                    let stride=antic.stride();
                    for _ in 0..stride.abs() {
                        let before=self.pos;
                        self.pos.x+=stride.signum();
                        self.keep_on_screen(width,height,if habitat.pixel_mode() {
                            DisplayMode::Pixel } else {DisplayMode::Ascii});
                        self.pos=habitat.safe_step(before,self.pos,self.species,false);
                    }
                }
                return;
            }
            self.antic=None;
            self.until=tick+FPS/2;
        }
        if tick>=self.until {
            if matches!(self.activity,Activity::Perch|Activity::Scratch) {
                // Land fully, then walk out toward the normal depth band;
                // never snap out of the air or across the coop footprint.
                self.perch_at=None;
                self.pending_perch=None;
                self.pending_scratch=false;
                self.activity=Activity::Investigate;
                self.target=Some(Point{x:self.pos.x,y:self.band.clamp(self.pos.y)});
                self.interest=None;
                self.route=None;
                self.until=tick+FPS*25;
            } else {
                self.choose_activity(tick,phase,hour,neighbors,habitat,roof_free,rng);
            }
        }
        if tick>=self.next_step {
            let before_step=self.pos;
            self.next_step=tick+match (self.species,self.activity) {
                (_,Activity::GoHome)=>3,
                (Species::GuineaFowl|Species::GuineaChick,_)=>4,
                (Species::Chicken,_)=>8,
                (Species::Shepherd,_)=>8,
                (Species::Cat,_)=>10,
            };
            // The guineas move together. Rejoining the flock does not erase a
            // character's personality or interrupt going home at dusk.
            let social=if phase!=Phase::Night &&
                matches!(self.activity,Activity::Wander|Activity::Huddle) {
                flock::rejoin(self.id,self.species,self.pos,neighbors,self.activity==Activity::Huddle)
            } else {None};
            if let Some(target)=social {
                self.pos=habitat.route_step(before_step,target,self.species,&mut self.route,false);
            } else {
                match self.activity {
                    Activity::Wander|Activity::Stalk => {
                        let moved=Point {x:before_step.x+if self.facing==Facing::Left {-1} else {1},
                            y:before_step.y};
                        self.pos=habitat.safe_step(before_step,moved,self.species,false);
                        // Free roaming turns at a wall; destination-based animals
                        // instead use actual walk-around routing below.
                        if self.pos.x==before_step.x {
                            self.facing=if self.facing==Facing::Left {Facing::Right} else {Facing::Left};
                        }
                    }
                    Activity::Investigate|Activity::GoHome => {
                        if let Some(target)=self.target {
                            let homeward=self.activity==Activity::GoHome && self.bird();
                            let mut next=target;
                            if homeward && habitat.pixel_mode() && !self.home_approached {
                                next=habitat.at_feet(habitat.coop.front_approach(),self.species);
                            }
                            let entering=homeward && self.home_approached;
                            self.pos=habitat.route_step(before_step,next,self.species,
                                &mut self.route,entering);
                            if self.pos.x<before_step.x {self.facing=Facing::Left;}
                            if self.pos.x>before_step.x {self.facing=Facing::Right;}
                            if homeward && !self.home_approached &&
                                (self.pos.x-next.x).abs()<=3 && (self.pos.y-next.y).abs()<=3 {
                                self.home_approached=true;
                                self.route=None;
                            }
                            let final_target=if homeward {target} else {next};
                            if (!homeward || self.home_approached) &&
                                (self.pos.x-final_target.x).abs()<=3 &&
                                (self.pos.y-final_target.y).abs()<=3 {
                                if homeward {
                                    self.inside_coop=true;
                                    self.release_at=None;
                                    self.activity=Activity::Rest;
                                    self.until=tick+FPS*15;
                                } else if let Some(roof)=self.pending_perch.take() {
                                    self.activity=Activity::Perch;
                                    self.perch_at=Some(roof);
                                    self.perch_started=tick;
                                    self.until=tick+FPS*(9+rng.range(7) as u64);
                                    self.interest=None;
                                } else if self.pending_scratch {
                                    self.pending_scratch=false;
                                    self.activity=Activity::Scratch;
                                    self.until=tick+FPS*(5+rng.range(6) as u64);
                                    self.last_interest=Some(Place::Scratch);
                                    self.interest=None;
                                } else {
                                    self.activity=if self.interest.is_some_and(|p|curiosity::forage_here(self.species,p)) {
                                        Activity::Forage
                                    } else { Activity::Observe };
                                    self.last_interest=self.interest.take();
                                    self.until=tick+FPS*(2+rng.range(4) as u64);
                                }
                                self.target=None;
                                self.route=None;
                            }
                        }
                    }
                    _=>{}
                }
            }
        }
        let (sprite_w,_)=habitat.size(self.species);
        let limit=(width as i32-sprite_w).max(0);
        if self.pos.x<=0 {self.facing=Facing::Right;}
        if self.pos.x>=limit {self.facing=Facing::Left;}
        self.keep_on_screen(width,height,if habitat.pixel_mode() {
            DisplayMode::Pixel } else {DisplayMode::Ascii});
    }

    pub fn pose(&self,tick:u64)->Pose {
        if let Some(a)=self.antic {
            if a.active(tick) {
                return match a.kind {
                    AnticKind::Perk=>Pose::Look,
                    AnticKind::Hop=>Pose::WalkA,
                    AnticKind::Flutter|AnticKind::Dash|AnticKind::Pounce=>
                        if (tick/4)%2==0 {Pose::WalkA} else {Pose::WalkB},
                };
            }
        }
        match self.activity {
            Activity::Wander|Activity::Investigate|Activity::Stalk|Activity::GoHome=>{
                if (tick/9)%2==0 {Pose::WalkA} else {Pose::WalkB}
            }
            Activity::Forage=>Pose::Forage,
            Activity::Alert|Activity::Observe=>Pose::Look,
            Activity::Groom=>Pose::Forage,
            Activity::Scratch=>Pose::Scratch,
            Activity::DustBathe=>Pose::DustBathe,
            Activity::Perch=>Pose::Perch,
            Activity::Hop=>Pose::Hop,
            Activity::Huddle=>Pose::Huddle,
            Activity::Rest=>Pose::Rest,
            Activity::Pause if self.species==Species::Shepherd=>Pose::Forage, // sit
            Activity::Pause=>Pose::Idle,
        }
    }

    fn choose_activity(&mut self,tick:u64,phase:Phase,hour:u8,neighbors:&[Neighbor],
        habitat:&Habitat,roof_free:bool,rng:&mut Rng) {
        self.target=None;
        self.interest=None;
        self.route=None;
        self.home_approached=false;
        self.pending_perch=None;
        self.perch_at=None;
        self.pending_scratch=false;
        if self.bird() && coop::should_roost(phase,hour,rng) {
            self.activity=Activity::GoHome;
            self.target=Some(self.home);
            self.face(self.home);
            self.until=tick+FPS*50;
            return;
        }
        // The shepherd has a four-pose identity, not arbitrary curiosity states.
        if self.species==Species::Shepherd {
            let routine=shepherd::choose(phase,rng);
            self.activity=match routine {
                DogRoutine::Patrol=>Activity::Wander,
                DogRoutine::Sit=>Activity::Pause,
                DogRoutine::Lie=>Activity::Rest,
                DogRoutine::Look=>Activity::Observe,
            };
            if routine==DogRoutine::Patrol && rng.range(2)==0 {
                self.facing=if self.facing==Facing::Left {Facing::Right} else {Facing::Left};
            }
            self.until=tick+FPS*match routine {
                DogRoutine::Patrol=>5+rng.range(5) as u64,
                DogRoutine::Sit=>4+rng.range(5) as u64,
                DogRoutine::Lie=>8+rng.range(10) as u64,
                DogRoutine::Look=>2+rng.range(3) as u64,
            };
            return;
        }
        let routine=routines::choose(self.species,phase,rng);
        // A real coop-roof visit: route to a walkable SIDE approach, then flutter
        // to the foot-level perch anchor. The ground position never goes through
        // the roof footprint; other guineas and herons honor the reservation.
        if routine==Routine::Perch && habitat.pixel_mode() && roof_free {
            if let Some(roof)=habitat.anchor(AnchorKind::CoopRoofLeft) {
                self.pending_perch=Some(habitat.at_feet(roof,self.species));
                self.target=Some(habitat.at_feet(habitat.coop.left_approach(),self.species));
                self.activity=Activity::Investigate;
                self.face(self.target.unwrap());
                self.until=tick+FPS*55;
                return;
            }
        }
        // The cat visits the actual scratch landmark and stays there to scratch,
        // instead of scratching an arbitrary patch of the ground.
        if routine==Routine::Scratch {
            if let Some(scratch)=habitat.landmarks.iter().find(|l|l.place==Place::Scratch) {
                self.target=Some(habitat.at_feet(scratch.point,self.species));
                self.pending_scratch=true;
                self.activity=Activity::Investigate;
                self.face(self.target.unwrap());
                self.until=tick+FPS*55;
                return;
            }
        }
        // Social attention stays gentle and guineas are handled by flock.rs.
        if routine==Routine::Stalk && phase!=Phase::Night {
            let interest=if self.species==Species::Cat {
                Some(Species::Chicken)
            } else {None};
            if let Some(kind)=interest {
                if let Some(n)=neighbors.iter().filter(|n|n.species==kind && n.id!=self.id)
                    .min_by_key(|n| (n.pos.x-self.pos.x).abs()+(n.pos.y-self.pos.y).abs()) {
                    let d=(n.pos.x-self.pos.x).abs()+(n.pos.y-self.pos.y).abs();
                    if d<38 && rng.range(5)==0 {
                        self.target=Some(n.pos);
                        self.face(n.pos);
                        self.activity=if d<12 {Activity::Observe} else {Activity::Investigate};
                        self.until=tick+FPS*(2+rng.range(4) as u64);
                        return;
                    }
                }
            }
        }
        if routine==Routine::Explore {
            if let Some(interest)=curiosity::choose(self.species,self.pos,habitat,self.last_interest,rng) {
                // Curiosity landmarks represent FEET in the shared world.
                // Rock/pine/clearing visits now use their real anchors.
                let feet=if !habitat.pixel_mode() {interest.point} else {match interest.place {
                    Place::Rock=>habitat.anchor(AnchorKind::RockSide).unwrap_or(interest.point),
                    Place::Pine=>habitat.anchors.iter().filter(|a|a.kind==AnchorKind::TreeBase)
                        .min_by_key(|a|(a.point.x-interest.point.x).abs())
                        .map(|a|a.point).unwrap_or(interest.point),
                    Place::Clearing=>habitat.anchor(AnchorKind::Clearing).unwrap_or(interest.point),
                    Place::Scratch=>interest.point,
                }};
                self.target=Some(habitat.at_feet(feet,self.species));
                self.interest=Some(interest.place);
                self.activity=Activity::Investigate;
                self.face(interest.point);
                self.until=tick+FPS*40;
                return;
            }
        }
        self.activity=match routine {
            Routine::Roam|Routine::Explore=>Activity::Wander,
            Routine::Forage=>Activity::Forage,
            Routine::Pause=>Activity::Pause,
            Routine::Watch=>Activity::Observe,
            Routine::Rest=>Activity::Rest,
            Routine::GoHome=>Activity::GoHome,
            Routine::Stalk=>Activity::Stalk,
            Routine::Groom=>Activity::Groom,
            Routine::Scratch=>Activity::Scratch, // fallback if no scratch landmark
            Routine::DustBathe=>Activity::DustBathe,
            Routine::Perch=>Activity::Alert, // fallback when roof is occupied / ASCII
            Routine::Hop=>Activity::Hop,
            Routine::Huddle=>Activity::Huddle,
        };
        if self.activity==Activity::GoHome {
            self.target=Some(self.home);
            self.until=tick+FPS*50;
            return;
        }
        if matches!(self.activity,Activity::Wander|Activity::Stalk) && rng.range(2)==0 {
            self.facing=if self.facing==Facing::Left {Facing::Right} else {Facing::Left};
        }
        self.until=tick+FPS*(match self.activity {
            Activity::Stalk=>5, Activity::DustBathe=>5, Activity::Groom=>5,
            Activity::Huddle=>5, Activity::Hop=>2, _=>2,
        }+rng.range(if self.activity==Activity::Rest {12} else {5}) as u64);
    }
}
