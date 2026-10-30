use super::{PreparedCatalogue,StarRecord,ProperMotionRa,EquatorialFrame,EpochTimeScale,QualityFlags,StellarError,Vec3};
use super::types::PreparedStar;
use super::types::{MAS_RAD,AU_KM,JULIAN_YEAR_DAYS,C_AU_DAY,direction};
use std::collections::HashSet;

/// ESA (1997) I/239 individual Hipparcos astrometric records, ICRS at J1991.25 TT.
/// Source evidence: tests/provenance/hipparcos_1997_selected.json (operator retrieved).
/// This is a small test catalogue, not the historical artistic constellation data.
/// Radial velocity is unknown for these entries: None is not a measured zero.
pub fn embedded_reference_stars()->&'static [StarRecord] {&REFERENCE}
const SRC:&str="ESA1997_Hipparcos_I239_hip_main_J1991p25";
const fn r(id:&'static str,ra:f64,dec:f64,pmra:f64,pmdec:f64,plx:Option<f64>,rv:Option<f64>)->StarRecord{
    StarRecord{id,source:SRC,reference_frame:EquatorialFrame::Icrs,epoch_time_scale:EpochTimeScale::Tt,
        ra_deg:ra,dec_deg:dec,epoch_julian_tt:1991.25,
        pm_ra_mas_per_year:pmra,pm_dec_mas_per_year:pmdec,pm_ra_convention:ProperMotionRa::MuAlphaStar,
        parallax_mas:plx,radial_velocity_kms:rv}
}
const REFERENCE:[StarRecord;17]=[
    r("HIP11767_Polaris",37.94614689,89.26413805,44.22,-11.74,Some(7.56),None),
    r("HIP32349_Sirius",101.28854105,-16.71314306,-546.01,-1223.08,Some(379.21),None),
    r("HIP27989_Betelgeuse",88.79287161,7.40703634,27.33,10.86,Some(7.63),None),
    r("HIP24436_Rigel",78.63446353,-8.20163919,1.87,-0.56,Some(4.22),None),
    r("HIP91262_Vega",279.23410832,38.78299311,201.02,287.46,Some(128.93),None),
    r("HIP102098_Deneb",310.3579727,45.28033423,1.56,1.55,Some(1.01),None),
    r("HIP97649_Altair",297.6945086,8.86738491,536.82,385.54,Some(194.44),None),
    r("HIP69673_Arcturus",213.91811403,19.18726997,-1093.45,-1999.4,Some(88.85),None),
    r("HIP37279_Procyon",114.82724194,5.22750767,-716.57,-1034.58,Some(285.93),None),
    r("HIP87937_BarnardsStar",269.45402305,4.66828815,-797.84,10326.93,Some(549.01),None),
    r("HIP30438_Canopus",95.98787763,-52.69571799,19.99,23.67,Some(10.43),None),
    r("HIP7588_Achernar",24.42813204,-57.23666007,88.02,-40.08,Some(22.68),None),
    r("HIP71683_AlphaCenA",219.92041034,-60.83514707,-3678.19,481.84,Some(742.12),None),
    r("HIP113368_Fomalhaut",344.41177323,-29.62183701,329.22,-164.22,Some(130.08),None),
    r("HIP677_Alpheratz",2.09653333,29.09082805,135.68,-162.95,Some(33.6),None),
    r("HIP60718_Acrux",186.64975585,-63.09905586,-35.37,-14.73,Some(10.17),None),
    r("HIP72607_Kochab",222.67664751,74.15547596,-32.29,11.91,Some(25.79),None),
];

/// Deterministic FNV-1a identity over the actual ordered input records,
/// including the distinction between None and Some(0.0) optional values.
/// Non-cryptographic: detects accidental catalogue changes, not tampering.
fn catalogue_fingerprint(records:&[StarRecord])->u64 {
    fn absorb(h:&mut u64,bytes:&[u8]) {
        for &byte in bytes {*h^=u64::from(byte);*h=h.wrapping_mul(0x100000001b3);}
    }
    fn number(h:&mut u64,v:f64){absorb(h,&v.to_bits().to_le_bytes());}
    fn option(h:&mut u64,v:Option<f64>) {
        absorb(h,&[u8::from(v.is_some())]);if let Some(x)=v {number(h,x)}
    }
    let mut h=0xcbf29ce484222325u64;
    absorb(&mut h,&(records.len() as u64).to_le_bytes());
    for r in records {
        for s in [r.id,r.source] {absorb(&mut h,&(s.len() as u64).to_le_bytes());absorb(&mut h,s.as_bytes());}
        absorb(&mut h,&[match r.reference_frame {EquatorialFrame::Icrs=>1},
            match r.epoch_time_scale {EpochTimeScale::Tt=>1},
            match r.pm_ra_convention {ProperMotionRa::DAlphaDt=>1,ProperMotionRa::MuAlphaStar=>2}]);
        for v in [r.ra_deg,r.dec_deg,r.epoch_julian_tt,r.pm_ra_mas_per_year,r.pm_dec_mas_per_year] {number(&mut h,v);}
        option(&mut h,r.parallax_mas);option(&mut h,r.radial_velocity_kms);
    }
    h
}

pub fn prepare_catalogue(records:&[StarRecord])->Result<PreparedCatalogue,StellarError>{
    let mut result=Vec::with_capacity(records.len());
    let mut ids=HashSet::with_capacity(records.len());
    const MAX_AU_PER_YEAR:f64=0.01*C_AU_DAY*JULIAN_YEAR_DAYS;
    for &record in records.iter(){
        let invalid=record.id.is_empty() || record.source.is_empty() || !record.ra_deg.is_finite()
          || !record.dec_deg.is_finite()
          || !(-90.0..=90.0).contains(&record.dec_deg) || !record.epoch_julian_tt.is_finite()
          || !(1800.0..=2200.0).contains(&record.epoch_julian_tt)
          || !record.pm_ra_mas_per_year.is_finite() || !record.pm_dec_mas_per_year.is_finite()
          || record.parallax_mas.map_or(false,|v|!v.is_finite() || v<0.0)
          || record.radial_velocity_kms.map_or(false,|v|!v.is_finite())
          || (record.pm_ra_convention==ProperMotionRa::DAlphaDt && record.dec_deg.to_radians().cos().abs()<1e-12
              && record.pm_ra_mas_per_year!=0.0);
        if invalid {return Err(StellarError::InvalidCatalogueRecord{id:record.id});}
        if !ids.insert(record.id) {return Err(StellarError::DuplicateCatalogueId{id:record.id});}
        let mut q=QualityFlags::default();
        if record.parallax_mas.is_none(){q.insert(QualityFlags::ASSUMED_ZERO_PARALLAX)}
        if record.radial_velocity_kms.is_none(){q.insert(QualityFlags::ASSUMED_ZERO_RADIAL_VELOCITY)}
        let ra=record.ra_deg.rem_euclid(360.0).to_radians();let dec=record.dec_deg.to_radians();
        let p=direction(ra,dec);
        let alpha_rate=record.pm_ra_mas_per_year*MAS_RAD*if record.pm_ra_convention==ProperMotionRa::DAlphaDt {dec.cos()} else {1.0};
        let delta_rate=record.pm_dec_mas_per_year*MAS_RAD;
        let tangent=Vec3::new(-ra.sin()*alpha_rate-dec.sin()*ra.cos()*delta_rate,
            ra.cos()*alpha_rate-dec.sin()*ra.sin()*delta_rate,dec.cos()*delta_rate);
        let distance=record.parallax_mas.filter(|v|*v>0.0).map(|v|1.0/(v*MAS_RAD));
        let radial=record.radial_velocity_kms.unwrap_or(0.0)*86_400.0*JULIAN_YEAR_DAYS/AU_KM;
        // Radial speed is constrained even for parallax None or Some(0.0),
        // which cannot constrain the otherwise unknown transverse speed.
        if !radial.is_finite() || radial.abs()>MAX_AU_PER_YEAR {
            return Err(StellarError::ExcessiveVelocity{id:record.id});
        }
        let (origin,velocity)=if let Some(dist)=distance {
            let v=tangent.scale(dist).add(p.scale(radial));
            if !dist.is_finite() {return Err(StellarError::InvalidCatalogueRecord{id:record.id});}
            if !v.norm().is_finite() || v.norm()>MAX_AU_PER_YEAR {
                return Err(StellarError::ExcessiveVelocity{id:record.id});
            }
            (p.scale(dist),v)
        } else {(p,tangent)};
        let dt=2000.0-record.epoch_julian_tt;
        let position_j2000=origin.add(velocity.scale(dt));
        if !position_j2000.norm().is_finite() || position_j2000.norm()<=0.0 {
            return Err(StellarError::InvalidCatalogueRecord{id:record.id});
        }
        result.push(PreparedStar{record,position_j2000,velocity_per_year:velocity,distance_au:distance,quality:q});
    }
    Ok(PreparedCatalogue{stars:result,fingerprint:catalogue_fingerprint(records)})
}
