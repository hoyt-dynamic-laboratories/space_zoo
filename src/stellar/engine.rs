use super::{HorizontalDirection,ObservationContext,PreparedCatalogue,SnapshotMetadata,StarResult,StellarError,Vec3};
use super::types::{TAU_CONST};
use super::astrometry::{propagated_icrs,solar_deflect,aberrate};
use super::time::TIME_DATASET;

/// Transactional batch: no caller-visible writes until all computations succeed.
/// `scratch` is reusable storage; pass at least `catalogue.len()` elements.
/// Empty batches succeed even with empty buffers.
pub(crate) fn altitude_azimuth(enu:Vec3)->(f64,Option<f64>){
    let alt=enu.z.clamp(-1.,1.).asin();
    let az=if enu.x.hypot(enu.y)<=1e-12 {None}
        else {Some(enu.x.atan2(enu.y).rem_euclid(TAU_CONST))};
    (alt,az)
}

fn horizontal_from_true(ctx:&ObservationContext,p:Vec3)->Result<HorizontalDirection,StellarError>{
    let sun=ctx.earth_au.scale(-1.);
    let p=solar_deflect(p,sun)?;
    let p=aberrate(p,ctx.total_velocity_au_day)?;
    let enu=Vec3::new(p.dot(ctx.east),p.dot(ctx.north),p.dot(ctx.up)).unit()?;
    let (altitude,azimuth)=altitude_azimuth(enu);
    Ok(HorizontalDirection{enu,altitude_rad:altitude,azimuth_rad:azimuth,
        above_geometric_horizon:enu.z>0.})
}

/// Convert an already epoch-propagated ICRS direction into the same observer-relative
/// apparent direction used by the accepted stellar engine. Proper motion remains a
/// catalogue responsibility; precession/nutation, solar deflection, aberration and ENU
/// projection remain engine responsibilities.
pub fn evaluate_icrs_direction(ctx:&ObservationContext,icrs:Vec3)->Result<HorizontalDirection,StellarError>{
    if ![icrs.x,icrs.y,icrs.z].iter().all(|v|v.is_finite()) {return Err(StellarError::NonFiniteResult)}
    let true_of_date=ctx.icrs_to_true.act(icrs.unit()?).unit()?;
    horizontal_from_true(ctx,true_of_date)
}

pub fn evaluate_stars(ctx:&ObservationContext,cat:&PreparedCatalogue,output:&mut [StarResult],scratch:&mut [StarResult])->Result<SnapshotMetadata,StellarError>{
    let count=cat.len();
    if output.len()<count {return Err(StellarError::InsufficientCapacity{needed:count,provided:output.len()})}
    if scratch.len()<count {return Err(StellarError::InsufficientScratchCapacity{needed:count,provided:scratch.len()})}
    let sun=ctx.earth_au.scale(-1.);
    for (i,star) in cat.stars.iter().enumerate(){
        let p=propagated_icrs(star,ctx.jd_tt,ctx.earth_au,ctx.observer_au,ctx.icrs_to_true)?;
        let p=solar_deflect(p,sun)?;
        let p=aberrate(p,ctx.total_velocity_au_day)?;
        let enu=Vec3::new(p.dot(ctx.east),p.dot(ctx.north),p.dot(ctx.up)).unit()?;
        let (altitude,azimuth)=altitude_azimuth(enu);
        scratch[i]=StarResult{id:star.record.id,enu,altitude_rad:altitude,azimuth_rad:azimuth,
            above_geometric_horizon:enu.z>0.,quality:star.quality};
    }
    output[..count].copy_from_slice(&scratch[..count]);
    Ok(SnapshotMetadata{utc:ctx.utc,observer:ctx.observer,model:"IAU2006-precession/Meeus-truncated-nutation/equinox-ERA-0.50",
        time_dataset:TIME_DATASET,catalogue_count:count,catalogue_version:"SZ-EMBEDDED-17-0502",
        catalogue_fingerprint:cat.content_fingerprint(),quality:ctx.quality,eop_provenance:ctx.provenance})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zenith_nadir_and_tiny_horizontal_components(){
        assert_eq!(altitude_azimuth(Vec3::new(0.,0.,1.)).1,None);
        assert_eq!(altitude_azimuth(Vec3::new(0.,0.,-1.)).1,None);
        assert_eq!(altitude_azimuth(Vec3::new(1e-13,1e-13,1.)).1,None);
        assert!(altitude_azimuth(Vec3::new(2e-12,0.,1.)).1.is_some());
        assert_eq!(altitude_azimuth(Vec3::new(1.,0.,0.)).1,Some(std::f64::consts::FRAC_PI_2));
        assert_eq!(altitude_azimuth(Vec3::new(0.,1.,0.)).1,Some(0.));
        assert_eq!(altitude_azimuth(Vec3::new(-1.,0.,0.)).1,Some(3.*std::f64::consts::FRAC_PI_2));
    }
    #[test]
    fn direct_icrs_direction_matches_existing_batch_path_for_direction_only_star(){
        use super::super::{EquatorialFrame,EpochTimeScale,ObservationOptions,Observer,ProperMotionRa,StarRecord,UtcInstant,prepare_catalogue,prepare_observation};
        let star=StarRecord{id:"DIRECT_TEST",source:"unit",reference_frame:EquatorialFrame::Icrs,epoch_time_scale:EpochTimeScale::Tt,
            ra_deg:183.25,dec_deg:-22.5,epoch_julian_tt:2000.0,pm_ra_mas_per_year:0.0,pm_dec_mas_per_year:0.0,
            pm_ra_convention:ProperMotionRa::MuAlphaStar,parallax_mas:None,radial_velocity_kms:None};
        let cat=prepare_catalogue(&[star]).unwrap();
        let utc=UtcInstant::parse("2026-07-01T12:00:00Z").unwrap();
        let ctx=prepare_observation(Observer::new(35.0,-105.0,0.0).unwrap(),utc,ObservationOptions::default()).unwrap();
        let mut out=[StarResult::default();1];let mut scratch=[StarResult::default();1];
        evaluate_stars(&ctx,&cat,&mut out,&mut scratch).unwrap();
        let (sr,cr)=star.ra_deg.to_radians().sin_cos();let (sd,cd)=star.dec_deg.to_radians().sin_cos();
        let direct=evaluate_icrs_direction(&ctx,Vec3::new(cd*cr,cd*sr,sd)).unwrap();
        assert!(direct.enu.sub(out[0].enu).norm()<1e-13);
        assert!((direct.altitude_rad-out[0].altitude_rad).abs()<1e-13);
        assert_eq!(direct.azimuth_rad,out[0].azimuth_rad);
        assert_eq!(direct.above_geometric_horizon,out[0].above_geometric_horizon);
    }
}

#[cfg(test)]
mod batch_atomicity_tests {
    use super::*;
    use super::super::{Observer,ObservationOptions,UtcInstant,prepare_observation,prepare_catalogue};
    #[test]
    fn output_untouched_when_second_prepared_state_is_nonfinite(){
        let cat0=super::super::embedded_reference_stars();
        let mut cat=prepare_catalogue(&cat0[..2]).unwrap();
        // Mimic unforeseen corruption / invalid numerical intermediate in the
        // second record after first scratch write: API commit must be atomic.
        cat.stars[1].velocity_per_year=Vec3::new(f64::NAN,0.,0.);
        let t=UtcInstant::parse("2026-07-01T12:00:00Z").unwrap();
        let ctx=prepare_observation(Observer::new(45.,0.,0.).unwrap(),t,ObservationOptions::default()).unwrap();
        let untouched=StarResult::default();
        let mut out=[untouched;2];let mut scratch=[untouched;2];
        assert_eq!(evaluate_stars(&ctx,&cat,&mut out,&mut scratch),Err(StellarError::NonFiniteResult));
        assert!(out.iter().all(|s|s.id==untouched.id));
    }
}
