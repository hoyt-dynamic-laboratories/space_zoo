use std::f64::consts::{PI,TAU};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {pub x:f64,pub y:f64,pub z:f64}
impl Vec3 {
    pub const fn new(x:f64,y:f64,z:f64)->Self {Self{x,y,z}}
    pub fn dot(self,b:Self)->f64 {self.x*b.x+self.y*b.y+self.z*b.z}
    pub fn cross(self,b:Self)->Self {Self::new(self.y*b.z-self.z*b.y,self.z*b.x-self.x*b.z,self.x*b.y-self.y*b.x)}
    pub fn norm(self)->f64 {self.x.hypot(self.y).hypot(self.z)}
    pub fn unit(self)->Result<Self,StellarError> {let n=self.norm(); if !n.is_finite() || n<=0.0 {Err(StellarError::NonFiniteResult)} else {Ok(self.scale(1.0/n))}}
    pub fn scale(self,s:f64)->Self {Self::new(self.x*s,self.y*s,self.z*s)}
    pub fn add(self,b:Self)->Self {Self::new(self.x+b.x,self.y+b.y,self.z+b.z)}
    pub fn sub(self,b:Self)->Self {Self::new(self.x-b.x,self.y-b.y,self.z-b.z)}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StellarError {
    InvalidObserver,InvalidUtc,UnsupportedLeapSecondInstant,OutOfRangeUtc,
    InvalidEarthOrientation,InvalidCatalogueRecord {id:&'static str},
    DuplicateCatalogueId {id:&'static str},InsufficientCapacity {needed:usize,provided:usize},
    InsufficientScratchCapacity {needed:usize,provided:usize},NonFiniteResult,
    ExcessiveVelocity {id:&'static str},
}
impl std::fmt::Display for StellarError {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {write!(f,"{self:?}")}}
impl std::error::Error for StellarError {}

#[derive(Debug,Clone,Copy,PartialEq)]
pub struct Observer {latitude_rad:f64,longitude_rad:f64,elevation_m:f64}
impl Observer {
    pub fn new(latitude_deg:f64,longitude_deg:f64,elevation_m:f64)->Result<Self,StellarError>{
        if !latitude_deg.is_finite() || !longitude_deg.is_finite() || !elevation_m.is_finite()
           || !(-90.0..=90.0).contains(&latitude_deg) || elevation_m < -11_000.0 || elevation_m>100_000.0 {
            return Err(StellarError::InvalidObserver);
        }
        let lon = (longitude_deg.rem_euclid(360.0)+180.0).rem_euclid(360.0)-180.0;
        Ok(Self{latitude_rad:latitude_deg.to_radians(),longitude_rad:lon.to_radians(),elevation_m})
    }
    pub fn latitude_deg(self)->f64 {self.latitude_rad.to_degrees()}
    pub fn longitude_deg(self)->f64 {self.longitude_rad.to_degrees()}
    pub fn elevation_m(self)->f64 {self.elevation_m}
    pub(crate) fn angles(self)->(f64,f64) {(self.latitude_rad,self.longitude_rad)}
}

#[derive(Debug,Clone,Copy,PartialEq)]
pub struct EarthOrientation {
    pub ut1_minus_utc_seconds:Option<f64>,
    /// Optional x/y polar motion together in arcseconds.
    pub polar_motion_arcsec:Option<(f64,f64)>,
    /// The data's asserted inclusive UTC validity interval.
    pub valid_from:super::UtcInstant,
    pub valid_through:super::UtcInstant,
    pub provenance:&'static str,
}
#[derive(Debug,Clone,Copy,Default)]
pub struct ObservationOptions {
    pub earth_orientation:Option<EarthOrientation>,
}

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum ProperMotionRa {DAlphaDt,MuAlphaStar}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum EquatorialFrame {Icrs}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum EpochTimeScale {Tt}

/// Original ICRS coordinate epoch is a Julian epoch in the TT timescale (e.g. 1991.25).
/// Proper motions are milliarcseconds per Julian year. `mu_alpha_star` includes cos(dec).
#[derive(Debug,Clone,Copy)]
pub struct StarRecord {
    pub id:&'static str,
    pub source:&'static str,
    pub reference_frame:EquatorialFrame,
    pub epoch_time_scale:EpochTimeScale,
    pub ra_deg:f64,
    pub dec_deg:f64,
    pub epoch_julian_tt:f64,
    pub pm_ra_mas_per_year:f64,
    pub pm_dec_mas_per_year:f64,
    pub pm_ra_convention:ProperMotionRa,
    pub parallax_mas:Option<f64>,
    pub radial_velocity_kms:Option<f64>,
}

#[derive(Debug,Clone,Copy,Default,PartialEq,Eq)]
pub struct QualityFlags(pub u32);
impl QualityFlags {
    pub const ASSUMED_ZERO_PARALLAX:u32=1<<0;
    pub const ASSUMED_ZERO_RADIAL_VELOCITY:u32=1<<1;
    pub const ASSUMED_ZERO_UT1:u32=1<<2;
    pub const ASSUMED_ZERO_POLAR_MOTION:u32=1<<3;
    pub const FUTURE_LEAP_OFFSET_HELD:u32=1<<4;
    pub const EOP_OUTSIDE_VALIDITY:u32=1<<5;
    pub const TRUNCATED_NUTATION:u32=1<<6;
    pub const ANALYTIC_EARTH_EPHEMERIS:u32=1<<7;
    pub fn contains(self,flag:u32)->bool {self.0&flag!=0}
    pub(crate) fn insert(&mut self,flag:u32) {self.0|=flag;}
}

#[derive(Debug,Clone)]
pub struct PreparedStar {
    pub(crate) record:StarRecord,
    /// Cartesian state at J2000 TT. Units are AU and AU/Julian-year when
    /// parallax is positive; otherwise homogeneous direction and direction/year.
    /// Both are propagated linearly before final normalization: no velocity
    /// component is lost during epoch conversion.
    pub(crate) position_j2000:Vec3,
    pub(crate) velocity_per_year:Vec3,
    pub(crate) distance_au:Option<f64>,
    pub(crate) quality:QualityFlags,
}
#[derive(Debug,Clone)]
pub struct PreparedCatalogue {pub(crate) stars:Vec<PreparedStar>,pub(crate) fingerprint:u64}
impl PreparedCatalogue {
    pub fn len(&self)->usize{self.stars.len()}
    pub fn is_empty(&self)->bool{self.stars.is_empty()}
    pub fn original_record(&self,index:usize)->Option<&StarRecord>{self.stars.get(index).map(|s|&s.record)}
    pub fn content_fingerprint(&self)->u64{self.fingerprint}
}

#[derive(Debug,Clone,Copy)]
pub struct StarResult {
    pub id:&'static str,
    pub enu:Vec3,
    pub altitude_rad:f64,
    pub azimuth_rad:Option<f64>,
    pub above_geometric_horizon:bool,
    pub quality:QualityFlags,
}
impl Default for StarResult {fn default()->Self{Self{id:"",enu:Vec3::new(0.0,0.0,1.0),altitude_rad:PI/2.0,azimuth_rad:None,above_geometric_horizon:true,quality:QualityFlags::default()}}}

/// Observer-relative result for a direction supplied directly in the ICRS frame.
/// Used by production catalogues whose stable IDs are not `&'static str`.
#[derive(Debug,Clone,Copy)]
pub struct HorizontalDirection {
    pub enu:Vec3,
    pub altitude_rad:f64,
    pub azimuth_rad:Option<f64>,
    pub above_geometric_horizon:bool,
}
#[derive(Debug,Clone,Copy,PartialEq)]
pub struct SnapshotMetadata {
    pub utc:super::UtcInstant,
    pub observer:Observer,
    pub model:&'static str,
    pub time_dataset:&'static str,
    pub catalogue_count:usize,
    pub catalogue_version:&'static str,
    pub catalogue_fingerprint:u64,
    pub quality:QualityFlags,
    pub eop_provenance:Option<&'static str>,
}
pub(crate) fn direction(ra:f64,dec:f64)->Vec3 {let (sd,cd)=dec.sin_cos();let (sr,cr)=ra.sin_cos();Vec3::new(cd*cr,cd*sr,sd)}
pub(crate) const MAS_RAD:f64=PI/(180.0*3600.0*1000.0);
pub(crate) const AS_RAD:f64=PI/(180.0*3600.0);
pub(crate) const AU_KM:f64=149_597_870.7;
pub(crate) const C_AU_DAY:f64=173.144632674240;
pub(crate) const JULIAN_YEAR_DAYS:f64=365.25;
pub(crate) const TAU_CONST:f64=TAU;
