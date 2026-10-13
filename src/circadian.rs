//! Linux-local clock: the celestial complication and game lighting share one timebase.
//! Once per two seconds, with no subprocess or per-frame system calls.
use std::time::{Duration, Instant};
#[cfg(not(unix))]
use std::process::Command;
#[cfg(test)]
use crate::world::FPS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase { Dawn, Day, Dusk, Night }
impl Phase {
    pub fn at(hour: u8) -> Self {
        match hour { 5..=8 => Self::Dawn, 9..=16 => Self::Day,
            17..=19 => Self::Dusk, _ => Self::Night }
    }
}

// Local weekday (0=Sunday) and day-of-month (1..31), from the same
// Linux-local time source as the hands and zoo lighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockDate {pub weekday:u8, pub day:u8}
impl ClockDate {
    pub fn weekday_name(self) -> &'static str {
        ["SUN","MON","TUE","WED","THU","FRI","SAT"][self.weekday as usize % 7]
    }
}

#[cfg(unix)]
fn local_datetime() -> Option<((u8,u8,u8),ClockDate)> {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&now, &mut tm) }.is_null() {return None;}
    let (h,m,s)=(tm.tm_hour,tm.tm_min,tm.tm_sec);
    if !(0..24).contains(&h) || !(0..60).contains(&m) || !(0..60).contains(&s)
        || !(0..7).contains(&tm.tm_wday) || !(1..=31).contains(&tm.tm_mday) {return None;}
    Some(((h as u8,m as u8,s as u8),ClockDate {
        weekday:tm.tm_wday as u8, day:tm.tm_mday as u8 }))
}
#[cfg(not(unix))]
fn local_datetime() -> Option<((u8,u8,u8),ClockDate)> {
    let out=Command::new("date").arg("+%H:%M:%S:%w:%d").output().ok()?;
    if !out.status.success(){return None;}
    let text=String::from_utf8(out.stdout).ok()?;
    let mut parts=text.trim().split(':');
    let (h,m,s,w,d)=(parts.next()?.parse::<u8>().ok()?,
        parts.next()?.parse::<u8>().ok()?,parts.next()?.parse::<u8>().ok()?,
        parts.next()?.parse::<u8>().ok()?,parts.next()?.parse::<u8>().ok()?);
    if h<24 && m<60 && s<60 && w<7 && (1..=31).contains(&d) {
        Some(((h,m,s),ClockDate{weekday:w,day:d}))
    } else {None}
}
#[cfg(test)]
fn local_hms() -> Option<(u8,u8,u8)> {local_datetime().map(|(hms,_)|hms)}

pub struct CircadianClock {
    hms: (u8,u8,u8),
    date: ClockDate,
    override_hour: Option<u8>,
    refresh_at: Instant,
}
impl CircadianClock {
    pub fn new(override_hour: Option<u8>) -> Self {
        // Frozen --hour places the dial at :30, useful for viewing a celestial arc.
        let local=local_datetime();
        let hms=override_hour.map(|h|(h,30,0))
            .or_else(||local.map(|(hms,_)|hms)).unwrap_or((12,30,0));
        let date=local.map(|(_,date)|date).unwrap_or(ClockDate{weekday:0,day:1});
        Self { hms, date, override_hour, refresh_at:Instant::now()+Duration::from_secs(2) }
    }
    pub fn advance(&mut self,_tick:u64) {
        if Instant::now()>=self.refresh_at {
            if let Some((next,date))=local_datetime() {
                if self.override_hour.is_none(){self.hms=next;}
                self.date=date;
            }
            self.refresh_at=Instant::now()+Duration::from_secs(2);
        }
    }
    /// Same calendar date/time as the face clock for constellation position.
    /// Normal play uses exact Unix seconds. In --hour preview, shift only the
    /// star atlas to that displayed local hour, preserving the local date.
    pub fn observing_unix(&self,unix:u64)->i64 {
        if self.override_hour.is_none() {return unix as i64;}
        #[cfg(unix)]
        {
            let raw=unix as libc::time_t;
            let mut tm:libc::tm=unsafe{std::mem::zeroed()};
            if unsafe{libc::localtime_r(&raw,&mut tm)}.is_null(){return unix as i64;}
            let current=tm.tm_hour as i64*3600+tm.tm_min as i64*60+tm.tm_sec as i64;
            return unix as i64+self.seconds_today() as i64-current;
        }
        #[cfg(not(unix))]
        {unix as i64}
    }
    pub fn date(&self)->ClockDate {self.date}
    pub fn hour(&self)->u8 {self.hms.0}
    pub fn minute(&self)->u8 {self.hms.1}
    pub fn seconds_today(&self)->u32 {
        self.hms.0 as u32*3600+self.hms.1 as u32*60+self.hms.2 as u32
    }
    pub fn phase(&self)->Phase {Phase::at(self.hour())}
    pub fn is_day(&self)->bool {(6..18).contains(&self.hour())}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn phases_cover_entire_day(){
        for (hour,phase) in [(0,Phase::Night),(5,Phase::Dawn),(9,Phase::Day),(17,Phase::Dusk),(20,Phase::Night)] {
            assert_eq!(Phase::at(hour),phase);
        }
    }
    #[test] fn native_hour_is_in_range(){let (h,m,s)=local_hms().unwrap(); assert!(h<24&&m<60&&s<60);}
    #[test] fn date_complication_has_valid_linux_local_calendar(){
        let c=CircadianClock::new(None);
        assert!(c.date().weekday<7 && (1..=31).contains(&c.date().day));
        assert_eq!(ClockDate{weekday:0,day:11}.weekday_name(),"SUN");
        assert_eq!(ClockDate{weekday:6,day:31}.weekday_name(),"SAT");
    }
    #[test] fn fixed_demo_hour_is_stable(){
        let mut c=CircadianClock::new(Some(22));c.advance(FPS*86400);
        assert_eq!((c.hour(),c.minute(),c.seconds_today()),(22,30,81000));
    }
}
