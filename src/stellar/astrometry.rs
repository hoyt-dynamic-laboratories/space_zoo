//! Low-cost analytical astrometry; all matrix operations use active rotations
//! of column vectors. No CIO/equinox mismatch: true-equator/equinox vectors are
//! paired with GAST (GMST06 + equation of equinoxes).
//!
//! SOFA-derived-work notice: portions of these computations are derived by
//! Hoyt Dynamic Laboratories from algorithms provided by the IAU SOFA project
//! under the SOFA Software License. Space Zoo is not software provided by or
//! endorsed by SOFA. The implementation uses Space Zoo Rust types/names, active
//! column-vector conventions, and a deliberately truncated nutation model; it
//! does not claim to reproduce the complete SOFA model or use `iau`/`sofa`
//! routine-name prefixes. See CREDITS.md for the source/license ledger.
use super::{Vec3,StellarError};
use super::types::{AS_RAD,C_AU_DAY,JULIAN_YEAR_DAYS,TAU_CONST};

#[derive(Clone,Copy,Debug)]
pub(crate) struct Mat3(pub [[f64;3];3]);
impl Mat3 {
    pub fn rot_x(a:f64)->Self{let (s,c)=a.sin_cos();Self([[1.,0.,0.],[0.,c,-s],[0.,s,c]])}
    pub fn rot_z(a:f64)->Self{let (s,c)=a.sin_cos();Self([[c,-s,0.],[s,c,0.],[0.,0.,1.]])}
    pub fn rot_y(a:f64)->Self{let (s,c)=a.sin_cos();Self([[c,0.,s],[0.,1.,0.],[-s,0.,c]])}
    pub fn mul(self,b:Self)->Self{let mut r=[[0.;3];3];for (i,row) in r.iter_mut().enumerate(){for (j,e) in row.iter_mut().enumerate(){*e=(0..3).map(|k|self.0[i][k]*b.0[k][j]).sum()}}Self(r)}
    pub fn act(self,p:Vec3)->Vec3{let a=self.0;Vec3::new(a[0][0]*p.x+a[0][1]*p.y+a[0][2]*p.z,
        a[1][0]*p.x+a[1][1]*p.y+a[1][2]*p.z,a[2][0]*p.x+a[2][1]*p.y+a[2][2]*p.z)}
}

/// Fukushima-Williams IAU 2006 precession polynomial with frame bias to subarcsecond order.
/// Consistent active transform ICRS/J2000 -> mean equator/equinox of date.
fn precession_2006(tt_jd:f64)->(Mat3,f64){
    let t=(tt_jd-2_451_545.)/36_525.;
    let gamb=(-0.052928+(10.556378+(0.4932044+(-0.00031238+(-0.000002788+0.0000000260*t)*t)*t)*t)*t)*AS_RAD;
    let phib=(84381.412819+(-46.811016+(0.0511268+(0.00053289+(-0.000000440-0.0000000176*t)*t)*t)*t)*t)*AS_RAD;
    let psib=(-0.041775+(5038.481484+(1.5584175+(-0.00018522+(-0.000026452-0.0000000148*t)*t)*t)*t)*t)*AS_RAD;
    let epsa=(84381.406+(-46.836769+(-0.0001831+(0.00200340+(-0.000000576-0.0000000434*t)*t)*t)*t)*t)*AS_RAD;
    // SOFA fw2m passive-rotation formula inverted into active column convention.
    (Mat3::rot_x(epsa).mul(Mat3::rot_z(psib)).mul(Mat3::rot_x(-phib)).mul(Mat3::rot_z(-gamb)),epsa)
}

/// Largest IAU 2000 nutation terms retained. Missing 2000A series is an explicit
/// model approximation/quality flag, NOT described as a full IAU 2000A solution.
fn nutation_approx(tt_jd:f64)->(f64,f64,f64){
    let t=(tt_jd-2_451_545.)/36_525.;
    let omega=(125.04452-1934.136261*t+0.0020708*t*t+t*t*t/450000.).to_radians();
    let l=(280.4665+36000.7698*t).to_radians();
    let lp=(218.3165+481267.8813*t).to_radians();
    let dpsi=(-17.20*omega.sin()-1.32*(2.*l).sin()-0.23*(2.*lp).sin()+0.21*(2.*omega).sin())*AS_RAD;
    let deps=(9.20*omega.cos()+0.57*(2.*l).cos()+0.10*(2.*lp).cos()-0.09*(2.*omega).cos())*AS_RAD;
    (dpsi,deps,omega)
}

pub(crate) fn true_equator(tt_jd:f64)->(Mat3,f64,f64){
    let (p,epsa)=precession_2006(tt_jd);
    let (dpsi,deps,omega)=nutation_approx(tt_jd);
    let n=Mat3::rot_x(epsa+deps).mul(Mat3::rot_z(dpsi)).mul(Mat3::rot_x(-epsa));
    (n.mul(p),epsa+deps,dpsi* (epsa+deps).cos()+(0.00264*omega.sin()+0.000063*(2.*omega).sin())*AS_RAD)
}

/// Greenwich apparent sidereal angle: ERA + GMST06 polynomial + equation of equinoxes.
/// This is consistently equinox-based; CIO RA is never mixed with GAST.
pub(crate) fn greenwich_apparent_sidereal(ut1_jd:f64,tt_jd:f64,equation_equinoxes:f64)->f64 {
    let days=ut1_jd-2_451_545.;
    let era=TAU_CONST*(0.7790572732640+1.00273781191135448*days).rem_euclid(1.);
    let t=(tt_jd-2_451_545.)/36_525.;
    let polynomial=(0.014506+(4612.156534+(1.3915817+(-0.00000044+(-0.000029956-0.0000000368*t)*t)*t)*t)*t)*AS_RAD;
    (era+polynomial+equation_equinoxes).rem_euclid(TAU_CONST)
}

/// Meeus/VSOP-derived geometric solar position; returned vector is Earth to Sun.
/// No JPL ephemeris is embedded, and no absolute arcsecond ephemeris claim is made.
fn geometric_sun(tt_jd:f64)->(f64,f64){
    let t=(tt_jd-2_451_545.)/36_525.;
    let mean_lon=(280.46646+36000.76983*t+0.0003032*t*t).rem_euclid(360.);
    let m=(357.52911+35999.05029*t-0.0001537*t*t).to_radians();
    let eccentricity=0.016708634-0.000042037*t-0.0000001267*t*t;
    let c=(1.914602-0.004817*t-0.000014*t*t)*m.sin()
        +(0.019993-0.000101*t)*(2.*m).sin()+0.000289*(3.*m).sin();
    let lon=(mean_lon+c).to_radians();
    let radius=1.000001018*(1.-eccentricity*eccentricity)/(1.+eccentricity*(m+c.to_radians()).cos());
    (lon,radius)
}
fn earth_heliocentric_true_date(tt_jd:f64)->Vec3 {
    let (lon,r)=geometric_sun(tt_jd);
    let (_,eps,_)=true_equator(tt_jd);
    let (s,c)=lon.sin_cos();
    // Earth opposite the Sun, equatorial-of-date using mean-obliquity+nutation.
    Vec3::new(-r*c,-r*s*eps.cos(),-r*s*eps.sin())
}
pub(crate) fn earth_state(tt_jd:f64)->(Vec3,Vec3){
    let r=earth_heliocentric_true_date(tt_jd);
    let h=0.002;
    let v=earth_heliocentric_true_date(tt_jd+h).sub(earth_heliocentric_true_date(tt_jd-h)).scale(0.5/h);
    (r,v)
}

/// First-order aberration, including annual and diurnal velocity; beta < 0.001.
pub(crate) fn aberrate(p:Vec3,v_au_day:Vec3)->Result<Vec3,StellarError> {
    let beta=v_au_day.scale(1./C_AU_DAY);
    if beta.norm()>0.001 {return Err(StellarError::NonFiniteResult)}
    p.add(beta.sub(p.scale(p.dot(beta)))).unit()
}

/// Solar monopole light bending (distant source, single gravitating body);
/// near-Sun geometry deliberately bounded by the physical solar limb.
pub(crate) fn solar_deflect(p:Vec3,sun:Vec3)->Result<Vec3,StellarError> {
    let s=sun.unit()?;
    let cos=p.dot(s).clamp(-1.,1.);
    // Angular size of the solar disk at ~1AU; no ray in this model passes through it.
    let min_den=1.-(0.00465_f64).cos();
    let denom=(1.-cos).max(min_den);
    // Schwarzschild 2GM/(c² AU), dimensionless; deflect away from the Sun.
    let k=1.97412574336e-8;
    p.sub(s.sub(p.scale(cos)).scale(k/denom)).unit()
}

pub(crate) fn propagated_icrs(star:&super::types::PreparedStar,tt_jd:f64,earth_au:Vec3,observer_au:Vec3,frame:Mat3)->Result<Vec3,StellarError>{
    let dt=(tt_jd-2_451_545.)/JULIAN_YEAR_DAYS;
    // Propagate the unchanged Cartesian phase-space state. Re-projecting the
    // old radial velocity onto a new J2000 direction would alter the velocity.
    let source=star.position_j2000.add(star.velocity_per_year.scale(dt));
    let position=if star.distance_au.is_some() {
        // Transform both barycentric star and Earth+observer into true-of-date frame.
        frame.act(source).sub(earth_au).sub(observer_au).unit()?
    } else {
        // Without a distance, transverse motion is a projective (homogeneous)
        // direction. Normalize only at evaluation, not between epoch segments.
        frame.act(source.unit()?).unit()?
    };
    Ok(position)
}
