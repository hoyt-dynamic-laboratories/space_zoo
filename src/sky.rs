//! Watch-dial sky: independent geographic Sun/Moon clocks and artistic orb paths.
//! Real planets are a separate ephemeris/camera layer; no visitor scheduler lives here.
use std::time::{SystemTime, UNIX_EPOCH, Instant};
use crate::astronomy::{Location,SolarCache};
use crate::lunar_astronomy::LunarCache;
use crate::display_settings::DisplaySettings;
use crate::world::{Point, FPS};

const SYNODIC_MONTH_DAYS:f64=29.53058867;
const SECS_PER_DAY:f64=86_400.0;
// 2000-01-06 18:14 UTC: observed new moon, not an arbitrary Unix epoch.
const KNOWN_NEW_MOON_UNIX:f64=947_182_440.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanetKind {Mercury,Venus,Earth,Mars,Jupiter,Saturn,Uranus,Neptune,Pluto}
impl PlanetKind {
    /// Complete data-model order: all eight planets plus Pluto. Earth remains
    /// the observer body and therefore has no sky sprite; lookup returns ObserverBody.
    pub const ALL:[PlanetKind;9]=[PlanetKind::Mercury,PlanetKind::Venus,PlanetKind::Earth,
        PlanetKind::Mars,PlanetKind::Jupiter,PlanetKind::Saturn,PlanetKind::Uranus,
        PlanetKind::Neptune,PlanetKind::Pluto];
    /// Sky-body order excludes the body under the observer's feet.
    pub const SKY_ORDER:[PlanetKind;8]=[PlanetKind::Mercury,PlanetKind::Venus,PlanetKind::Mars,
        PlanetKind::Jupiter,PlanetKind::Saturn,PlanetKind::Uranus,PlanetKind::Neptune,PlanetKind::Pluto];
    pub fn parse(name:&str)->Option<Self>{match name.trim().to_ascii_lowercase().as_str(){
        "mercury"=>Some(Self::Mercury),"venus"=>Some(Self::Venus),"earth"=>Some(Self::Earth),
        "mars"=>Some(Self::Mars),"jupiter"=>Some(Self::Jupiter),"saturn"=>Some(Self::Saturn),
        "uranus"=>Some(Self::Uranus),"neptune"=>Some(Self::Neptune),"pluto"=>Some(Self::Pluto),_=>None}}
    pub fn name(self)->&'static str{match self{Self::Mercury=>"mercury",Self::Venus=>"venus",Self::Earth=>"earth",
        Self::Mars=>"mars",Self::Jupiter=>"jupiter",Self::Saturn=>"saturn",Self::Uranus=>"uranus",
        Self::Neptune=>"neptune",Self::Pluto=>"pluto"}}
    pub fn radius(self)->i32{match self {Self::Mercury|Self::Mars|Self::Pluto=>4,Self::Jupiter=>7,Self::Saturn=>6,_=>5}}
    pub fn color(self)->u32{match self{Self::Mercury=>0xb9a48b,Self::Venus=>0xe7cf87,Self::Earth=>0x5e91d8,Self::Mars=>0xc96a4e,
        Self::Jupiter=>0xd6b178,Self::Saturn=>0xe6cf8b,Self::Uranus=>0x9ed6ca,Self::Neptune=>0x5f85d9,Self::Pluto=>0xc9b8a1}}
    pub fn accent(self)->u32{match self{Self::Mercury=>0x7a6a5c,Self::Venus=>0xf0e7b2,Self::Earth=>0x9fd5ff,Self::Mars=>0xe29b76,
        Self::Jupiter=>0xb88252,Self::Saturn=>0xb38c54,Self::Uranus=>0xd7f5ef,Self::Neptune=>0x9db1f5,Self::Pluto=>0xf5e9d3}}
    pub fn has_ring(self)->bool {self==Self::Saturn}
}
#[derive(Clone,Copy,Debug)]
pub struct PlanetVisual {
    pub kind:PlanetKind,pub center:Point,pub radius:i32,pub color:u32,pub accent:u32,pub ring:bool
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Orb {Sun,Moon}
#[derive(Clone,Copy,Debug)]
pub struct CelestialVisual {pub kind:Orb,pub center:Point,pub radius:i32}

// The unconfigured legacy display uses a fixed 24-hour artistic wheel. The
// configured Sun and Moon use independent rise/set clocks on that same path.
// Both travel LEFT -> RIGHT; no eclipse logic in this revision.
// The deliberately eccentric cam-shaped sweep follows the red design sketch:
// high on the left, below the window titlebar, then curling down to the right
// of the coop. This is composed art, NOT an astronomical orbit calculation.
const SUN_START:u32 = 6*3600;
const MOON_START:u32 = 18*3600;
const HALF_WHEEL:u32 = 12*3600;

fn position(progress:f32,kind:Orb,width:u16,height:u16)->CelestialVisual {
    let p=progress.clamp(0.0,1.0);
    let radius=if kind==Orb::Sun {11}else{10};
    let travel=width as f32+2.0*(radius+2) as f32;
    let x=-(radius+2) as f32+travel*p;
    // One continuous monotone cam curve, identical for both bodies.
    // Scene proportions at 320x200: left ~11, center ~30, right ~121.
    // Staying high until the right third clears the suspended sign and coop.
    let y=height as f32*(0.055+0.200*p*p+0.350*p.powi(3));
    CelestialVisual {kind,center:Point{x:x.round() as i32,y:y.round() as i32},radius}
}
pub fn position_on_sun_arc(progress:f32,width:u16,height:u16)->Point {
    position(progress,Orb::Sun,width,height).center
}
pub fn celestial_pair(seconds_today:u32,width:u16,height:u16)->[Option<CelestialVisual>;2] {
    let t=seconds_today%86400;
    // 12h sun and 12h moon, separated by exactly half of the 24h wheel.
    if (SUN_START..MOON_START).contains(&t) {
        let p=(t-SUN_START) as f32/HALF_WHEEL as f32;
        [Some(position(p,Orb::Sun,width,height)),None]
    } else {
        let elapsed=if t>=MOON_START {t-MOON_START} else {t+86400-MOON_START};
        let p=elapsed as f32/HALF_WHEEL as f32;
        [None,Some(position(p,Orb::Moon,width,height))]
    }
}

// Night->day->night palette as the hands pass the horizon.
fn ramp(t:f32)->f32 {let t=t.clamp(0.0,1.0);t*t*(3.0-2.0*t)}
pub fn daylight_strength(seconds_today:u32)->f32 {
    // Dawn starts at 05:30; dusk completes by 19:30.
    match seconds_today {
        0..=19799=>0.0,
        19800..=27000=>ramp((seconds_today-19800) as f32/7200.0),
        27001..=63000=>1.0,
        63001..=70200=>1.0-ramp((seconds_today-63000) as f32/7200.0),
        _=>0.0,
    }
}

/// Rendering policy derived from one authoritative astronomical input: the
/// Sun's geometric altitude at the current simulated UTC and observer.
/// Camera orientation is deliberately absent from this state.
#[derive(Clone,Copy,Debug,PartialEq)]
pub struct SkyLightState {
    pub sun_altitude_deg:f64,
    /// None means the physical star-dot layer is suppressed. Otherwise only
    /// native Tycho VT values strictly brighter than this limit are rendered.
    pub visible_limiting_magnitude:Option<f64>,
    /// Constellations remain a navigation/map overlay even in daylight.
    pub constellation_visibility:f64,
}
impl SkyLightState {
    pub fn from_sun_altitude(sun_altitude_deg:f64)->Self {
        let alt=if sun_altitude_deg.is_finite(){sun_altitude_deg}else{0.0};
        let visible_limiting_magnitude=if alt>=0.0 {None}
        else if alt>=-6.0 {Some(2.0+(-alt/6.0)*1.5)}
        else if alt>=-12.0 {Some(3.5+((-alt-6.0)/6.0)*2.5)}
        else if alt>-18.0 {Some(6.0+((-alt-12.0)/6.0)*2.0)}
        else {Some(8.0)};
        let darkness=(-alt/18.0).clamp(0.0,1.0);
        // Keep a deliberately faint 18% map overlay in full daylight, then
        // restore the accepted subdued constellation palette continuously.
        let constellation_visibility=0.18+0.82*darkness;
        Self{sun_altitude_deg:alt,visible_limiting_magnitude,constellation_visibility}
    }
}

pub struct SkyState {
    now_secs:u64, virtual_time:f64, refresh_at:u64,
    solar:Option<SolarCache>, lunar:Option<LunarCache>,
    fast:bool, preview:bool, rate:f64, command_paused:bool,
    location:Option<Location>,last_update:Instant,display:DisplaySettings,
    navigation_cache_dirty:bool,
}
impl SkyState {
    pub fn new()->Self {
        let now=unix_now();
        Self{now_secs:now,virtual_time:now as f64,refresh_at:0,
            solar:None,lunar:None,fast:false,preview:false,rate:0.0,command_paused:false,
            location:None,last_update:Instant::now(),display:DisplaySettings::default(),
            navigation_cache_dirty:false}
    }
    /// Executable startup configures the required geographic location before
    /// any draw. Internal legacy test worlds use the unconfigured art clock.
    pub fn configure(&mut self,place:Location,fast:bool) {
        self.fast=fast;
        self.location=Some(place);
        self.command_paused=false;
        self.preview=false;
        self.rate=if fast {1.0} else {0.0}; // Time Machine launches in N: current time, forward 1x
        self.last_update=Instant::now();
        self.virtual_time=unix_now() as f64;
        self.now_secs=self.virtual_time as u64;
        self.solar=Some(SolarCache::new(place,self.virtual_time));
        self.lunar=Some(LunarCache::new(place,self.virtual_time));
        self.navigation_cache_dirty=false;
    }
    pub fn freeze_preview(&mut self,utc:i64) {
        if self.fast {return;}
        self.preview=true;
        self.virtual_time=utc.max(0) as f64;
        self.now_secs=self.virtual_time as u64;
        if let Some(cache)=&mut self.solar {cache.ensure(self.virtual_time);}
        if let Some(cache)=&mut self.lunar {cache.ensure(self.virtual_time);}
    }
    fn sync_fast(&mut self) {
        let current=Instant::now();
        let delta=current.duration_since(self.last_update).as_secs_f64();
        self.last_update=current;
        self.virtual_time+=delta*self.rate; // UI/command overlays never control simulation time; only rate == 0 pauses.
        self.virtual_time=self.virtual_time.clamp(0.0,crate::time_machine::MAX_UNIX as f64);
        self.now_secs=self.virtual_time.floor() as u64;
        if !self.navigation_cache_dirty {
            if let Some(cache)=&mut self.solar {cache.ensure(self.virtual_time);}
            if let Some(cache)=&mut self.lunar {cache.ensure(self.virtual_time);}
        }
    }
    fn system_time_at(&mut self,utc:u64) {
        self.now_secs=utc;
        self.virtual_time=utc as f64;
        if !self.navigation_cache_dirty {
            if let Some(cache)=&mut self.solar {cache.ensure(self.virtual_time);}
            if let Some(cache)=&mut self.lunar {cache.ensure(self.virtual_time);}
        }
    }
    pub fn advance(&mut self,tick:u64,_hour:u8,_width:u16,_height:u16) {
        if self.fast {
            self.sync_fast();
        } else if !self.preview && tick>=self.refresh_at {
            self.system_time_at(unix_now());
            self.refresh_at=tick+FPS/2;
        }
    }
    pub fn fast(&self)->bool {self.fast}
    pub fn configured(&self)->bool {self.solar.is_some()}
    #[cfg(test)]
    pub fn ff_control(&mut self,key:char) {
        if !self.fast {return;}
        self.sync_fast();
        match key {
            'f'=>self.rate=3600.0,
            'b'=>self.rate=-3600.0,
            ' '=>self.rate=0.0,
            '.'=>self.virtual_time+=900.0,
            '>'=>self.virtual_time+=3600.0,
            ','=>self.virtual_time-=900.0,
            '<'=>self.virtual_time-=3600.0,
            'r'=>{self.virtual_time=unix_now() as f64;self.rate=0.0;},
            _=>{},
        }
        self.virtual_time=self.virtual_time.max(0.0);
        self.now_secs=self.virtual_time.floor() as u64;
        if !self.navigation_cache_dirty {
            if let Some(cache)=&mut self.solar {cache.ensure(self.virtual_time);}
            if let Some(cache)=&mut self.lunar {cache.ensure(self.virtual_time);}
        }
    }
    pub fn rate(&self)->f64 {self.rate}
    pub fn command_paused(&self)->bool {self.command_paused}
    pub fn location(&self)->Option<Location> {self.location}
    /// Rebind geographic-dependent caches without disturbing Time Machine time,
    /// rate, pause state, selected culture or display preferences. Manual movement
    /// keeps this immediate behavior; Gimbal travel uses the deferred variant below.
    pub fn relocate(&mut self,place:Location) {
        if self.location==Some(place) {return;}
        self.location=Some(place);
        self.solar=Some(SolarCache::new(place,self.virtual_time));
        self.lunar=Some(LunarCache::new(place,self.virtual_time));
        self.navigation_cache_dirty=false;
    }
    /// Gimbal-only relocation path: observer coordinates and instantaneous sky math
    /// update immediately, while full daily event caches rebuild once at trip end.
    pub fn relocate_navigation(&mut self,place:Location) {
        if self.location==Some(place) {return;}
        self.location=Some(place);
        self.navigation_cache_dirty=true;
    }
    pub fn finish_navigation_relocation(&mut self) {
        if !self.navigation_cache_dirty {return;}
        if let Some(place)=self.location {
            self.solar=Some(SolarCache::new(place,self.virtual_time));
            self.lunar=Some(LunarCache::new(place,self.virtual_time));
        }
        self.navigation_cache_dirty=false;
    }
    pub fn navigation_cache_dirty(&self)->bool {self.navigation_cache_dirty}
    pub fn begin_navigation_pause(&mut self)->Option<f64> {
        if !self.fast {return None;}
        self.sync_fast();
        let saved=self.rate;
        self.rate=0.0;
        Some(saved)
    }
    pub fn restore_navigation_rate(&mut self,saved_rate:f64) {
        if !self.fast {return;}
        self.sync_fast();
        self.rate=saved_rate;
        self.last_update=Instant::now();
    }

    pub fn star_multiplier(&self)->f64 {self.display.star_multiplier()}
    pub fn set_command_pause(&mut self,paused:bool) {
        if !self.fast{return;}
        self.sync_fast();self.command_paused=paused;
    }
    pub fn command(&mut self,cmd:crate::time_machine::Command)->Result<(),String> {
        use crate::time_machine::Command;
        if !self.fast{return Err("Time Machine is not active".into());}
        // Display-only commands must not advance simulated time merely because the
        // command was entered. Their renderer refresh is driven independently.
        if let Command::Stars(multiplier)=&cmd {
            return self.display.set_star_multiplier(*multiplier);
        }
        self.sync_fast();
        match cmd {
            Command::Forward=>self.rate=crate::time_machine::PRESET_FAST_RATE,
            Command::Backward=>self.rate=-crate::time_machine::PRESET_FAST_RATE,
            Command::Stop=>self.rate=0.0,
            Command::Normal=>{self.virtual_time=unix_now() as f64;self.rate=1.0;},
            Command::NormalBackward=>self.rate=-1.0,
            Command::Rate(rate)=>{
                if !rate.is_finite() || (rate!=0.0 && !(0.000001..=100_000_000.0).contains(&rate.abs())) {
                    return Err("speed must be 0x or have magnitude between 0.000001x and 100000000x".into());
                }
                self.rate=if rate==0.0 {0.0} else {rate};
            }
            Command::ReverseRate(factor)=>{
                let result=-factor;
                if !result.is_finite() || result.abs()<0.000001 || result.abs()>100_000_000.0 {
                    return Err("resulting speed must be between 0.000001x and 100000000x".into());
                }
                self.rate=result;
            }
            Command::Date(utc)=>{
                if utc>crate::time_machine::MAX_UNIX{return Err("date out of range".into());}
                self.virtual_time=utc as f64;
            }
            Command::TimeOfDay(seconds)=>{
                debug_assert!(seconds<86_400);
                let start_of_day=self.now_secs/86_400*86_400;
                let utc=start_of_day.saturating_add(seconds as u64);
                if utc>crate::time_machine::MAX_UNIX{return Err("time out of range".into());}
                self.virtual_time=utc as f64;
            }
            Command::Location(place)=>{
                self.location=Some(place);
                self.solar=Some(SolarCache::new(place,self.virtual_time));
                self.lunar=Some(LunarCache::new(place,self.virtual_time));
                self.navigation_cache_dirty=false;
            }
            Command::Stars(_)=>unreachable!("display command returned before sky synchronization"),
            Command::Culture(_) | Command::CultureQuery | Command::Cultures | Command::DebugStars(_) |
                Command::DebugStarsQuery | Command::Planet(_) | Command::ListPlanets | Command::Help |
                Command::StarsQuery | Command::Cryptids(_) | Command::PlanetAppearance(_) |
                Command::Jumbotron(_) | Command::Hud(_) | Command::Chickens(_) | Command::Named(_) | Command::CelestialPlaceholder =>
                return Err("visual command requires pixel renderer".into()),
            Command::Quit=>return Err("quit is handled by the interface".into()),
        }
        self.now_secs=self.virtual_time.floor() as u64;
        if !self.navigation_cache_dirty {
            if let Some(cache)=&mut self.solar{cache.ensure(self.virtual_time);}
            if let Some(cache)=&mut self.lunar{cache.ensure(self.virtual_time);}
        }
        Ok(())
    }
    pub fn solar_daylight(&self,local_seconds:u32)->f32 {
        if self.navigation_cache_dirty {
            let Some(alt)=self.solar_altitude() else{return daylight_strength(local_seconds);};
            let p=((alt+18.0)/24.0).clamp(0.0,1.0) as f32;
            return p*p*(3.0-2.0*p);
        }
        self.solar.as_ref().map_or_else(||daylight_strength(local_seconds),
            |cache|cache.daylight(self.virtual_time))
    }
    pub fn solar_altitude(&self)->Option<f64> {
        if self.navigation_cache_dirty {
            return self.location.map(|place|crate::astronomy::altitude(self.virtual_time,place));
        }
        self.solar.as_ref().map(|cache|cache.altitude_at(self.virtual_time))
    }
    pub fn sky_light_state(&self)->SkyLightState {
        // Configured pixel/Time-Machine paths always have SolarCache. The
        // fallback keeps internal legacy test worlds deterministic without
        // inventing a camera-dependent lighting state.
        SkyLightState::from_sun_altitude(self.solar_altitude().unwrap_or(-18.0))
    }
    pub fn solar_phase(&self)->crate::circadian::Phase {
        use crate::circadian::Phase;
        let Some(cache)=&self.solar else {return Phase::at((self.now_secs/3600%24) as u8)};
        let alt=self.sky_light_state().sun_altitude_deg;
        if alt>=0.0 {Phase::Day}
        else if alt<=-18.0 {Phase::Night}
        else if self.navigation_cache_dirty {
            let Some(place)=self.location else{return Phase::Dusk;};
            let before=crate::astronomy::altitude((self.virtual_time-60.0).max(0.0),place);
            let after=crate::astronomy::altitude(self.virtual_time+60.0,place);
            if after>=before {Phase::Dawn}else{Phase::Dusk}
        } else if cache.rising(self.virtual_time) {Phase::Dawn}
        else {Phase::Dusk}
    }
    pub fn celestial_pair(&self,local_seconds:u32,width:u16,height:u16)->[Option<CelestialVisual>;2] {
        if self.navigation_cache_dirty {
            let Some(place)=self.location else{return celestial_pair(local_seconds,width,height);};
            let sun=crate::astronomy::progress_now(self.virtual_time,place)
                .map(|p|position(p,Orb::Sun,width,height));
            let moon=crate::lunar_astronomy::progress_now(self.virtual_time,place)
                .map(|p|position(p,Orb::Moon,width,height));
            return [sun,moon];
        }
        let Some(cache)=&self.solar else {return celestial_pair(local_seconds,width,height)};
        let sun=cache.sun_progress(self.virtual_time).map(|p|position(p,Orb::Sun,width,height));
        let moon=self.lunar.as_ref().and_then(|lunar|lunar.progress(self.virtual_time))
            .map(|p|position(p,Orb::Moon,width,height));
        [sun,moon]
    }
    pub fn now_secs(&self)->u64{self.now_secs}
    pub fn moon_phase(&self)->f32 {
        Self::phase_at(self.now_secs)
    }
    pub fn phase_at(unix_seconds:u64)->f32 {
        let days=(unix_seconds as f64-KNOWN_NEW_MOON_UNIX)/SECS_PER_DAY;
        (days/SYNODIC_MONTH_DAYS).rem_euclid(1.0) as f32
    }
}
fn unix_now()->u64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn system_clock_jump_and_suspend_resume_recover_at_next_sample() {
        let mut sky=SkyState::new();
        sky.configure(Location::new(40.0,-105.0).unwrap(),false);
        let base=1_797_000_000u64;
        sky.system_time_at(base);
        sky.system_time_at(base+40*86400); // forwards 40 days
        assert_eq!(sky.now_secs(),base+40*86400);
        assert_eq!(sky.solar.as_ref().unwrap().center,
            crate::astronomy::solar_day_id(sky.virtual_time,-105.0));
        sky.system_time_at(base-20*86400); // backwards 20 days
        assert_eq!(sky.now_secs(),base-20*86400);
        // Simulates a long suspend: no reliance on accumulated frame ticks.
        sky.refresh_at=0;
        sky.advance(1,0,320,200);
        assert!((sky.now_secs() as i64-unix_now() as i64).abs()<3);
    }
    #[test] fn time_machine_speed_direction_stop_pause_and_utc_jump() {
        use crate::time_machine::Command;
        let mut sky=SkyState::new();
        let place=Location::new(40.123,-105.456).unwrap();
        sky.configure(place,true);
        assert_eq!(sky.rate(),1.0);
        sky.set_command_pause(true);
        sky.command(Command::Forward).unwrap();
        assert_eq!(sky.rate(),3600.0);
        sky.command(Command::Rate(3.0)).unwrap();
        assert_eq!(sky.rate(),3.0);
        sky.command(Command::Backward).unwrap();
        assert_eq!(sky.rate(),-3600.0);
        sky.command(Command::Stop).unwrap();
        assert_eq!(sky.rate(),0.0);
        sky.command(Command::Rate(0.5)).unwrap();
        assert_eq!(sky.rate(),0.5);
        sky.command(Command::ReverseRate(2.0)).unwrap();
        assert_eq!(sky.rate(),-2.0);
        sky.command(Command::Date(1_798_223_400)).unwrap();
        assert_eq!(sky.now_secs(),1_798_223_400);
        assert_eq!(sky.rate(),-2.0);
        sky.command(Command::TimeOfDay(1*3600+2*60+3)).unwrap();
        assert_eq!(sky.now_secs()%86_400,3723);
        assert_eq!(sky.rate(),-2.0);
        let new=Location::new(-33.8688,151.2093).unwrap();
        sky.command(Command::Location(new)).unwrap();
        assert_eq!(sky.location(),Some(new));
        assert_eq!(sky.solar.as_ref().unwrap().center,
            crate::astronomy::solar_day_id(sky.virtual_time,151.2093));
        assert_eq!(sky.lunar.as_ref().unwrap().center,
            (sky.virtual_time/crate::astronomy::DAY).floor() as i64);
        sky.command(Command::Normal).unwrap();
        assert_eq!(sky.rate(),1.0);
        assert!((sky.now_secs() as i64-unix_now() as i64).abs()<3);
        sky.command(Command::NormalBackward).unwrap();assert_eq!(sky.rate(),-1.0);
        assert!(sky.command(Command::Rate(1e12)).is_err());
        assert_eq!(sky.rate(),-1.0);
        sky.set_command_pause(false);
        assert!(!sky.command_paused());
    }
    #[test] fn time_machine_speed_multiples_are_absolute_from_one_x() {
        use crate::time_machine::Command;
        let mut sky=SkyState::new();
        sky.configure(Location::new(40.0,-105.0).unwrap(),true);
        sky.command(Command::Rate(2.0)).unwrap();
        assert_eq!(sky.rate(),2.0);
        sky.command(Command::Rate(4.0)).unwrap();
        assert_eq!(sky.rate(),4.0,"forward multiples must not compound");
        sky.command(Command::ReverseRate(2.0)).unwrap();
        assert_eq!(sky.rate(),-2.0);
        sky.command(Command::ReverseRate(4.0)).unwrap();
        assert_eq!(sky.rate(),-4.0,"reverse multiples must not compound");
        sky.command(Command::Rate(0.5)).unwrap();
        assert_eq!(sky.rate(),0.5,"forward command is always relative to 1x");
    }
    #[test] fn command_overlay_never_pauses_simulated_time() {
        use crate::time_machine::Command;
        let mut sky=SkyState::new();
        sky.configure(Location::new(40.0,-105.0).unwrap(),true);
        sky.command(Command::Forward).unwrap();
        sky.set_command_pause(true);
        let before=sky.virtual_time;
        sky.last_update=Instant::now()-std::time::Duration::from_secs(1);
        sky.advance(0,0,320,200);
        assert!(sky.virtual_time-before>3500.0,
            "opening/editing the command UI must not freeze simulated time");
        assert!(sky.command_paused(),"UI state may remain tracked without affecting time integration");
    }
    #[test] fn fast_forward_steps_across_multiple_solar_days() {
        let mut sky=SkyState::new();
        sky.configure(Location::new(40.123,-105.456).unwrap(),true);
        sky.ff_control(' ');
        let start=sky.now_secs();
        for _ in 0..72 {sky.ff_control('>');}
        assert!((sky.now_secs() as i64-start as i64-72*3600).abs()<2);
        assert!(sky.solar.is_some());
        assert_eq!(sky.solar.as_ref().unwrap().center,
            crate::astronomy::solar_day_id(sky.virtual_time,-105.456));
        sky.ff_control('<');
        assert!((sky.now_secs() as i64-start as i64-71*3600).abs()<2);
        sky.ff_control('r');
        assert!((sky.now_secs() as i64-unix_now() as i64).abs()<2);
        sky.ff_control('b');assert!(sky.rate<0.0);
        sky.ff_control('f');assert!(sky.rate>0.0);
    }
    #[test] fn navigation_defers_daily_caches_but_final_rebuild_matches_fresh_state() {
        use crate::time_machine::Command;
        let start=Location::new(39.7392,-104.9903).unwrap();
        let middle=Location::new(20.0,-40.0).unwrap();
        let target=Location::new(-33.8688,151.2093).unwrap();
        let utc=1_700_000_000u64;
        let mut moving=SkyState::new();
        moving.configure(start,true);
        // Freeze before setting the deterministic test timestamp. `command`
        // synchronizes elapsed wall time before applying a rate change, so
        // stopping after `Date` would legitimately advance the clock slightly.
        moving.command(Command::Rate(0.0)).unwrap();
        moving.command(Command::Date(utc)).unwrap();
        moving.relocate_navigation(middle);
        assert!(moving.navigation_cache_dirty());
        let middle_alt=moving.solar_altitude().unwrap();
        assert!((middle_alt-crate::astronomy::altitude(utc as f64,middle)).abs()<1e-9,
            "moving observer must use current-position solar geometry while caches are deferred");
        moving.relocate_navigation(target);
        assert!(moving.navigation_cache_dirty());
        moving.finish_navigation_relocation();
        assert!(!moving.navigation_cache_dirty());

        let mut fresh=SkyState::new();
        fresh.configure(target,true);
        fresh.command(Command::Rate(0.0)).unwrap();
        fresh.command(Command::Date(utc)).unwrap();
        assert!((moving.solar_altitude().unwrap()-fresh.solar_altitude().unwrap()).abs()<1e-12);
        assert_eq!(moving.solar_phase(),fresh.solar_phase());
        let visual_key=|visual:Option<CelestialVisual>| visual.map(|v|(v.kind,v.center.x,v.center.y,v.radius));
        let moving_pair=moving.celestial_pair(0,320,200);
        let fresh_pair=fresh.celestial_pair(0,320,200);
        assert_eq!(visual_key(moving_pair[0]),visual_key(fresh_pair[0]));
        assert_eq!(visual_key(moving_pair[1]),visual_key(fresh_pair[1]));
        let moving_sun=moving.solar.as_ref().unwrap().sunrise_sunset();
        let fresh_sun=fresh.solar.as_ref().unwrap().sunrise_sunset();
        assert_eq!(moving_sun.rise,fresh_sun.rise);
        assert_eq!(moving_sun.set,fresh_sun.set);
        let moving_moon=moving.lunar.as_ref().unwrap().events_today();
        let fresh_moon=fresh.lunar.as_ref().unwrap().events_today();
        assert_eq!(moving_moon.len(),fresh_moon.len());
        for (a,b) in moving_moon.iter().zip(fresh_moon) {
            assert_eq!(a.utc,b.utc);
            assert_eq!(a.kind,b.kind);
        }
    }

    #[test] fn independent_lunar_horizon_clock_and_simulated_utc() {
        let p=Location::new(39.7392,-104.9903).unwrap();
        let mut sky=SkyState::new();
        sky.configure(p,true);
        sky.ff_control(' '); // freeze simulated clock; no wall-time drift
        for (utc,sun_expected,moon_expected) in [
            (1_792_000_800, true, true),  // daytime, both
            (1_791_993_600, true, false), // daytime, only Sun
            (1_791_939_600, false, true), // after sunset, only Moon
            (1_791_957_600, false, false),// nighttime, neither
        ] {
            sky.system_time_at(utc);
            let [sun,moon]=sky.celestial_pair(0,320,200);
            assert_eq!(sun.is_some(),sun_expected,"Sun @ {utc}");
            assert_eq!(moon.is_some(),moon_expected,"Moon @ {utc}");
        }
        sky.system_time_at(1_791_998_800); // Oct 14 17:20Z: before rise
        assert!(sky.celestial_pair(0,320,200)[1].is_none());
        sky.ff_control('.'); // +15 simulated minutes, crosses real moonrise
        assert!(sky.celestial_pair(0,320,200)[1].is_some());
        for period in 0..4 {
            let utc=1_792_000_800+period*29*86400;
            sky.system_time_at(utc);
            let phase_before=sky.moon_phase();
            sky.ff_control('>');
            assert_eq!(sky.lunar.as_ref().unwrap().center,
                (sky.virtual_time/crate::astronomy::DAY).floor() as i64);
            assert!(sky.moon_phase()!=phase_before);
            let [_,moon]=sky.celestial_pair(0,320,200);
            assert_eq!(moon.is_some(),sky.lunar.as_ref().unwrap().progress(sky.virtual_time).is_some());
        }
    }
        #[test] fn moon_phase_is_unit_interval(){let p=SkyState::new().moon_phase();assert!((0.0..1.0).contains(&p));}
    #[test] fn real_new_moon_epoch_calibrates_phase(){
        let new=SkyState::phase_at(KNOWN_NEW_MOON_UNIX as u64);
        assert!(new<0.00001);
        let half=(SYNODIC_MONTH_DAYS*SECS_PER_DAY*0.5) as u64;
        let full=SkyState::phase_at(KNOWN_NEW_MOON_UNIX as u64+half);
        assert!((full-0.5).abs()<0.001);
    }
    #[test] fn planet_model_is_eight_planets_plus_pluto_with_earth_as_observer() {
        assert_eq!(PlanetKind::ALL.len(),9);
        assert_eq!(PlanetKind::ALL[2],PlanetKind::Earth);
        assert_eq!(PlanetKind::SKY_ORDER.len(),8);
        assert_eq!(PlanetKind::SKY_ORDER[0],PlanetKind::Mercury);
        assert_eq!(PlanetKind::SKY_ORDER[7],PlanetKind::Pluto);
        assert!(!PlanetKind::SKY_ORDER.contains(&PlanetKind::Earth));
    }
    #[test] fn one_wheel_has_one_visible_orb_at_every_minute(){
        for minute in 0..24*60 {
            let [sun,moon]=celestial_pair(minute*60,320,200);
            assert_ne!(sun.is_some(),moon.is_some());
            assert_eq!(sun.is_some(),(6*60..18*60).contains(&minute));
        }
    }
    #[test] fn both_bodies_travel_same_direction_on_same_curve(){
        for i in 0..11 {
            let p=i as f32/10.0;
            let a=position(p,Orb::Sun,320,200).center;
            let b=position(p,Orb::Moon,320,200).center;
            assert!((a.x-b.x).abs()<=2);
            assert_eq!(a.y,b.y);
            if i>0 {
                let prev=position((i-1) as f32/10.0,Orb::Sun,320,200).center;
                assert!(a.x>prev.x && a.y>=prev.y);
            }
        }
    }
    #[test] fn designed_arc_clears_coop(){
        for i in 0..=1000 {
            let p=i as f32/1000.0;
            let orb=position(p,Orb::Sun,320,200);
            let x=orb.center.x;let y=orb.center.y;let r=orb.radius;
            // The relocated hillside billboard is intentionally foreground;
            // it may occlude the sun/moon as they move behind it. Keep the
            // original celestial trajectory and verify the coop clearance.
            // Actual stepped outline of the coop roof at native scene size.
            let coop=crate::coop::Coop::new(320,200,crate::world::DisplayMode::Pixel);
            let roof_h=(coop.height/3).max(6);
            for row in 0..roof_h {
                let inset=(roof_h-row)*coop.width/(roof_h*3);
                let left=coop.x+inset-2;
                let right=left+coop.width-2*inset+4;
                let closest_x=x.clamp(left,right);
                let dy=y-(coop.y+row);
                assert!((x-closest_x).pow(2)+dy.pow(2)>r*r,
                    "coop roof p={p} x={x} y={y} row={row}");
            }
            // Main walls are lower than the roof; this path stays above both.
            let body_top=coop.y+roof_h;
            let closest_x=x.clamp(coop.x+3,coop.x+coop.width-3);
            if x+r>=coop.x+3 && x-r<coop.x+coop.width-3 {
                assert!((x-closest_x).pow(2)+(y-body_top).pow(2)>r*r,
                    "coop body p={p} x={x} y={y}");
            }
        }
    }
    #[test] fn blue_day_dark_night(){
        assert_eq!(daylight_strength(0),0.0);
        assert_eq!(daylight_strength(12*3600),1.0);
        assert_eq!(daylight_strength(23*3600),0.0);
    }
    #[test] fn star_fade_uses_continuous_twilight_limiting_magnitude() {
        let day=SkyLightState::from_sun_altitude(10.0);
        assert_eq!(day.visible_limiting_magnitude,None);
        assert!((day.constellation_visibility-0.18).abs()<1e-12);
        for (alt,limit) in [(-0.001,2.00025),(-6.0,3.5),(-12.0,6.0),(-18.0,8.0),(-30.0,8.0)] {
            let state=SkyLightState::from_sun_altitude(alt);
            assert!((state.visible_limiting_magnitude.unwrap()-limit).abs()<1e-6,
                "alt={alt} limit={:?}",state.visible_limiting_magnitude);
        }
        assert_eq!(SkyLightState::from_sun_altitude(0.0).visible_limiting_magnitude,None);
        assert!((SkyLightState::from_sun_altitude(-18.0).constellation_visibility-1.0).abs()<1e-12);
    }
    #[test] fn identical_utc_different_observers_can_have_different_sky_light_states() {
        // 2026-03-20 12:00 UTC: near local noon at Greenwich and near local
        // midnight around 170 E. This verifies geography, not host timezone.
        let utc=1_774_008_000.0;
        let greenwich=SolarCache::new(Location::new(0.0,0.0).unwrap(),utc);
        let far_side=SolarCache::new(Location::new(0.0,170.0).unwrap(),utc);
        let lit=SkyLightState::from_sun_altitude(greenwich.altitude_at(utc));
        let dark=SkyLightState::from_sun_altitude(far_side.altitude_at(utc));
        assert!(lit.sun_altitude_deg>80.0 && lit.visible_limiting_magnitude.is_none());
        assert!(dark.sun_altitude_deg< -70.0 && dark.visible_limiting_magnitude==Some(8.0));
    }
    #[test] fn star_brightness_command_does_not_advance_or_change_time_rate() {
        use crate::time_machine::Command;
        let mut sky=SkyState::new();
        sky.configure(Location::new(40.0,-105.0).unwrap(),true);
        sky.command(Command::Stop).unwrap();
        let before_time=sky.virtual_time;
        let before_rate=sky.rate();
        sky.command(Command::Stars(4.0)).unwrap();
        assert_eq!(sky.virtual_time,before_time);
        assert_eq!(sky.rate(),before_rate);
        assert_eq!(sky.star_multiplier(),4.0);
        sky.command(Command::Stars(2.0)).unwrap();
        assert_eq!(sky.star_multiplier(),2.0,"replacement is absolute, never cumulative");
        assert!(sky.command(Command::Stars(0.0)).is_err());
        assert!(sky.command(Command::Stars(-1.0)).is_err());
        assert!(sky.command(Command::Stars(f64::NAN)).is_err());
        assert!(sky.command(Command::Stars(f64::INFINITY)).is_err());
        assert_eq!(sky.star_multiplier(),2.0,"invalid display commands must preserve the active setting");
    }

}
