//! Cached topocentric lunar horizon events, independent of the solar clock.
//!
//! Lunar geocentric longitude, latitude and range: Meeus, *Astronomical
//! Algorithms*, second edition, chapter 47, periodic series (47.A / 47.B).
//! Apparent equatorial coordinates: mean obliquity and leading nutation terms.
//! Observer: WGS84 sea-level ellipsoid, with the actual lunar parallax removed
//! vectorially. Rise/set convention: upper limb on an unobstructed horizon,
//! standard 34 arcminute atmospheric refraction, and a distance-dependent lunar
//! semidiameter. No local elevation, obstruction or meteorological correction.
//!
//! No orbital math in ordinary rendering: 5-minute bracketing and bisection
//! occur only when the UTC-day cache is built or changes (including -ff).
use crate::astronomy::{Location,DAY};

// Periodic coefficients: (D, M, M', F, longitude [microdegrees], range [m]).
// The smallest Meeus terms are retained to avoid systematic timing drift.
const LR:&[(i8,i8,i8,i8,i32,i32)] = &[
 (0,0,1,0,6288774,-20905355), (2,0,-1,0,1274027,-3699111),
 (2,0,0,0,658314,-2955968), (0,0,2,0,213618,-569925),
 (0,1,0,0,-185116,48888), (0,0,0,2,-114332,-3149),
 (2,0,-2,0,58793,246158), (2,-1,-1,0,57066,-152138),
 (2,0,1,0,53322,-170733), (2,-1,0,0,45758,-204586),
 (0,1,-1,0,-40923,-129620), (1,0,0,0,-34720,108743),
 (0,1,1,0,-30383,104755), (2,0,0,-2,15327,10321),
 (0,0,1,2,-12528,0), (0,0,1,-2,10980,79661),
 (4,0,-1,0,10675,-34782), (0,0,3,0,10034,-23210),
 (4,0,-2,0,8548,-21636), (2,1,-1,0,-7888,24208),
 (2,1,0,0,-6766,30824), (1,0,-1,0,-5163,-8379),
 (1,1,0,0,4987,-16675), (2,-1,1,0,4036,-12831),
 (2,0,2,0,3994,-10445), (4,0,0,0,3861,-11650),
 (2,0,-3,0,3665,14403), (0,1,-2,0,-2689,-7003),
 (2,0,-1,2,-2602,0), (2,-1,-2,0,2390,10056),
 (1,0,1,0,-2348,6322), (2,-2,0,0,2236,-9884),
 (0,1,2,0,-2120,5751), (0,2,0,0,-2069,0),
 (2,-2,-1,0,2048,-4950), (2,0,1,-2,-1773,4130),
 (2,0,0,2,-1595,0), (4,-1,-1,0,1215,-3958),
 (0,0,2,2,-1110,0), (3,0,-1,0,-892,3258),
 (2,1,1,0,-810,2616), (4,-1,-2,0,759,-1897),
 (0,2,-1,0,-713,-2117), (2,2,-1,0,-700,2354),
 (2,1,-2,0,691,0), (2,-1,0,-2,596,0),
 (4,0,1,0,549,-1423), (0,0,4,0,537,-1117),
 (4,-1,0,0,520,-1571), (1,0,-2,0,-487,-1739),
 (2,1,0,-2,-399,0), (0,0,2,-2,-381,-4421),
 (1,1,1,0,351,0), (3,0,-2,0,-340,0),
 (4,0,-3,0,330,0), (2,-1,2,0,327,0),
 (0,2,1,0,-323,1165), (1,1,-1,0,299,0),
 (2,0,3,0,294,0), (2,0,-1,-2,0,8752),
];
// Periodic latitude coefficients: (D, M, M', F, latitude [microdegrees]).
const B:&[(i8,i8,i8,i8,i32)] = &[
 (0,0,0,1,5128122), (0,0,1,1,280602),
 (0,0,1,-1,277693), (2,0,0,-1,173237),
 (2,0,-1,1,55413), (2,0,-1,-1,46271),
 (2,0,0,1,32573), (0,0,2,1,17198),
 (2,0,1,-1,9266), (0,0,2,-1,8822),
 (2,-1,0,-1,8216), (2,0,-2,-1,4324),
 (2,0,1,1,4200), (2,1,0,-1,-3359),
 (2,-1,-1,1,2463), (2,-1,0,1,2211),
 (2,-1,-1,-1,2065), (0,1,-1,-1,-1870),
 (4,0,-1,-1,1828), (0,1,0,1,-1794),
 (0,0,0,3,-1749), (0,1,-1,1,-1565),
 (1,0,0,1,-1491), (0,1,1,1,-1475),
 (0,1,1,-1,-1410), (0,1,0,-1,-1344),
 (1,0,0,-1,-1335), (0,0,3,1,1107),
 (4,0,0,-1,1021), (4,0,-1,1,833),
 (0,0,1,-3,777), (4,0,-2,1,671),
 (2,0,0,-3,607), (2,0,2,-1,596),
 (2,-1,1,-1,491), (2,0,-2,1,-451),
 (0,0,3,-1,439), (2,0,2,1,422),
 (2,0,-3,-1,421), (2,1,-1,1,-366),
 (2,1,0,1,-351), (4,0,0,1,331),
 (2,-1,1,1,315), (2,-2,0,-1,302),
 (0,0,1,3,-283), (2,1,1,-1,-229),
 (1,1,0,-1,223), (1,1,0,1,223),
 (0,1,-2,-1,-220), (2,1,-1,-1,-220),
 (1,0,1,1,-185), (2,-1,-2,-1,181),
 (0,1,2,1,-177), (4,0,-2,-1,176),
 (4,-1,-1,-1,166), (1,0,1,-1,-164),
 (4,0,1,-1,132), (1,0,-1,-1,-119),
 (4,-1,0,-1,115), (2,-2,0,1,107),
];

fn rad(degrees:f64)->f64 {degrees.to_radians()}
fn periodic_argument(d:i8,m:i8,mp:i8,f:i8,angles:[f64;4])->f64 {
    d as f64*angles[0]+m as f64*angles[1]
        +mp as f64*angles[2]+f as f64*angles[3]
}
fn eccentricity_factor(m:i8,e:f64)->f64 {
    match m.abs() {1=>e,2=>e*e,_=>1.0}
}

/// Lightweight instantaneous topocentric state used both by the cached event
/// builder and by moving-observer rendering while daily event caches are deferred.
#[derive(Clone,Copy,Debug)]
struct LunarInstant {
    margin_deg:f64,
    topocentric_declination:f64,
    topocentric_hour_angle:f64,
    center_horizon_deg:f64,
}

fn lunar_instant(utc:f64,place:Location)->LunarInstant {
    let jd_ut=2_440_587.5+utc/DAY;
    // UTC->TT: lunar arguments need dynamical time, while Earth rotation uses
    // UT1. A modern ~69 s estimate changes predicted events by seconds, not
    // minutes. UT1-UTC and leap-second forecast remain outside this clock.
    let t=(jd_ut+69.0/DAY-2_451_545.0)/36_525.0;
    let lprime=rad((218.3164477+(481267.88123421+(-0.0015786
        +(1.0/538841.0-t/65194000.0)*t)*t)*t).rem_euclid(360.0));
    let d=rad((297.8501921+(445267.1114034+(-0.0018819
        +(1.0/545868.0-t/113065000.0)*t)*t)*t).rem_euclid(360.0));
    let m=rad((357.5291092+(35999.0502909+(-0.0001536
        +t/24490000.0)*t)*t).rem_euclid(360.0));
    let mp=rad((134.9633964+(477198.8675055+(0.0087414
        +(1.0/69699.9+t/14712000.0)*t)*t)*t).rem_euclid(360.0));
    let f=rad((93.2720950+(483202.0175233+(-0.0036539
        +(-1.0/3526000.0+t/863310000.0)*t)*t)*t).rem_euclid(360.0));
    let a1=rad((119.75+131.849*t).rem_euclid(360.0));
    let a2=rad((53.09+479264.290*t).rem_euclid(360.0));
    let a3=rad((313.45+481266.484*t).rem_euclid(360.0));
    let e=1.0-0.002516*t-0.0000074*t*t;
    let angles=[d,m,mp,f];
    let mut longitude=3958.0*a1.sin()+1962.0*(lprime-f).sin()+318.0*a2.sin();
    let mut range=0.0;
    for &(dd,mm,mpp,ff,coeff_l,coeff_r) in LR {
        let arg=periodic_argument(dd,mm,mpp,ff,angles);
        let factor=eccentricity_factor(mm,e);
        longitude+=factor*coeff_l as f64*arg.sin();
        range+=factor*coeff_r as f64*arg.cos();
    }
    let mut latitude=-2235.0*lprime.sin()+382.0*a3.sin()
        +175.0*(a1-f).sin()+175.0*(a1+f).sin()
        +127.0*(lprime-mp).sin()-115.0*(lprime+mp).sin();
    for &(dd,mm,mpp,ff,coeff_b) in B {
        latitude+=eccentricity_factor(mm,e)*coeff_b as f64
            *periodic_argument(dd,mm,mpp,ff,angles).sin();
    }
    let distance_km=385_000.56+range/1000.0;
    // Leading luni-solar nutation terms: arcseconds to degrees.
    let omega=rad((125.04452-1934.136261*t).rem_euclid(360.0));
    let sun_mean=rad((280.4665+36000.7698*t).rem_euclid(360.0));
    let dpsi=(-17.20*omega.sin()-1.32*(2.0*sun_mean).sin()
        -0.23*(2.0*lprime).sin()+0.21*(2.0*omega).sin())/3600.0;
    let deps=(9.20*omega.cos()+0.57*(2.0*sun_mean).cos()
        +0.10*(2.0*lprime).cos()-0.09*(2.0*omega).cos())/3600.0;
    let eps=rad(23.0+26.0/60.0+21.448/3600.0
        -(46.8150*t+0.00059*t*t-0.001813*t*t*t)/3600.0+deps);
    let lon=lprime+rad(longitude/1_000_000.0+dpsi);
    let lat=rad(latitude/1_000_000.0);
    let (sin_lon,cos_lon)=lon.sin_cos();
    let (sin_lat,cos_lat)=lat.sin_cos();
    let x=cos_lat*cos_lon;
    let y=cos_lat*sin_lon*eps.cos()-sin_lat*eps.sin();
    let z=cos_lat*sin_lon*eps.sin()+sin_lat*eps.cos();
    let right_ascension=y.atan2(x);
    let declination=z.clamp(-1.0,1.0).asin();

    let tut=(jd_ut-2_451_545.0)/36_525.0;
    let gmst=280.46061837+360.98564736629*(jd_ut-2_451_545.0)
        +0.000387933*tut*tut-tut*tut*tut/38_710_000.0;
    let hour_angle=rad((gmst+dpsi*eps.cos()+place.longitude).rem_euclid(360.0))
        -right_ascension;
    // Geocentric lunar vector in km, rotated to the observer meridian.
    let (sh,ch)=hour_angle.sin_cos();
    let (sd,cd)=declination.sin_cos();
    let phi=rad(place.latitude);
    let (sp,cp)=phi.sin_cos();
    const A:f64=6378.137; // WGS84 equatorial radius km
    const ECC_SQ:f64=0.00669437999014;
    let n=A/(1.0-ECC_SQ*sp*sp).sqrt();
    let xx=distance_km*cd*ch-n*cp;
    let yy=distance_km*cd*sh;
    let zz=distance_km*sd-n*(1.0-ECC_SQ)*sp;
    let norm=(xx*xx+yy*yy+zz*zz).sqrt();
    let zenith=xx*cp+zz*sp;
    let center_alt=(zenith/norm).clamp(-1.0,1.0).asin().to_degrees();
    let semidiameter=(1737.4/distance_km).asin().to_degrees();
    LunarInstant {
        margin_deg:center_alt+34.0/60.0+semidiameter,
        topocentric_declination:(zz/norm).clamp(-1.0,1.0).asin(),
        topocentric_hour_angle:yy.atan2(xx),
        center_horizon_deg:-(34.0/60.0+semidiameter),
    }
}

/// Topocentric geometric altitude of the Moon's center minus the adopted
/// upper-limb visibility threshold; zero denotes an unobstructed moonrise/set.
fn horizon_margin(utc:f64,place:Location)->f64 {lunar_instant(utc,place).margin_deg}

/// Stateless current-position fallback used only while a moving observer has
/// intentionally deferred the expensive five-day rise/set cache rebuild.
pub fn progress_now(utc:f64,place:Location)->Option<f32> {
    let instant=lunar_instant(utc,place);
    if instant.margin_deg<0.0 {return None;}
    let latitude=place.latitude.to_radians();
    let decl=instant.topocentric_declination;
    let denominator=latitude.cos()*decl.cos();
    let wrapped=((instant.topocentric_hour_angle+std::f64::consts::PI)
        .rem_euclid(std::f64::consts::TAU))-std::f64::consts::PI;
    if denominator.abs()<1e-12 {
        return Some(((wrapped+std::f64::consts::PI)/std::f64::consts::TAU)
            .clamp(0.0,1.0) as f32);
    }
    let threshold=instant.center_horizon_deg.to_radians();
    let cos_horizon=(threshold.sin()-latitude.sin()*decl.sin())/denominator;
    if cos_horizon>=1.0 {return None;}
    if cos_horizon<=-1.0 {
        return Some(((wrapped+std::f64::consts::PI)/std::f64::consts::TAU)
            .clamp(0.0,1.0) as f32);
    }
    let horizon=cos_horizon.acos();
    if wrapped.abs()>horizon+1e-12 {return None;}
    Some(((wrapped+horizon)/(2.0*horizon)).clamp(0.0,1.0) as f32)
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum EventKind {Rise,Set}
#[derive(Clone,Copy,Debug)]
pub struct LunarEvent {pub utc:f64,pub kind:EventKind}

pub struct LunarDay {
    pub id:i64,
    start_above:bool,
    events:Vec<LunarEvent>,
}
impl LunarDay {
    fn build(id:i64,place:Location)->Self {
        const STEP:f64=300.0;
        let start=id as f64*DAY;
        let mut before=horizon_margin(start,place);
        let start_above=before>=0.0;
        let mut events=Vec::with_capacity(2);
        for i in 1..=(DAY as usize/STEP as usize) {
            let end=start+i as f64*STEP;
            let after=horizon_margin(end,place);
            if (before>=0.0)!=(after>=0.0) {
                let kind=if after>=0.0 {EventKind::Rise} else {EventKind::Set};
                let mut lo=end-STEP;
                let mut hi=end;
                let low_above=before>=0.0;
                // 19 iterations: less than two milliseconds resolution.
                for _ in 0..19 {
                    let mid=(lo+hi)/2.0;
                    if (horizon_margin(mid,place)>=0.0)==low_above {lo=mid;} else {hi=mid;}
                }
                events.push(LunarEvent{utc:(lo+hi)/2.0,kind});
            }
            before=after;
        }
        Self{id,start_above,events}
    }
}

// Five adjacent UTC dates support intervals crossing midnight and the rare
// irregular days with no rise or set. There are no per-frame allocations.
pub struct LunarCache {
    pub center:i64,
    place:Location,
    days:[LunarDay;5],
}
impl LunarCache {
    pub fn new(place:Location,utc:f64)->Self {
        let center=(utc/DAY).floor() as i64;
        Self {center,place,days:std::array::from_fn(|i|LunarDay::build(center+i as i64-2,place))}
    }
    pub fn ensure(&mut self,utc:f64) {
        let id=(utc/DAY).floor() as i64;
        if id==self.center {return;}
        if id==self.center+1 {
            self.days.rotate_left(1);
            self.days[4]=LunarDay::build(id+2,self.place);
        } else if id==self.center-1 {
            self.days.rotate_right(1);
            self.days[0]=LunarDay::build(id-2,self.place);
        } else {
            self.days=std::array::from_fn(|i|LunarDay::build(id+i as i64-2,self.place));
        }
        self.center=id;
    }
    pub fn events_today(&self)->&[LunarEvent] {&self.days[2].events}
    pub fn progress(&self,utc:f64)->Option<f32> {
        let idx=(utc/DAY).floor() as i64-self.center+2;
        if !(0..5).contains(&idx) {return None;}
        let mut above=self.days[idx as usize].start_above;
        for event in &self.days[idx as usize].events {
            if event.utc>utc {break;}
            above=event.kind==EventKind::Rise;
        }
        if !above {return None;}
        let mut last_rise:Option<f64>=None;
        let mut next_set:Option<f64>=None;
        for day in &self.days {
            for event in &day.events {
                if event.utc<=utc && event.kind==EventKind::Rise {
                    last_rise=Some(last_rise.map_or(event.utc,|old|old.max(event.utc)));
                }
                if event.utc>utc && event.kind==EventKind::Set {
                    next_set=Some(next_set.map_or(event.utc,|old|old.min(event.utc)));
                }
            }
        }
        if let (Some(rise),Some(set))=(last_rise,next_set) {
            if set>rise {return Some(((utc-rise)/(set-rise)).clamp(0.0,1.0) as f32);}
        }
        // Continuous polar visibility beyond the available rise/set window:
        // bounded, continuous UTC-anchored artistic bounce. Never fictional
        // events, and visibility still comes from the true topocentric state.
        let phase=(utc/DAY).rem_euclid(1.0);
        Some((1.0-(2.0*phase-1.0).abs()) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn denver()->Location {Location::new(39.7392,-104.9903).unwrap()}
    fn has(cache:&LunarCache,utc:f64)->bool {cache.progress(utc).is_some()}

    #[test]
    fn independent_day_night_horizon_states() {
        // Oct 14 Denver: moonrise 17:29Z, sunset ~00:20Z (Oct 15),
        // moonset Oct 15 at 02:26Z. All four combinations occur.
        let cache=LunarCache::new(denver(),1_791_990_000.0);
        assert!(has(&cache,1_792_000_800.0)); // 18:00Z: daytime Moon
        assert!(!has(&cache,1_791_993_600.0)); // 16:00Z: daytime without Moon
        assert!(has(&cache,1_791_939_600.0)); // 01:00Z: Moon after sunset
        assert!(!has(&cache,1_791_957_600.0)); // 06:00Z: nighttime, Moon down
    }

    #[test]
    fn cross_midnight_traversal_has_real_endpoints_and_midpoint() {
        let rise=1_791_998_938.0; // 2026-10-14 17:28:58Z
        let set=1_792_031_176.0; // 2026-10-15 02:26:16Z
        let cache=LunarCache::new(denver(),rise+60.0);
        let p0=cache.progress(rise+5.0).unwrap();
        let pm=cache.progress((rise+set)*0.5).unwrap();
        let p1=cache.progress(set-5.0).unwrap();
        assert!(p0<0.001 && (pm-0.5).abs()<0.001 && p1>0.999);
        let midnight=1_792_022_400.0;
        let before=cache.progress(midnight-1.0).unwrap();
        let after=cache.progress(midnight+1.0).unwrap();
        assert!((after-before).abs()<0.001 && after>before);
        assert!(!has(&cache,set+30.0));
    }

    #[test]
    fn reference_rise_set_within_three_minutes() {
        // Independent Swiss Ephemeris/Moshier rise_trans reference in UTC,
        // sea level, standard pressure 1013.25 hPa and 15C; upper lunar limb.
        // Expected time is rounded to the nearest minute. No runtime dependency.
        let cases:[(Location,i64,EventKind,f64);8]=[
            (Location{latitude:39.7392,longitude:-104.9903},1_791_936_000,EventKind::Rise,1_791_998_941.0),
            (Location{latitude:39.7392,longitude:-104.9903},1_791_936_000,EventKind::Set,1_791_942_288.0),
            (Location{latitude:51.5,longitude:-0.12},1_773_964_800,EventKind::Rise,1_773_987_397.0),
            (Location{latitude:51.5,longitude:-0.12},1_773_964_800,EventKind::Set,1_774_038_915.0),
            (Location{latitude:-33.87,longitude:151.21},1_782_000_000,EventKind::Rise,1_782_005_100.0),
            (Location{latitude:-33.87,longitude:151.21},1_782_000_000,EventKind::Set,1_782_049_130.0),
            (Location{latitude:70.0,longitude:20.0},1_781_913_600,EventKind::Rise,1_781_943_084.0),
            (Location{latitude:70.0,longitude:20.0},1_781_913_600,EventKind::Set,1_781_994_821.0),
        ];
        for (place,_id,kind,reference) in cases {
            let day=(reference/DAY).floor() as i64;
            let today=LunarDay::build(day,place);
            let actual=today.events.iter().find(|e|e.kind==kind)
                .unwrap_or_else(||panic!("missing {kind:?} at {place:?}"));
            assert!((actual.utc-reference).abs()<180.0,
                "{place:?} {kind:?} delta {} seconds",actual.utc-reference);
        }
    }

    #[test]
    fn polar_continuous_above_below_and_missing_events() {
        let north=Location::new(85.0,0.0).unwrap();
        // During these periods no rising/setting event exists on the UTC day.
        for (utc,expected) in [(1_784_116_800.0,true), // 2026-07-15 noon
                               (1_772_798_400.0,false)] { // 2026-03-06 noon
            let cache=LunarCache::new(north,utc);
            assert!(cache.events_today().is_empty());
            assert_eq!(has(&cache,utc),expected);
            if expected {
                assert!(cache.progress(utc+3600.0).unwrap().is_finite());
            }
        }
    }

    #[test]
    fn polar_edges_have_only_real_transitions() {
        let north=Location::new(85.0,0.0).unwrap();
        for (year_month_day,kind) in [("2026-05-13",EventKind::Rise),
                                       ("2026-09-10",EventKind::Set)] {
            let utc=if year_month_day=="2026-05-13" {1_778_630_400.0}
                else {1_788_998_400.0};
            let day=LunarDay::build((utc/DAY).floor() as i64,north);
            assert_eq!(day.events.len(),1,"{year_month_day}");
            let event=day.events[0];
            assert_eq!(event.kind,kind);
            let cache=LunarCache::new(north,event.utc);
            assert_eq!(has(&cache,event.utc-30.0),kind==EventKind::Set);
            assert_eq!(has(&cache,event.utc+30.0),kind==EventKind::Rise);
        }
    }

    #[test]
    fn multiple_crossings_in_one_utc_day_and_bidirectional_cache_jumps() {
        let p=Location::new(70.0,0.0).unwrap();
        let start=1_781_049_600.0; // 2026-06-10 UTC
        let day=LunarDay::build((start/DAY).floor() as i64,p);
        assert_eq!(day.events.len(),3); // Rise, set, and rise again
        assert_eq!(day.events[0].kind,EventKind::Rise);
        assert_eq!(day.events[1].kind,EventKind::Set);
        assert_eq!(day.events[2].kind,EventKind::Rise);
        let mut cache=LunarCache::new(p,start);
        for days in [1.0,30.0,59.0,88.0,-3.0,2.0] {
            let now=start+DAY*days;
            cache.ensure(now);
            assert_eq!(cache.center,(now/DAY).floor() as i64);
            assert_eq!(cache.days[2].id,cache.center);
            // Deterministic output independent of the direction of time travel.
            let fresh=LunarCache::new(p,now);
            assert_eq!(cache.progress(now),fresh.progress(now));
        }
    }
}
