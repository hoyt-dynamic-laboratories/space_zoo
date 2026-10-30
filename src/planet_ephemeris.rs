//! v0.55.391 Planets Rock: one cached real-sky state for every sky planet.
//!
//! Runtime work stays deliberately small: the baked daily RA/DEC table is
//! interpolated, transformed through observer latitude/longitude and sidereal
//! rotation, then cached for every non-Earth body. Renderer, Compass listing,
//! and `:planets <name>` navigation consume this same state. No visitor queue,
//! orbital solver, perturbation model, JPL kernel, network access, or wobble
//! provider exists in the live path.
use crate::astronomy::Location;
use crate::sky::PlanetKind;

const FIRST_JULIAN_DAY:i32=2_440_587;
const LAST_JULIAN_DAY:i32=2_488_435;
const RECORD_BODIES:usize=9;
const BYTES_PER_PAIR:usize=8;
const BYTES_PER_DAY:usize=RECORD_BODIES*BYTES_PER_PAIR;
const EPHEMERIS:&[u8; 3_445_128] = include_bytes!("../data/planets/planet_ephemeris.bin");
const CACHE_SECONDS:u64=30;

#[derive(Clone,Copy,Debug,PartialEq)]
pub struct Equatorial {pub ra_deg:f64,pub dec_deg:f64}
#[derive(Clone,Copy,Debug,PartialEq)]
pub struct Horizontal {pub az_deg:f64,pub alt_deg:f64}
#[derive(Clone,Copy,Debug,PartialEq)]
pub struct PlanetState {
    pub kind:PlanetKind,
    pub ra_deg:f64,
    pub dec_deg:f64,
    pub altitude_deg:f64,
    pub azimuth_deg:f64,
    pub above_horizon:bool,
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
struct CacheKey {lat:u64,lon:u64,time_bucket:u64}

/// Tiny authoritative planet-state cache. Only coordinates/metadata live here;
/// artwork residency is owned separately by `PlanetSpriteCache` and is limited
/// to bodies that actually project into the current camera viewport.
pub struct PlanetEphemeris {
    states:Vec<PlanetState>,
    key:Option<CacheKey>,
}
impl PlanetEphemeris {
    pub fn new()->Self {Self{states:Vec::with_capacity(PlanetKind::SKY_ORDER.len()),key:None}}
    pub fn refresh(&mut self,observer:Location,unix_seconds:u64)->bool {
        let key=CacheKey{lat:observer.latitude.to_bits(),lon:observer.longitude.to_bits(),
            time_bucket:unix_seconds/CACHE_SECONDS};
        if self.key==Some(key){return false;}
        self.states.clear();
        for kind in PlanetKind::SKY_ORDER {
            let Some(eq)=interpolated_equatorial(kind,unix_seconds) else {continue;};
            let horizontal=equatorial_to_horizontal(eq,observer,unix_seconds);
            self.states.push(PlanetState{kind,ra_deg:eq.ra_deg,dec_deg:eq.dec_deg,
                altitude_deg:horizontal.alt_deg,azimuth_deg:horizontal.az_deg,
                above_horizon:horizontal.alt_deg>=0.0});
        }
        self.key=Some(key);
        true
    }
    pub fn states(&self)->&[PlanetState] {&self.states}
    pub fn state(&self,kind:PlanetKind)->Option<PlanetState> {
        self.states.iter().copied().find(|state|state.kind==kind)
    }
    #[cfg(test)]
    fn resident_state_bytes_bound(&self)->usize {
        self.states.capacity()*std::mem::size_of::<PlanetState>()
    }
}
impl Default for PlanetEphemeris {fn default()->Self{Self::new()}}

#[inline] fn normalize_deg(x:f64)->f64{x.rem_euclid(360.0)}
#[inline] fn unix_to_julian_date(unix_seconds:u64)->f64 {
    unix_seconds as f64/86_400.0+2_440_587.5
}
#[inline] fn body_index(kind:PlanetKind)->Option<usize>{
    Some(match kind {
        PlanetKind::Mercury=>0,PlanetKind::Venus=>1,PlanetKind::Earth=>return None,
        PlanetKind::Mars=>3,PlanetKind::Jupiter=>4,PlanetKind::Saturn=>5,
        PlanetKind::Uranus=>6,PlanetKind::Neptune=>7,PlanetKind::Pluto=>8,
    })
}
fn read_pair(day:i32,kind:PlanetKind)->Option<Equatorial>{
    let body=body_index(kind)?;
    if !(FIRST_JULIAN_DAY..=LAST_JULIAN_DAY).contains(&day){return None;}
    let record=(day-FIRST_JULIAN_DAY) as usize*BYTES_PER_DAY+body*BYTES_PER_PAIR;
    let ra=f32::from_le_bytes(EPHEMERIS[record..record+4].try_into().ok()?) as f64;
    let dec=f32::from_le_bytes(EPHEMERIS[record+4..record+8].try_into().ok()?) as f64;
    Some(Equatorial{ra_deg:ra,dec_deg:dec})
}
pub fn interpolated_equatorial(kind:PlanetKind,unix_seconds:u64)->Option<Equatorial>{
    if kind==PlanetKind::Earth{return None;}
    let jd=unix_to_julian_date(unix_seconds);
    let day=jd.floor() as i32;
    let t=jd-day as f64;
    let a=read_pair(day,kind)?;let b=read_pair(day+1,kind)?;
    let delta=((b.ra_deg-a.ra_deg+540.0).rem_euclid(360.0))-180.0;
    Some(Equatorial{ra_deg:normalize_deg(a.ra_deg+delta*t),
        dec_deg:a.dec_deg+(b.dec_deg-a.dec_deg)*t})
}

/// Greenwich mean sidereal time. The baked coordinates are mean-equator /
/// equinox-of-date values, so mean sidereal rotation is the intentional pair.
pub fn gmst_deg(jd:f64)->f64 {
    let d=jd-2_451_545.0;
    normalize_deg(280.460_618_37+360.985_647_366_29*d)
}

/// Choose the Earth observer point that puts `kind` exactly at the normal
/// Space Zoo camera center without steering the camera. The locked reset view
/// is north (AZ 0) at +30 degrees altitude, so the observer sits 60 degrees
/// south of the planet's declination on the planet's current meridian.
///
/// Across the baked 1970..2100 planet table every supported body's declination
/// keeps this latitude on Earth; returning `None` preserves that invariant as a
/// checked boundary rather than silently inventing a camera offset.
pub fn default_camera_observer(kind:PlanetKind,unix_seconds:u64)->Option<Location> {
    let eq=interpolated_equatorial(kind,unix_seconds)?;
    let camera_alt=space_zoo::star_projection::CAMERA_ALTITUDE_DEG;
    let camera_az=space_zoo::star_projection::CAMERA_AZIMUTH_DEG;
    if camera_az.abs()>1e-12 || !(0.0..=90.0).contains(&camera_alt) {return None;}
    let latitude=eq.dec_deg-(90.0-camera_alt);
    if !(-90.0..=90.0).contains(&latitude) {return None;}
    let longitude=((eq.ra_deg-gmst_deg(unix_to_julian_date(unix_seconds))+180.0)
        .rem_euclid(360.0))-180.0;
    Location::new(latitude,longitude).ok()
}
pub fn equatorial_to_horizontal(eq:Equatorial,observer:Location,unix_seconds:u64)->Horizontal {
    let jd=unix_to_julian_date(unix_seconds);
    let lst_deg=normalize_deg(gmst_deg(jd)+observer.longitude);
    let h=normalize_deg(lst_deg-eq.ra_deg).to_radians();
    let dec=eq.dec_deg.to_radians();
    let lat=observer.latitude.to_radians();
    let sin_alt=dec.sin()*lat.sin()+dec.cos()*lat.cos()*h.cos();
    let alt=sin_alt.clamp(-1.0,1.0).asin();
    let y=-h.sin()*dec.cos();
    let x=dec.sin()*lat.cos()-dec.cos()*lat.sin()*h.cos();
    Horizontal{az_deg:normalize_deg(y.atan2(x).to_degrees()),alt_deg:alt.to_degrees()}
}
pub const LAST_SUPPORTED_YEAR:i32=2100;

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn baked_size_and_range_are_fixed(){
        assert_eq!(EPHEMERIS.len(),3_445_128);
        assert_eq!(FIRST_JULIAN_DAY,2_440_587);assert_eq!(LAST_JULIAN_DAY,2_488_435);
    }
    #[test] fn earth_is_observer_body_and_all_sky_bodies_resolve_now(){
        let place=Location::new(0.0,0.0).unwrap();let now=1_824_326_400; // 2027-10-24 UTC
        let mut cache=PlanetEphemeris::new();cache.refresh(place,now);
        assert_eq!(cache.states().len(),PlanetKind::SKY_ORDER.len());
        assert!(cache.state(PlanetKind::Earth).is_none());
        for kind in PlanetKind::SKY_ORDER {assert!(cache.state(kind).is_some());}
    }
    #[test] fn cache_holds_all_sky_states_but_only_tiny_metadata(){
        let place=Location::new(39.7392,-104.9903).unwrap();
        let mut cache=PlanetEphemeris::new();
        assert!(cache.refresh(place,1_824_326_400));
        assert_eq!(cache.states().len(),8);
        assert!(cache.resident_state_bytes_bound()<4096);
        assert!(!cache.refresh(place,1_824_326_410),"same 30-second bucket must reuse state");
        assert!(cache.refresh(place,1_824_326_431),"new bucket must refresh state");
    }
    #[test] fn interpolation_wraps_ra_short_way(){
        let a=Equatorial{ra_deg:359.0,dec_deg:-2.0};let b=Equatorial{ra_deg:1.0,dec_deg:2.0};
        let delta=((b.ra_deg-a.ra_deg+540.0).rem_euclid(360.0))-180.0;
        assert_eq!(delta,2.0);
    }
    #[test] fn planet_navigation_moves_observer_so_reset_camera_is_centered(){
        let utc=1_824_326_400; // 2027-10-24 UTC
        for kind in PlanetKind::SKY_ORDER {
            let eq=interpolated_equatorial(kind,utc).unwrap();
            let observer=default_camera_observer(kind,utc).unwrap();
            let horizontal=equatorial_to_horizontal(eq,observer,utc);
            assert!((horizontal.alt_deg-space_zoo::star_projection::CAMERA_ALTITUDE_DEG).abs()<1e-8,
                "{} altitude {}",kind.name(),horizontal.alt_deg);
            let az=((horizontal.az_deg-space_zoo::star_projection::CAMERA_AZIMUTH_DEG+180.0)
                .rem_euclid(360.0))-180.0;
            assert!(az.abs()<1e-8,"{} azimuth {}",kind.name(),horizontal.az_deg);
        }
    }
    #[test] fn baked_planet_range_can_always_use_reset_camera_observer(){
        for utc in [0,946_684_800,2_524_608_000,4_134_023_999] {
            for kind in PlanetKind::SKY_ORDER {
                assert!(default_camera_observer(kind,utc).is_some(),
                    "{} lacks reset-camera observer at {}",kind.name(),utc);
            }
        }
    }
    #[test] fn horizontal_cardinals_have_navigation_azimuth_convention(){
        // At the equator, a declination-zero body on the meridian is at zenith.
        let observer=Location::new(0.0,0.0).unwrap();let utc=0u64;
        let ra=gmst_deg(unix_to_julian_date(utc));
        let h=equatorial_to_horizontal(Equatorial{ra_deg:ra,dec_deg:0.0},observer,utc);
        assert!((h.alt_deg-90.0).abs()<1e-8);
    }
}
