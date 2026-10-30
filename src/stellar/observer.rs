use super::{Observer,ObservationOptions,QualityFlags,StellarError,UtcInstant,Vec3};
use super::types::{AS_RAD,AU_KM,TAU_CONST};
use super::astrometry::{Mat3,true_equator,greenwich_apparent_sidereal,earth_state};
use super::time::{julian_dates,TIME_DATASET};

#[derive(Debug,Clone)]
pub struct ObservationContext {
    pub(crate) utc:UtcInstant,
    pub(crate) observer:Observer,
    pub(crate) quality:QualityFlags,
    pub(crate) provenance:Option<&'static str>,
    pub(crate) jd_tt:f64,
    pub(crate) earth_au:Vec3,
    pub(crate) observer_au:Vec3,
    pub(crate) total_velocity_au_day:Vec3,
    pub(crate) icrs_to_true:Mat3,
    pub(crate) east:Vec3,
    pub(crate) north:Vec3,
    pub(crate) up:Vec3,
}
impl ObservationContext {
    pub fn utc(&self)->UtcInstant{self.utc}
    pub fn observer(&self)->Observer{self.observer}
    pub fn quality(&self)->QualityFlags{self.quality}
    pub fn time_dataset(&self)->&'static str{TIME_DATASET}
    /// Julian year in TT, derived from the same time basis used by this context.
    pub fn julian_year_tt(&self)->f64 {2000.0+(self.jd_tt-2_451_545.0)/365.25}
}

pub fn prepare_observation(observer:Observer,utc:UtcInstant,options:ObservationOptions)->Result<ObservationContext,StellarError>{
    let mut q=QualityFlags::default();
    q.insert(QualityFlags::TRUNCATED_NUTATION|QualityFlags::ANALYTIC_EARTH_EPHEMERIS);
    let (ut1_offset,xp,yp,provenance)=if let Some(eop)=options.earth_orientation {
        if eop.ut1_minus_utc_seconds.map_or(false,|v|!v.is_finite() || v.abs()>2.0)
            || eop.polar_motion_arcsec.map_or(false,|(x,y)|!x.is_finite() || x.abs()>5.0 || !y.is_finite() || y.abs()>5.0)
            || eop.valid_from>eop.valid_through || eop.provenance.is_empty() {return Err(StellarError::InvalidEarthOrientation);}
        if utc<eop.valid_from || utc>eop.valid_through {q.insert(QualityFlags::EOP_OUTSIDE_VALIDITY)}
        if eop.ut1_minus_utc_seconds.is_none() {q.insert(QualityFlags::ASSUMED_ZERO_UT1)}
        if eop.polar_motion_arcsec.is_none() {q.insert(QualityFlags::ASSUMED_ZERO_POLAR_MOTION)}
        let (xp,yp)=eop.polar_motion_arcsec.unwrap_or((0.0,0.0));
        (eop.ut1_minus_utc_seconds.unwrap_or(0.0),xp,yp,Some(eop.provenance))
    } else {
        q.insert(QualityFlags::ASSUMED_ZERO_UT1|QualityFlags::ASSUMED_ZERO_POLAR_MOTION);
        (0.,0.,0.,None)
    };
    let (jd_utc,jd_tt)=julian_dates(utc,&mut q);
    let jd_ut1=jd_utc+ut1_offset/86400.;
    let (pn,_,eqeq)=true_equator(jd_tt);
    let gast=greenwich_apparent_sidereal(jd_ut1,jd_tt,eqeq);
    let (latitude,longitude)=observer.angles();
    // Build the position and ENU axes in WGS84 Earth-fixed Cartesian space.
    // Polar motion rotates *all* of these vectors together. Changing latitude
    // and longitude independently is singular at geographic poles.
    let (sl,cl)=latitude.sin_cos();
    let (so,co)=longitude.sin_cos();
    let east_fixed=Vec3::new(-so,co,0.);
    let north_fixed=Vec3::new(-sl*co,-sl*so,cl);
    let up_fixed=Vec3::new(cl*co,cl*so,sl);
    let a=6_378_137.0;
    let e2=6.6943799901413165e-3;
    let n=a/(1.-e2*sl*sl).sqrt();
    let equatorial=(n+observer.elevation_m())*cl;
    let z=((1.-e2)*n+observer.elevation_m())*sl;
    let position_fixed=Vec3::new(equatorial*co,equatorial*so,z);
    // Consistent ITRS -> pseudo-Earth-fixed -> true-equator/equinox-of-date
    // active-column rotation, with xp/yp supplied in arcseconds. s' (TIO
    // locator) is neglected at its sub-milliarcsecond magnitude.
    let polar=Mat3::rot_x(-yp*AS_RAD).mul(Mat3::rot_y(-xp*AS_RAD));
    let rotation=Mat3::rot_z(gast).mul(polar);
    let east=rotation.act(east_fixed);
    let north=rotation.act(north_fixed);
    let up=rotation.act(up_fixed);
    let observer_au=rotation.act(position_fixed).scale(1./(AU_KM*1000.));
    // Physical terrestrial rotation is about the TIRS z axis, not necessarily
    // the true-of-date z axis: compute spin before applying Earth orientation.
    let rotated_fixed=polar.act(position_fixed).scale(1./(AU_KM*1000.));
    let spin=TAU_CONST*1.00273781191135448;
    let diurnal_velocity=Mat3::rot_z(gast).act(
        Vec3::new(-rotated_fixed.y,rotated_fixed.x,0.).scale(spin));
    let (earth_au,earth_vel)=earth_state(jd_tt);
    Ok(ObservationContext{utc,observer,quality:q,provenance,jd_tt,earth_au,observer_au,
        total_velocity_au_day:earth_vel.add(diurnal_velocity),icrs_to_true:pn,east,north,up})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;
    fn rotated(axis:Vec3,angle:f64,point:Vec3)->Vec3 {
        // Rodrigues independent of matrix rotation implementation.
        let (s,c)=angle.sin_cos();
        point.scale(c).add(axis.cross(point).scale(s)).add(axis.scale(axis.dot(point)*(1.0-c)))
    }
    fn near(a:Vec3,b:Vec3,tol:f64){
        assert!(a.sub(b).norm()<tol,"expected {b:?}, actual {a:?}, difference {}",a.sub(b).norm());
    }
    #[test]
    fn exact_and_near_polar_motion_cartesian_reference(){
        let instant=UtcInstant::parse("2026-07-01T12:00:00Z").unwrap();
        let xp=0.75;let yp=-0.65;
        let eop=super::super::EarthOrientation{ut1_minus_utc_seconds:Some(0.33),polar_motion_arcsec:Some((xp,yp)),
            valid_from:UtcInstant::parse("2026-01-01T00:00:00Z").unwrap(),
            valid_through:UtcInstant::parse("2026-12-31T23:59:59Z").unwrap(),provenance:"independent-polar-test"};
        for lat in [90.0,89.999999,-89.999999,-90.0] {
            for lon in [-150.0,0.0,33.0,170.0] {
                let observer=Observer::new(lat,lon,3500.0).unwrap();
                let ctx=prepare_observation(observer,instant,ObservationOptions{earth_orientation:Some(eop)}).unwrap();
                let latitude=lat.to_radians();let longitude=lon.to_radians();
                let (sl,cl)=latitude.sin_cos();let (so,co)=longitude.sin_cos();
                let e=Vec3::new(-so,co,0.);
                let n=Vec3::new(-sl*co,-sl*so,cl);
                let u=Vec3::new(cl*co,cl*so,sl);
                let (_,tt)=julian_dates(instant,&mut QualityFlags::default());
                let (_,_,eqeq)=true_equator(tt);
                let gast=greenwich_apparent_sidereal(2_440_587.5+instant.fractional_unix()/86400.0+0.33/86400.0,tt,eqeq);
                let apply=|v:Vec3|{
                    let polar=rotated(Vec3::new(0.,1.,0.),-xp*AS_RAD,v);
                    let polar=rotated(Vec3::new(1.,0.,0.),-yp*AS_RAD,polar);
                    rotated(Vec3::new(0.,0.,1.),gast,polar)
                };
                near(ctx.east,apply(e),3e-14);
                near(ctx.north,apply(n),3e-14);
                near(ctx.up,apply(u),3e-14);
                near(ctx.east.cross(ctx.north),ctx.up,4e-14);
                let a=6_378_137.0;let e2=6.6943799901413165e-3;
                let curvature=a/(1.0-e2*sl*sl).sqrt();
                let pos=Vec3::new((curvature+3500.0)*cl*co,(curvature+3500.0)*cl*so,((1.0-e2)*curvature+3500.0)*sl);
                near(ctx.observer_au,apply(pos).scale(1.0/(AU_KM*1000.)),1e-14);
                assert!((ctx.east.norm()-1.0).abs()<1e-13);
                assert!(ctx.total_velocity_au_day.norm().is_finite());
            }
        }
        let pole=prepare_observation(Observer::new(90.0,33.0,3500.0).unwrap(),instant,ObservationOptions{earth_orientation:Some(eop)}).unwrap();
        let near_pole=prepare_observation(Observer::new(89.999999,33.0,3500.0).unwrap(),instant,ObservationOptions{earth_orientation:Some(eop)}).unwrap();
        assert!(pole.up.sub(near_pole.up).norm()<2.0*PI/180.0*1e-6);
    }
}
