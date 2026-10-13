//! Watch-dial sky: wall-clock-driven sun/moon arcs and one hourly planet visitor.
//! No astronomical service, per-frame polling, or moving-object simulation.
use std::time::{SystemTime, UNIX_EPOCH};
use crate::world::{Point, FPS};

const SECS_PER_HOUR:u64=3600;
const PLANET_VISIT_SECS:u64=1800; // half hour in, half hour out
const SYNODIC_MONTH_DAYS:f64=29.53058867;
const SECS_PER_DAY:f64=86_400.0;
// 2000-01-06 18:14 UTC: observed new moon, not an arbitrary Unix epoch.
const KNOWN_NEW_MOON_UNIX:f64=947_182_440.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanetKind {Mercury,Venus,Mars,Jupiter,Saturn,Uranus,Neptune}
impl PlanetKind {
    pub const ORDER:[PlanetKind;7]=[PlanetKind::Mercury,PlanetKind::Venus,PlanetKind::Mars,
        PlanetKind::Jupiter,PlanetKind::Saturn,PlanetKind::Uranus,PlanetKind::Neptune];
    pub fn radius(self)->i32{match self {Self::Mercury|Self::Mars=>4,Self::Jupiter=>7,Self::Saturn=>6,_=>5}}
    pub fn color(self)->u32{match self{Self::Mercury=>0xb9a48b,Self::Venus=>0xe7cf87,Self::Mars=>0xc96a4e,
        Self::Jupiter=>0xd6b178,Self::Saturn=>0xe6cf8b,Self::Uranus=>0x9ed6ca,Self::Neptune=>0x5f85d9}}
    pub fn accent(self)->u32{match self{Self::Mercury=>0x7a6a5c,Self::Venus=>0xf0e7b2,Self::Mars=>0xe29b76,
        Self::Jupiter=>0xb88252,Self::Saturn=>0xb38c54,Self::Uranus=>0xd7f5ef,Self::Neptune=>0x9db1f5}}
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

// One 24-hour clockwork wheel: the sun is at angle 0 and the moon at 180 deg.
// Exactly ONE body occupies the visible (upper) sweep at a time. Both travel
// LEFT -> RIGHT along the SAME path. No overlap or eclipse in Rev 0.41.
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

// Rev .4423: the planets descend on the hand-drawn, right-side sky arc.
// A sine perpendicular to the diagonal travel vector gives the alternating
// steep drops and shallow rightward sweeps in the reference sketch. Unlike the
// former horizontal band, the underlying track falls toward the distant hill.
// Keep this independent of the sun/moon cam and the planet visit scheduler.
fn planet_position(progress:f32,radius:i32,width:u16,height:u16)->Point {
    let p=progress.clamp(0.0,1.0);
    let w=width as f32;
    let h=height as f32;
    // Reference at 960x592: enter near (269, -radius), sink near (787, 376).
    let start_x=0.28*w;
    let start_y=-(radius+2) as f32;
    let travel_x=0.54*w;
    let travel_y=0.635*h-start_y;
    let length=(travel_x*travel_x+travel_y*travel_y).sqrt().max(1.0);
    // Four soft lateral waves; no snapping between control points.
    let sway=0.048*h*(4.0*std::f32::consts::TAU*p).sin();
    Point {
        x:(start_x+travel_x*p-travel_y/length*sway).round() as i32,
        y:(start_y+travel_y*p+travel_x/length*sway).round() as i32,
    }
}

pub struct SkyState {now_secs:u64,refresh_at:u64}
impl SkyState {
    pub fn new()->Self{Self{now_secs:unix_now(),refresh_at:0}}
    pub fn advance(&mut self,tick:u64,_hour:u8,_width:u16,_height:u16){
        if tick>=self.refresh_at {self.now_secs=unix_now();self.refresh_at=tick+FPS/2;}
    }
    pub fn now_secs(&self)->u64{self.now_secs}
    pub fn moon_phase(&self)->f32 {
        Self::phase_at(self.now_secs)
    }
    pub fn phase_at(unix_seconds:u64)->f32 {
        let days=(unix_seconds as f64-KNOWN_NEW_MOON_UNIX)/SECS_PER_DAY;
        (days/SYNODIC_MONTH_DAYS).rem_euclid(1.0) as f32
    }
    pub fn active_planet(&self,width:u16,height:u16)->Option<PlanetVisual>{
        Self::planet_at(self.now_secs,width,height)
    }
    pub fn planet_at(now_secs:u64,width:u16,height:u16)->Option<PlanetVisual>{
        let phase=now_secs%SECS_PER_HOUR;
        if phase>=PLANET_VISIT_SECS{return None;}
        let slot=(now_secs/SECS_PER_HOUR) as usize;
        let kind=PlanetKind::ORDER[slot%PlanetKind::ORDER.len()];
        let progress=phase as f32/PLANET_VISIT_SECS as f32;
        let radius=kind.radius();
        let center=planet_position(progress,radius,width,height);
        Some(PlanetVisual{kind,center,radius,color:kind.color(),accent:kind.accent(),ring:kind.has_ring()})
    }
}
fn unix_now()->u64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn moon_phase_is_unit_interval(){let p=SkyState::new().moon_phase();assert!((0.0..1.0).contains(&p));}
    #[test] fn real_new_moon_epoch_calibrates_phase(){
        let new=SkyState::phase_at(KNOWN_NEW_MOON_UNIX as u64);
        assert!(new<0.00001);
        let half=(SYNODIC_MONTH_DAYS*SECS_PER_DAY*0.5) as u64;
        let full=SkyState::phase_at(KNOWN_NEW_MOON_UNIX as u64+half);
        assert!((full-0.5).abs()<0.001);
    }
    #[test] fn planet_visits_for_exactly_half_hour(){
        assert!(SkyState::planet_at(0,320,200).is_some());
        assert!(SkyState::planet_at(1799,320,200).is_some());
        assert!(SkyState::planet_at(1800,320,200).is_none());
        assert!(SkyState::planet_at(3599,320,200).is_none());
        assert!(SkyState::planet_at(3600,320,200).is_some());
    }
    #[test] fn planet_arc_enters_at_top_and_descends_to_far_hill(){
        let r=PlanetKind::Saturn.radius();
        let start=planet_position(0.0,r,320,200);
        let end=planet_position(1.0,r,320,200);
        assert_eq!(start,Point{x:90,y:-r-2});
        assert_eq!(end,Point{x:262,y:127});
        // Across the visit, progress advances to the right (never teleports).
        let mut last_x=start.x;
        for i in 1..=1000 {
            let current=planet_position(i as f32/1000.0,r,320,200);
            assert!(current.x>=last_x,"planet reversed at sample {i}");
            last_x=current.x;
        }
    }
    #[test] fn planet_arc_has_smooth_alternating_sine_sway(){
        let r=PlanetKind::Mars.radius();
        // Quarter of the first wave: +sway; three quarters: -sway.
        // Compare with the straight diagonal baseline at matching progress.
        for (p,sign) in [(1.0/16.0,1),(3.0/16.0,-1)] {
            let point=planet_position(p,r,320,200);
            let baseline_y=-(r+2) as f32+(0.635*200.0+(r+2) as f32)*p;
            let delta=point.y as f32-baseline_y;
            assert!(delta*sign as f32>4.0,"missing alternating planetary sway at {p}");
        }
        // The sine begins and ends exactly at zero, with no discontinuity.
        assert_eq!(planet_position(0.0,r,320,200).x,90);
        assert_eq!(planet_position(1.0,r,320,200).x,262);
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
}
