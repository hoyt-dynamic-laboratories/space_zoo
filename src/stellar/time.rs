use super::{StellarError,QualityFlags};
/// Strict proleptic-Gregorian UTC civil instant (leap-second labels are intentionally unsupported).
/// No environment clock or operating-system timezone or leap data is consulted.
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub struct UtcInstant {unix_seconds:i64,nanosecond:u32}
impl UtcInstant {
    pub fn parse(s:&str)->Result<Self,StellarError> {
        let b=s.as_bytes();
        if b.len()<20 || b[4]!=b'-' || b[7]!=b'-' || b[10]!=b'T' || b[13]!=b':' || b[16]!=b':' || *b.last().unwrap_or(&0)!=b'Z' {return Err(StellarError::InvalidUtc)}
        let dig=|a:usize,z:usize|->Result<u32,StellarError>{let part=b.get(a..z).ok_or(StellarError::InvalidUtc)?;
            if !part.iter().all(u8::is_ascii_digit) {return Err(StellarError::InvalidUtc)}
            let mut v=0u32;for d in part {v=v*10+(d-b'0') as u32;}Ok(v)};
        let y=dig(0,4)? as i32; let m=dig(5,7)?;let d=dig(8,10)?;
        let hh=dig(11,13)?;let mm=dig(14,16)?;let ss=dig(17,19)?;
        if ss==60 {return Err(StellarError::UnsupportedLeapSecondInstant)}
        if hh>23 || mm>59 || ss>59 || !(1..=12).contains(&m) {return Err(StellarError::InvalidUtc)}
        let leap=y%4==0 && (y%100!=0 || y%400==0);
        let monthdays=[31,if leap {29}else{28},31,30,31,30,31,31,30,31,30,31];
        if d==0 || d>monthdays[(m-1) as usize] {return Err(StellarError::InvalidUtc)}
        let nanos=if b.len()==20 {0} else {
            if b[19]!=b'.' || b.len()<22 || b.len()>30 {return Err(StellarError::InvalidUtc)}
            let frac=&b[20..b.len()-1];
            if !frac.iter().all(u8::is_ascii_digit) || frac.is_empty() || frac.len()>9 {return Err(StellarError::InvalidUtc)}
            let mut n=0u32;for &c in frac {n=n*10+(c-b'0') as u32;}n*10u32.pow((9-frac.len()) as u32)
        };
        let days=days_from_civil(y,m,d);
        let seconds=days*86400+(hh*3600+mm*60+ss) as i64;
        let beginning=days_from_civil(2000,1,1)*86400;
        let end=days_from_civil(2051,1,1)*86400;
        if seconds<beginning || seconds>=end {return Err(StellarError::OutOfRangeUtc)}
        Ok(Self{unix_seconds:seconds,nanosecond:nanos})
    }
    pub fn from_unix_seconds(unix_seconds:i64)->Result<Self,StellarError> {
        let beginning=days_from_civil(2000,1,1)*86400;
        let end=days_from_civil(2051,1,1)*86400;
        if unix_seconds<beginning || unix_seconds>=end {return Err(StellarError::OutOfRangeUtc)}
        Ok(Self{unix_seconds,nanosecond:0})
    }
    pub fn unix_seconds(self)->i64{self.unix_seconds}
    pub fn nanosecond(self)->u32{self.nanosecond}
    pub(crate) fn fractional_unix(self)->f64 {self.unix_seconds as f64+self.nanosecond as f64*1e-9}
}

pub(crate) fn days_from_civil(y:i32,m:u32,d:u32)->i64 {
    let y=y as i64 - if m<=2 {1} else {0};
    let era=if y>=0 {y} else {y-399}/400;
    let yoe=y-era*400;
    let m=m as i64;
    let doy=(153*(m+if m>2 {-3} else {9})+2)/5+d as i64-1;
    let doe=yoe*365+yoe/4-yoe/100+doy;
    era*146097+doe-719468
}

pub(crate) const TIME_DATASET:&str="IERS-Bulletin-C-72_2026-07-06";
/// Last confirmed no-leap-second date is 2026-12-31 (valid through June 2027 before next potential insertion).
/// The transition table is complete for supported instants from 2000 onward as of Bulletin C 72.
const TRANSITIONS:&[(i32,u32,u32,i32)]=&[
    (1999,1,1,32),(2006,1,1,33),(2009,1,1,34),(2012,7,1,35),
    (2015,7,1,36),(2017,1,1,37),
];
pub(crate) fn julian_dates(utc:UtcInstant,flags:&mut QualityFlags)->(f64,f64) {
    let mut offset=32;
    for &(y,m,d,delta) in TRANSITIONS {if utc.unix_seconds>=days_from_civil(y,m,d)*86400 {offset=delta;} }
    if utc.unix_seconds>=days_from_civil(2027,7,1)*86400 {flags.insert(QualityFlags::FUTURE_LEAP_OFFSET_HELD);}
    let jd=2_440_587.5+utc.fractional_unix()/86_400.0;
    let tt=jd+(offset as f64+32.184)/86_400.0;
    (jd,tt)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sec_offset(s:&str)->(f64,QualityFlags) {
        let mut q=QualityFlags::default();let u=UtcInstant::parse(s).unwrap();
        let (utc,tt)=julian_dates(u,&mut q);((tt-utc)*86400.0,q)
    }
    #[test]
    fn utc_tt_real_offsets_and_authoritative_boundary(){
        for (s,tai) in [
            ("2000-01-01T00:00:00Z",32.0),
            ("2005-12-31T23:59:59Z",32.0),("2006-01-01T00:00:00Z",33.0),
            ("2008-12-31T23:59:59Z",33.0),("2009-01-01T00:00:00Z",34.0),
            ("2012-06-30T23:59:59Z",34.0),("2012-07-01T00:00:00Z",35.0),
            ("2015-06-30T23:59:59Z",35.0),("2015-07-01T00:00:00Z",36.0),
            ("2016-12-31T23:59:59Z",36.0),("2017-01-01T00:00:00Z",37.0),
            ("2027-06-30T23:59:59Z",37.0),("2027-07-01T00:00:00Z",37.0),
            ("2050-12-31T23:59:59.999999999Z",37.0)] {
            let (actual,flags)=sec_offset(s);
            assert!((actual-(tai+32.184)).abs()<0.0001,"{s}: {actual}");
            assert_eq!(flags.contains(QualityFlags::FUTURE_LEAP_OFFSET_HELD),s>="2027-07-01T00:00:00Z");
        }
    }
    #[test]
    fn supplied_ut1_shift_changes_local_meridian_by_expected_earth_rotation(){
        use super::super::{Observer,ObservationOptions,EarthOrientation,prepare_observation};
        let t=UtcInstant::parse("2026-07-01T12:00:00Z").unwrap();
        let ob=Observer::new(30.,0.,0.).unwrap();
        let base=prepare_observation(ob,t,ObservationOptions::default()).unwrap();
        let opts=ObservationOptions{earth_orientation:Some(EarthOrientation{
            ut1_minus_utc_seconds:Some(0.5),polar_motion_arcsec:Some((0.,0.)),
            valid_from:UtcInstant::parse("2026-01-01T00:00:00Z").unwrap(),
            valid_through:UtcInstant::parse("2026-12-31T23:59:59Z").unwrap(),
            provenance:"test-UT1"})};
        let later=prepare_observation(ob,t,opts).unwrap();
        let angular=base.east.cross(later.east).z.atan2(base.east.dot(later.east));
        let expected=0.5*2.0*std::f64::consts::PI*1.00273781191135448/86400.0;
        assert!((angular-expected).abs()<1e-8,"UT1 shift {angular} instead of {expected}");
        assert!(!later.quality.contains(QualityFlags::ASSUMED_ZERO_UT1));
    }
    #[test]
    fn unix_seconds_constructor_matches_strict_parser_for_integral_utc(){
        let parsed=UtcInstant::parse("2026-10-15T01:02:03Z").unwrap();
        assert_eq!(UtcInstant::from_unix_seconds(parsed.unix_seconds()).unwrap(),parsed);
        assert!(UtcInstant::from_unix_seconds(days_from_civil(1999,12,31)*86400).is_err());
        assert!(UtcInstant::from_unix_seconds(days_from_civil(2051,1,1)*86400).is_err());
    }
    #[test]
    fn fractional_tt_julian_date_changes_in_expected_direction(){
        let a=UtcInstant::parse("2026-10-15T01:02:03Z").unwrap();
        let b=UtcInstant::parse("2026-10-15T01:02:03.500000000Z").unwrap();
        let (_,tta)=julian_dates(a,&mut QualityFlags::default());
        let (_,ttb)=julian_dates(b,&mut QualityFlags::default());
        assert!(((ttb-tta)*86400.0-0.5).abs()<0.0001);
    }
    #[test]
    fn fractional_times_are_order_independent(){
        let a="2017-01-01T00:00:00.123456789Z";
        let b="2050-12-31T23:59:59.987654321Z";
        let earlier=sec_offset(a);let later=sec_offset(b);
        assert_eq!(earlier,sec_offset(a));
        assert_eq!(later,sec_offset(b));
        assert_eq!(earlier,sec_offset(a));
        let t=UtcInstant::parse(a).unwrap();
        assert_eq!(t.nanosecond(),123456789);
        assert!(UtcInstant::parse("2016-12-31T23:59:60Z").is_err());
    }
}
