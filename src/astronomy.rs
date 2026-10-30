//! Lightweight NOAA fractional-year solar equations. UTC only; no ephemeris files.
//! Reference: NOAA Solar Calculation Details / equations of time and declination.
//! Altitude is geometric; conventional -0.833 degree apparent horizon accounts
//! approximately for refraction and solar radius. No terrain/elevation correction.
use std::f64::consts::{PI,TAU};

pub const DAY:f64=86_400.0;
pub const LEVELS:[f64;4]=[-0.833,-6.0,-12.0,-18.0];

#[derive(Clone,Copy,Debug,PartialEq)]
pub struct Location {pub latitude:f64,pub longitude:f64}
impl Location {
    pub fn new(latitude:f64,longitude:f64)->Result<Self,String> {
        if !latitude.is_finite() || !(-90.0..=90.0).contains(&latitude) {
            return Err("latitude must be finite and between -90 and 90".into());
        }
        if !longitude.is_finite() || !(-180.0..=180.0).contains(&longitude) {
            return Err("longitude must be finite and between -180 and 180".into());
        }
        Ok(Self{latitude,longitude})
    }
}

// All days are solar-longitude-shifted UTC intervals around the local solar
// midnight. That keeps sunrise and sunset on the SAME indexed day even at the
// date line, without DST or timezone databases.
pub fn solar_day_id(utc:f64,longitude:f64)->i64 {
    ((utc+longitude*240.0)/DAY).floor() as i64
}

// Civil Gregorian date (Howard Hinnant's civil-from-days algorithm).
fn year_and_ordinal(day:i64)->(i64,i64) {
    fn civil(day:i64)->(i64,i64,i64) {
        let z=day+719468;
        let era=if z>=0 {z} else {z-146096}/146097;
        let doe=z-era*146097;
        let yoe=(doe-doe/1460+doe/36524-doe/146096)/365;
        let mut y=yoe+era*400;
        let doy=doe-(365*yoe+yoe/4-yoe/100);
        let mp=(5*doy+2)/153;
        let d=doy-(153*mp+2)/5+1;
        let m=mp+if mp<10 {3} else {-9};
        if m<=2 {y+=1;}
        (y,m,d)
    }
    let (y,m,d)=civil(day);
    let leap=(y%4==0 && (y%100!=0 || y%400==0)) as i64;
    let cumulative=[0,31,59,90,120,151,181,212,243,273,304,334];
    (y,cumulative[(m-1) as usize]+d+if m>2 {leap} else {0})
}

fn solar_geometry(utc:f64,place:Location)->(f64,f64,f64) {
    let utc_day=(utc/DAY).floor() as i64;
    let (year,ordinal)=year_and_ordinal(utc_day);
    let minute=(utc-utc_day as f64*DAY)/60.0;
    // NOAA uses 366 days for leap years, 365 otherwise.
    let days=if year%4==0 && (year%100!=0 || year%400==0) {366.0} else {365.0};
    let gamma=TAU/days*(ordinal as f64-1.0+(minute/60.0-12.0)/24.0);
    let (s,c)=gamma.sin_cos();
    let (s2,c2)=(2.0*gamma).sin_cos();
    let (s3,c3)=(3.0*gamma).sin_cos();
    let eqtime=229.18*(0.000075+0.001868*c-0.032077*s-0.014615*c2-0.040849*s2);
    let decl=0.006918-0.399912*c+0.070257*s-0.006758*c2+0.000907*s2
        -0.002697*c3+0.00148*s3;
    let hour_angle=((minute+eqtime+4.0*place.longitude)/4.0-180.0).to_radians();
    let latitude=place.latitude*PI/180.0;
    let altitude=(latitude.sin()*decl.sin()+latitude.cos()*decl.cos()*hour_angle.cos())
        .clamp(-1.0,1.0).asin().to_degrees();
    (altitude,decl,hour_angle)
}

pub fn altitude(utc:f64,place:Location)->f64 {solar_geometry(utc,place).0}

/// Stateless current-position fallback used while navigation deliberately defers
/// the expensive daily rise/set caches. This keeps the artistic solar arc tied to
/// the moving observer without rebuilding three full one-minute SolarDay tables.
pub fn progress_now(utc:f64,place:Location)->Option<f32> {
    let (altitude,decl,hour_angle)=solar_geometry(utc,place);
    if altitude<LEVELS[0] {return None;}
    let latitude=place.latitude.to_radians();
    let threshold=LEVELS[0].to_radians();
    let denominator=latitude.cos()*decl.cos();
    let wrapped=((hour_angle+PI).rem_euclid(TAU))-PI;
    if denominator.abs()<1e-12 {
        return Some(((wrapped+PI)/TAU).clamp(0.0,1.0) as f32);
    }
    let cos_horizon=(threshold.sin()-latitude.sin()*decl.sin())/denominator;
    if cos_horizon>=1.0 {return None;}
    if cos_horizon<=-1.0 {return Some(((wrapped+PI)/TAU).clamp(0.0,1.0) as f32);}
    let horizon=cos_horizon.acos();
    if wrapped.abs()>horizon+1e-12 {return None;}
    Some(((wrapped+horizon)/(2.0*horizon)).clamp(0.0,1.0) as f32)
}

#[derive(Clone,Copy,Debug,Default)]
pub struct Crossing {pub rise:Option<f64>,pub set:Option<f64>}

pub struct SolarDay {
    pub id:i64,
    pub start:f64,
    // One-minute table: ~5.6 KiB. Values interpolated at render time.
    samples:[f32;1441],
    pub boundaries:[Crossing;4],
    pub noon:f64,
}
impl SolarDay {
    pub fn build(id:i64,place:Location)->Self {
        let start=id as f64*DAY-place.longitude*240.0;
        let mut samples=[0.0;1441];
        let mut peak=(f64::NEG_INFINITY,720);
        for (i,value) in samples.iter_mut().enumerate() {
            let alt=altitude(start+i as f64*60.0,place);
            *value=alt as f32;
            if alt>peak.0 {peak=(alt,i);}
        }
        let mut boundaries=[Crossing::default();4];
        for (level,crossing) in LEVELS.iter().zip(boundaries.iter_mut()) {
            for i in 1..=1440 {
                let a=samples[i-1] as f64;
                let b=samples[i] as f64;
                let is_rise=a<*level && b>=*level;
                let is_set=a>=*level && b<*level;
                if is_rise || is_set {
                    let fraction=(*level-a)/(b-a);
                    let timestamp=start+((i-1) as f64+fraction)*60.0;
                    if is_rise {crossing.rise=Some(timestamp)}
                    else {crossing.set=Some(timestamp)}
                }
            }
        }
        Self{id,start,samples,boundaries,noon:start+peak.1 as f64*60.0}
    }
    pub fn altitude_at(&self,utc:f64)->f64 {
        let sample=((utc-self.start)/60.0).clamp(0.0,1440.0);
        let i=sample.floor() as usize;
        if i==1440 {return self.samples[1440] as f64;}
        let p=sample-i as f64;
        self.samples[i] as f64*(1.0-p)+self.samples[i+1] as f64*p
    }
}

pub struct SolarCache {
    place:Location,
    pub days:[SolarDay;3],
    pub center:i64,
}
impl SolarCache {
    pub fn new(place:Location,utc:f64)->Self {
        let center=solar_day_id(utc,place.longitude);
        Self{place,days:std::array::from_fn(|i|SolarDay::build(center+i as i64-1,place)),center}
    }
    pub fn ensure(&mut self,utc:f64) {
        let id=solar_day_id(utc,self.place.longitude);
        if id==self.center {return;}
        if id==self.center+1 {
            let next=SolarDay::build(id+1,self.place);
            self.days.rotate_left(1);
            self.days[2]=next;
        } else if id==self.center-1 {
            let previous=SolarDay::build(id-1,self.place);
            self.days.rotate_right(1);
            self.days[0]=previous;
        } else {
            self.days=std::array::from_fn(|i|SolarDay::build(id+i as i64-1,self.place));
        }
        self.center=id;
    }
    pub fn current(&self)->&SolarDay {&self.days[1]}
    pub fn altitude_at(&self,utc:f64)->f64 {self.current().altitude_at(utc)}
    pub fn sunrise_sunset(&self)->Crossing {self.current().boundaries[0]}
    // Twilight palette uses the actual altitude; -18 is full darkness and
    // +6 is fully bright. Smoothstep retains Space Zoo's original gradient.
    pub fn daylight(&self,utc:f64)->f32 {
        let p=((self.altitude_at(utc)+18.0)/24.0).clamp(0.0,1.0) as f32;
        p*p*(3.0-2.0*p)
    }
    pub fn rising(&self,utc:f64)->bool {
        let day=self.current();
        let before=day.altitude_at((utc-60.0).max(day.start));
        let after=day.altitude_at((utc+60.0).min(day.start+DAY));
        after>=before
    }
    pub fn sun_progress(&self,utc:f64)->Option<f32> {
        let day=self.current();
        // Rise/set are defined by -0.833 degrees (upper limb + refraction).
        if day.altitude_at(utc)<LEVELS[0] {return None;}
        let crossings=self.sunrise_sunset();
        if let (Some(rise),Some(set))=(crossings.rise,crossings.set) {
            if set>rise && utc>=rise && utc<=set {
                return Some(((utc-rise)/(set-rise)).clamp(0.0,1.0) as f32);
            }
        }
        // Midnight sun: a forward/reverse traversal without disappearing.
        // For transition days, anchor an available sunrise or sunset to a
        // track endpoint; keep remaining motion bounded and continuous.
        let start=day.start;
        let end=start+DAY;
        let noon=day.noon;
        let progress=match (crossings.rise,crossings.set) {
            (Some(rise),None) if utc>=rise => {
                if utc<=noon { (utc-rise)/(noon-rise).max(1.0) }
                else {1.0-(utc-noon)/(end-noon).max(1.0)}
            }
            (None,Some(set)) if utc<=set => {
                // Last continuous-day sunset must reach the actual endpoint.
                (utc-start)/(set-start).max(1.0)
            }
            _ => {
                let p=((utc-start)/DAY).clamp(0.0,1.0);
                if p<0.5 {2.0*p} else {2.0-2.0*p}
            }
        };
        Some(progress.clamp(0.0,1.0) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn utc(y:i32,m:u32,d:u32,h:u32,min:u32)->f64 {
        // UTC date via a small deterministic day-count search, used in tests only.
        let mut days=0i64;
        for year in 1970..y {days+=if year%4==0&&(year%100!=0||year%400==0){366}else{365};}
        let lengths=[31,28+(y%4==0&&(y%100!=0||y%400==0)) as u32,31,30,31,30,31,31,30,31,30,31];
        for mm in 1..m {days+=lengths[(mm-1) as usize] as i64;}
        (days+(d-1) as i64) as f64*DAY+h as f64*3600.0+min as f64*60.0
    }
    fn clock(ts:f64)->f64 {(ts/DAY).rem_euclid(1.0)*24.0}
    #[test] fn utc_calendar_correct_and_midnight_rollover() {
        assert_eq!(year_and_ordinal(0),(1970,1));
        assert_eq!(year_and_ordinal((utc(2024,3,20,0,0)/DAY) as i64),(2024,80));
        let p=Location::new(40.0,-105.0).unwrap();
        let mut c=SolarCache::new(p,utc(2026,10,14,23,59));
        let first=c.center;
        c.ensure(utc(2026,10,15,7,1));
        assert_eq!(c.center,first+1);
        assert_eq!(c.days[0].id,first);
        // 06:00 UTC at 105 W is still Oct 13 in local solar time.
        c.ensure(utc(2026,10,14,6,0));
        assert_eq!(c.center,first-1);
        assert_eq!(c.current().id,first-1);
        // Advance forward again to verify the cache recovers the Oct 14 day.
        c.ensure(utc(2026,10,14,8,0));
        assert_eq!(c.center,first);
        assert_eq!(c.current().id,first);
    }
    #[test] fn noaa_offline_reference_times_and_seasons() {
        // Approximate NOAA/USNO published sunrise/sunset (UTC, sea-level),
        // deliberately broad 6 min tolerance for NOAA approximation/refraction.
        let cases=[
            (2026,3,20,0.0,0.0,6.1,18.2),         // equinox / equator
            (2026,6,21,40.0,-105.0,11.533,2.517),      // Denver: sunset next UTC day
            (2026,12,21,40.0,-105.0,14.283,23.633),    // Denver winter
            (2026,6,21,-33.87,151.21,20.983,6.883),    // Sydney winter (UTC wrapped)
        ];
        for &(y,m,d,lat,lon,rise,set) in &cases {
            let p=Location::new(lat,lon).unwrap();
            let t=utc(y,m,d,12,0)-lon*240.0;
            let c=SolarCache::new(p,t);
            let events=c.sunrise_sunset();
            let r=clock(events.rise.expect("rise"));
            let s=clock(events.set.expect("set"));
            let delta=|actual:f64,ref_hr:f64| ((actual-ref_hr+12.0).rem_euclid(24.0)-12.0).abs();
            assert!(delta(r,rise)<0.15,"rise {lat},{lon} {r} expected {rise}");
            assert!(delta(s,set)<0.15,"set {lat},{lon} {s} expected {set}");
        }
    }
    #[test] fn seasonal_reversal_and_extreme_twilight() {
        let n=Location::new(45.0,0.0).unwrap();
        let s=Location::new(-45.0,0.0).unwrap();
        let duration=|p,month,day| {
            let c=SolarCache::new(p,utc(2026,month,day,12,0));
            let e=c.sunrise_sunset();e.set.unwrap()-e.rise.unwrap()
        };
        assert!(duration(n,6,21)>duration(n,12,21)+5.0*3600.0);
        assert!(duration(s,12,21)>duration(s,6,21)+5.0*3600.0);
        // NOAA-formula UTC twilight references for 40N 105W, June 21;
        // rounded independent Denver almanac times (civil ~10:59,
        // nautical ~10:18, astronomical ~09:29 UTC).
        let denver=SolarCache::new(Location::new(40.0,-105.0).unwrap(),
            utc(2026,6,21,18,0));
        for (level,reference) in [(1,10.983),(2,10.30),(3,9.483)] {
            let dawn=clock(denver.current().boundaries[level].rise.unwrap());
            assert!((dawn-reference).abs()<0.16,"twilight {level}: {dawn}");
        }
        let high=SolarCache::new(Location::new(65.0,0.0).unwrap(),utc(2026,6,21,12,0));
        assert!(high.current().boundaries[3].rise.is_none()); // never fully dark
        assert!(high.current().boundaries[3].set.is_none());
    }
    #[test] fn polar_day_night_and_missing_twilight_boundaries() {
        let p=Location::new(80.0,20.0).unwrap();
        let summer=SolarCache::new(p,utc(2026,6,21,12,0));
        let winter=SolarCache::new(p,utc(2026,12,21,12,0));
        assert!(summer.current().boundaries.iter().all(|b|b.rise.is_none()&&b.set.is_none()));
        assert!(winter.current().boundaries[0].rise.is_none());
        assert!(winter.current().boundaries[0].set.is_none());
        assert!(summer.sun_progress(utc(2026,6,21,12,0)).is_some());
        assert!(winter.sun_progress(utc(2026,12,21,12,0)).is_none());
        assert!(summer.daylight(utc(2026,6,21,12,0))>0.0);
        let north=SolarCache::new(Location::new(70.0,0.0).unwrap(),utc(2026,4,20,12,0));
        assert!(north.current().boundaries[3].rise.is_none());
    }
    #[test] fn sun_visibility_and_clock_jumps() {
        let p=Location::new(40.0,-105.0).unwrap();
        let t=utc(2026,10,14,12,0);
        let mut c=SolarCache::new(p,t);
        let events=c.sunrise_sunset();
        let r=events.rise.unwrap();let s=events.set.unwrap();
        for (ts,visible) in [(r-61.0,false),(r+61.0,true),((r+s)/2.0,true),(s+61.0,false)] {
            assert_eq!(c.sun_progress(ts).is_some(),visible);
        }
        c.ensure(t+35.0*DAY);
        assert_eq!(c.center,solar_day_id(t+35.0*DAY,p.longitude));
        c.ensure(t-35.0*DAY);
        assert_eq!(c.center,solar_day_id(t-35.0*DAY,p.longitude));
    }
}
