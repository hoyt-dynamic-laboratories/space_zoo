//! Time Machine command grammars plus the modern Command Center state.
//! The locked ASCII path keeps the legacy Vim grammar through `parse`; pixel
//! framebuffer/X11 use the deterministic modern grammar through `parse_modern`.
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crate::{astronomy::Location, sky::SkyState};

pub const MAX_UNIX:u64=253_402_300_799; // 9999-12-31 23:59:59 UTC
pub(crate) const MAX_COMMAND:usize=128;
pub(crate) const PRESET_FAST_RATE:f64=3600.0;
pub const MENU_VISIBLE_ROWS:usize=27;

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum CryptidMode {Cycle,Night,Off}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum PlanetAppearance {Graphic,Point}

#[derive(Debug,Clone,PartialEq)]
pub enum Command {
    // Locked legacy/engine actions.
    Forward,Backward,Stop,Normal,NormalBackward,Rate(f64),ReverseRate(f64),Date(u64),TimeOfDay(u32),
    Location(Location),Stars(f64),Quit,
    // Shared visual actions.
    Culture(String),CultureQuery,Cultures,DebugStars(bool),DebugStarsQuery,Planet(String),ListPlanets,
    // Modern Command Center actions.
    Help,StarsQuery,Cryptids(CryptidMode),PlanetAppearance(PlanetAppearance),Jumbotron(bool),Hud(bool),Chickens(bool),
    Named(String),CelestialPlaceholder,
}

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum MenuView {Main,Help,Planets,Cultures}
impl Default for MenuView {fn default()->Self{Self::Main}}
impl MenuView {
    pub fn title(self)->&'static str {match self {Self::Main=>"SPACE TIME MACHINE",Self::Help=>"HELP",Self::Planets=>"PLANETS",Self::Cultures=>"CULTURES"}}
}

#[derive(Debug,Clone,PartialEq,Eq)]
pub enum CommandReply {Applied,Info(String),View(MenuView,Vec<String>)}

#[derive(Debug,Clone,PartialEq,Eq)]
pub struct MenuLine {pub text:String,pub heading:bool,pub muted:bool}
impl MenuLine {
    fn row(text:impl Into<String>)->Self {Self{text:text.into(),heading:false,muted:false}}
    fn muted(text:impl Into<String>)->Self {Self{text:text.into(),heading:false,muted:true}}
    fn heading(text:impl Into<String>)->Self {Self{text:text.into(),heading:true,muted:false}}
}

fn leap(y:i64)->bool {y%4==0 && (y%100!=0 || y%400==0)}
fn days_in_month(y:i64,m:u32)->u32 {match m {1|3|5|7|8|10|12=>31,4|6|9|11=>30,2=>if leap(y){29}else{28},_=>0}}
fn days_from_civil(year:i64,month:u32,day:u32)->i64 {
    let y=year-if month<=2 {1}else{0};let era=y.div_euclid(400);let yoe=y-era*400;
    let mp=month as i64+if month>2 {-3}else{9};let doy=(153*mp+2)/5+day as i64-1;
    let doe=yoe*365+yoe/4-yoe/100+doy;era*146097+doe-719468
}
fn date_parts(y:i64,m:u32,d:u32,h:u32,min:u32,sec:u32)->Result<u64,String> {
    if !(1970..=9999).contains(&y)||d==0||d>days_in_month(y,m)||h>23||min>59||sec>59 {
        return Err("invalid date/time (1970..9999, valid calendar and 24h clock)".into());
    }
    let unix=days_from_civil(y,m,d)*86_400+h as i64*3600+min as i64*60+sec as i64;
    u64::try_from(unix).map_err(|_|"date/time outside supported range".into())
}
fn parse_hms(t:&str)->Result<(u32,u32,u32),String> {
    let p:Vec<&str>=t.split(':').collect();
    if !(2..=3).contains(&p.len())||p.iter().any(|v|v.len()!=2){return Err("use two-digit HH:MM or HH:MM:SS".into());}
    let h=p[0].parse::<u32>().map_err(|_|"bad hour")?;let m=p[1].parse::<u32>().map_err(|_|"bad minute")?;
    let s=if p.len()==3 {p[2].parse::<u32>().map_err(|_|"bad second")?} else {0};
    if h>23||m>59||s>59{return Err("invalid time (24h clock)".into());} Ok((h,m,s))
}
fn parse_compact_time(t:&str)->Result<(u32,u32,u32),String> {
    if !t.bytes().all(|b|b.is_ascii_digit())||!(t.len()==4||t.len()==6){return Err("use HHMM or HHMMSS".into());}
    let h=t[0..2].parse::<u32>().map_err(|_|"bad hour")?;let m=t[2..4].parse::<u32>().map_err(|_|"bad minute")?;
    let s=if t.len()==6 {t[4..6].parse::<u32>().map_err(|_|"bad second")?} else {0};
    if h>23||m>59||s>59{return Err("invalid time (24h clock)".into());} Ok((h,m,s))
}
fn parse_compact_date(d:&str)->Result<(i64,u32,u32),String> {
    if d.len()!=8||!d.bytes().all(|b|b.is_ascii_digit()){return Err("use YYYYMMDD".into());}
    Ok((d[0..4].parse().map_err(|_|"bad year")?,d[4..6].parse().map_err(|_|"bad month")?,d[6..8].parse().map_err(|_|"bad day")?))
}
fn parse_separated_date(d:&str)->Result<(i64,u32,u32),String> {
    if d.len()!=10{return Err("use YYYY-MM-DD or YYYY/MM/DD".into());}
    let bytes=d.as_bytes();let sep=bytes[4];
    if !matches!(sep,b'-'|b'/')||bytes[7]!=sep{return Err("use consistent YYYY-MM-DD or YYYY/MM/DD separators".into());}
    if d.as_bytes().iter().enumerate().any(|(i,b)|i!=4&&i!=7&&!b.is_ascii_digit()) {return Err("date fields must be numeric".into());}
    Ok((d[0..4].parse().map_err(|_|"bad year")?,d[5..7].parse().map_err(|_|"bad month")?,d[8..10].parse().map_err(|_|"bad day")?))
}
fn parse_modern_datetime(text:&str)->Result<Option<Command>,String> {
    let parts:Vec<&str>=text.split_whitespace().collect();
    if parts.len()>2{return Ok(None);}
    if parts.len()==2 {
        let (y,m,d)=if parts[0].len()==8 {parse_compact_date(parts[0])?} else {parse_separated_date(parts[0])?};
        let (h,min,sec)=if parts[1].contains(':'){parse_hms(parts[1])?}else{parse_compact_time(parts[1])?};
        return date_parts(y,m,d,h,min,sec).map(Command::Date).map(Some);
    }
    if parts.len()!=1{return Ok(None);}
    let one=parts[0];
    if one.contains('-')||one.contains('/') {
        let (y,m,d)=parse_separated_date(one)?;return date_parts(y,m,d,0,0,0).map(Command::Date).map(Some);
    }
    if one.contains(':') {
        let (h,m,s)=parse_hms(one)?;return Ok(Some(Command::TimeOfDay(h*3600+m*60+s)));
    }
    if !one.bytes().all(|b|b.is_ascii_digit()){return Ok(None);}
    match one.len() {
        4|6=>{let (h,m,s)=parse_compact_time(one)?;Ok(Some(Command::TimeOfDay(h*3600+m*60+s)))},
        8=>{let (y,m,d)=parse_compact_date(one)?;date_parts(y,m,d,0,0,0).map(Command::Date).map(Some)},
        12|14=>{let (date,time)=one.split_at(8);let (y,m,d)=parse_compact_date(date)?;let (h,min,s)=parse_compact_time(time)?;date_parts(y,m,d,h,min,s).map(Command::Date).map(Some)},
        _=>Err("unsupported numeric date/time length; use HHMM, HHMMSS, YYYYMMDD, YYYYMMDDHHMM, or YYYYMMDDHHMMSS".into()),
    }
}
fn positive_factor(text:&str,example:&str)->Result<f64,String> {
    let factor=text.parse::<f64>().map_err(|_|format!("use {example}"))?;
    if !factor.is_finite()||factor<=0.0{return Err("speed factor must be finite and positive".into());} Ok(factor)
}

// Locked ASCII/Vim grammar. Do not route modern pixel commands through this function.
pub fn parse(input:&str)->Result<Command,String> {
    let text=input.trim();let mut words=text.split_whitespace();
    if let Some(word)=words.next() {
        if word.eq_ignore_ascii_case("culture") {let arg=words.next();if words.next().is_some(){return Err("use :culture <id>, :culture off, or :culture".into());}return Ok(match arg{Some(id)=>Command::Culture(id.to_ascii_lowercase()),None=>Command::CultureQuery});}
        if word.eq_ignore_ascii_case("cultures") {if words.next().is_some(){return Err("use :cultures".into());}return Ok(Command::Cultures);}
        if word.eq_ignore_ascii_case("debugstars") {let arg=words.next();if words.next().is_some(){return Err("use :debugstars on/off or :debugstars".into());}return match arg{None=>Ok(Command::DebugStarsQuery),Some(v) if v.eq_ignore_ascii_case("on")=>Ok(Command::DebugStars(true)),Some(v) if v.eq_ignore_ascii_case("off")=>Ok(Command::DebugStars(false)),_=>Err("use :debugstars on/off or :debugstars".into())};}
        if word.eq_ignore_ascii_case("planets") {let name=words.next().ok_or("use :planets <planet>")?;if words.next().is_some(){return Err("use :planets <planet>".into());}return Ok(Command::Planet(name.to_ascii_lowercase()));}
        if word.eq_ignore_ascii_case("list") {let what=words.next().ok_or("use :list planets")?;if words.next().is_some()||!what.eq_ignore_ascii_case("planets"){return Err("use :list planets".into());}return Ok(Command::ListPlanets);}
    }
    let mut words=text.split_whitespace();if words.next().is_some_and(|w|w.eq_ignore_ascii_case("stars")) {let value=words.next().ok_or("use :stars # or :stars reset")?;if words.next().is_some(){return Err("use :stars # or :stars reset".into());}if value.eq_ignore_ascii_case("reset"){return Ok(Command::Stars(crate::display_settings::DEFAULT_STAR_MULTIPLIER));}let multiplier=value.parse::<f64>().map_err(|_|"star brightness multiplier must be numeric")?;if !multiplier.is_finite()||multiplier<=0.0||multiplier>crate::display_settings::MAX_STAR_MULTIPLIER{return Err(format!("star brightness multiplier must be finite, positive, and at most {:.0}",crate::display_settings::MAX_STAR_MULTIPLIER));}return Ok(Command::Stars(multiplier));}
    let upper=text.to_ascii_uppercase();match upper.as_str(){"FF"=>return Ok(Command::Forward),"BB"=>return Ok(Command::Backward),"P"=>return Ok(Command::Stop),"N"=>return Ok(Command::Normal),"R"=>return Ok(Command::NormalBackward),"Q!"=>return Ok(Command::Quit),_=>{}}
    if let Some(rest)=upper.strip_prefix('R'){if !rest.is_empty(){return positive_factor(rest,":R3 or :R0.5").map(Command::ReverseRate);}}
    if text.contains(','){let (a,b)=text.split_once(',').unwrap();let lat=a.trim().parse::<f64>().map_err(|_|"latitude must be numeric")?;let lon=b.trim().parse::<f64>().map_err(|_|"longitude must be numeric")?;return Location::new(lat,lon).map(Command::Location);}
    if text.len()>=10&&text.as_bytes().get(4)==Some(&b'-')&&text.as_bytes().get(7)==Some(&b'-'){let parts:Vec<&str>=text.split_whitespace().collect();let (y,m,d)=parse_separated_date(parts[0])?;let (h,min,s)=if parts.len()==2 {parse_hms(parts[1])?}else if parts.len()==1{(0,0,0)}else{return Err("use YYYY-MM-DD [HH:MM[:SS]]".into())};return date_parts(y,m,d,h,min,s).map(Command::Date);}
    if text.contains(':'){let (h,m,s)=parse_hms(text)?;return Ok(Command::TimeOfDay(h*3600+m*60+s));}
    if !text.is_empty(){if let Ok(factor)=positive_factor(text,":3 or :0.5"){return Ok(Command::Rate(factor));}}
    Err("use :planets <name>, :list planets, :culture, :cultures, :debugstars, :FF, :BB, :P, :stars #, time, date, location, or :q!".into())
}

fn parse_switch(word:Option<&str>,usage:&str)->Result<bool,String>{match word{Some(v) if v.eq_ignore_ascii_case("on")=>Ok(true),Some(v) if v.eq_ignore_ascii_case("off")=>Ok(false),_=>Err(usage.into())}}
fn parse_speed_x(text:&str)->Result<Option<Command>,String>{
    let Some(raw)=text.strip_suffix('x').or_else(||text.strip_suffix('X')) else{return Ok(None)};
    if raw.is_empty()||raw.contains('e')||raw.contains('E'){return Err("use a decimal speed such as :3x, :-3x, :0x, or :0.5x".into());}
    if raw.chars().filter(|c|*c=='.').count()>1||!raw.chars().enumerate().all(|(i,c)|c.is_ascii_digit()||c=='.'||((c=='-'||c=='+')&&i==0)){return Err("invalid speed; use signed decimal NUMBERx".into());}
    let rate=raw.parse::<f64>().map_err(|_|"invalid speed; use signed decimal NUMBERx")?;
    if !rate.is_finite(){return Err("speed must be finite".into());}
    if rate==0.0{return Ok(Some(Command::Rate(0.0)));}
    let mag=rate.abs();if !(0.000001..=100_000_000.0).contains(&mag){return Err("speed magnitude must be between 0.000001x and 100000000x".into());}
    Ok(Some(Command::Rate(rate)))
}
fn valid_hip(text:&str)->Result<bool,String>{
    if !text.get(..3).is_some_and(|p|p.eq_ignore_ascii_case("hip")){return Ok(false)}
    let rest=&text[3..];if rest.is_empty()||!rest.bytes().all(|b|b.is_ascii_digit()){return Err("HIP lookup requires HIP followed by a positive catalogue number".into());}
    if rest.parse::<u64>().ok().filter(|v|*v>0).is_none(){return Err("HIP catalogue number must be positive".into());}Ok(true)
}
fn valid_rd(text:&str)->Result<bool,String>{
    if !text.get(..2).is_some_and(|p|p.eq_ignore_ascii_case("rd")){return Ok(false)}
    let rest=&text[2..];let (ra,dec)=rest.split_once(',').ok_or("RD lookup requires RD<ra>,<dec>")?;
    let ra=ra.parse::<f64>().map_err(|_|"RD right ascension must be numeric")?;let dec=dec.parse::<f64>().map_err(|_|"RD declination must be numeric")?;
    if !ra.is_finite()||!dec.is_finite()||!(0.0..360.0).contains(&ra)||!(-90.0..=90.0).contains(&dec){return Err("RD requires 0 <= RA < 360 and -90 <= Dec <= 90".into());}Ok(true)
}
fn valid_sexagesimal(text:&str)->Result<bool,String>{
    let normalized=text.to_ascii_lowercase();
    let text=normalized.as_str();
    // Recognition is intentionally strict. Registered names such as "chinese",
    // "earth" and "hawaiian_starlines" must reach Command::Named instead of
    // being mistaken for coordinates merely because they contain the letter h.
    let Some((ra,dec))=text.split_once(',') else{return Ok(false)};
    let looks_ra=ra.contains('h')&&ra.contains('m')&&ra.ends_with('s');
    let looks_dec=(dec.starts_with('+')||dec.starts_with('-'))&&
        (dec.contains('°')||dec.contains('d'))&&dec.contains('\'');
    if !looks_ra||!looks_dec{return Ok(false)}
    let h=ra.find('h').ok_or("RA requires h")?;let m=ra[h+1..].find('m').map(|i|h+1+i).ok_or("RA requires m")?;let s=ra[m+1..].find('s').map(|i|m+1+i).ok_or("RA requires s")?;
    if s+1!=ra.len(){return Err("unexpected text after RA seconds".into());}
    let hh=ra[..h].parse::<u32>().map_err(|_|"bad RA hours")?;let mm=ra[h+1..m].parse::<u32>().map_err(|_|"bad RA minutes")?;let ss=ra[m+1..s].parse::<f64>().map_err(|_|"bad RA seconds")?;
    if hh>=24||mm>=60||!ss.is_finite()||!(0.0..60.0).contains(&ss){return Err("RA fields outside ordinary ranges".into());}
    if dec.len()<3{return Err("declination requires sign and degrees".into());}let sign=match dec.as_bytes()[0]{b'+'=>1.0,b'-'=>-1.0,_=>return Err("declination requires + or - sign".into())};
    let body=&dec[1..];let mark=body.find('°').or_else(||body.find('d')).ok_or("declination requires degree marker ° or d")?;let deg=body[..mark].parse::<u32>().map_err(|_|"bad declination degrees")?;
    let tail=&body[mark+body[mark..].chars().next().unwrap().len_utf8()..];let apos=tail.find('\'').ok_or("declination requires minutes followed by '")?;let min=tail[..apos].parse::<u32>().map_err(|_|"bad declination minutes")?;let rem=&tail[apos+1..];
    let sec=if rem.is_empty(){0.0}else{let q=rem.strip_suffix('"').ok_or("declination seconds must end with double quote")?;q.parse::<f64>().map_err(|_|"bad declination seconds")?};
    if deg>90||min>=60||!sec.is_finite()||!(0.0..60.0).contains(&sec)||(deg==90&&(min!=0||sec!=0.0)){return Err("declination fields outside ordinary ranges".into());}
    let _=sign;Ok(true)
}

pub fn parse_modern(input:&str)->Result<Command,String>{
    let text=input.trim().trim_start_matches(':').trim();if text.is_empty(){return Err("enter a command".into());}
    let mut words=text.split_whitespace();let first=words.next().unwrap();let rest:Vec<&str>=words.collect();
    if first.eq_ignore_ascii_case("help"){if !rest.is_empty(){return Err("use :help".into())}return Ok(Command::Help)}
    if first.eq_ignore_ascii_case("planets"){
        if rest.is_empty(){return Ok(Command::ListPlanets)}if rest.len()!=1{return Err("use :planets, :planets <name>, or :planets graphic/point".into())}
        return Ok(match rest[0].to_ascii_lowercase().as_str(){"graphic"=>Command::PlanetAppearance(PlanetAppearance::Graphic),"point"=>Command::PlanetAppearance(PlanetAppearance::Point),name=>Command::Planet(name.into())});
    }
    if first.eq_ignore_ascii_case("list") {if rest.len()!=1{return Err("use :list planets or :list cultures".into())}return if rest[0].eq_ignore_ascii_case("planets"){Ok(Command::ListPlanets)}else if rest[0].eq_ignore_ascii_case("cultures"){Ok(Command::Cultures)}else{Err("use :list planets or :list cultures".into())};}
    if first.eq_ignore_ascii_case("culture") {if rest.is_empty(){return Ok(Command::Cultures)}if rest.len()!=1{return Err("use :culture <id>, :culture off, or :culture".into())}return Ok(Command::Culture(rest[0].to_ascii_lowercase()));}
    if first.eq_ignore_ascii_case("cultures"){if !rest.is_empty(){return Err("use :cultures".into())}return Ok(Command::Cultures)}
    if first.eq_ignore_ascii_case("stars") {if rest.is_empty(){return Ok(Command::StarsQuery)}if rest.len()!=1{return Err("use :stars, :stars NUMBER, or :stars reset".into())}if rest[0].eq_ignore_ascii_case("reset"){return Ok(Command::Stars(crate::display_settings::DEFAULT_STAR_MULTIPLIER))}let v=rest[0].parse::<f64>().map_err(|_|"star brightness multiplier must be numeric")?;if !v.is_finite()||v<=0.0||v>crate::display_settings::MAX_STAR_MULTIPLIER{return Err(format!("star brightness multiplier must be finite, positive, and at most {:.0}",crate::display_settings::MAX_STAR_MULTIPLIER));}return Ok(Command::Stars(v));}
    if first.eq_ignore_ascii_case("cryptids") {if rest.len()!=1{return Err("use :cryptids cycle, :cryptids night, or :cryptids off".into())}return Ok(Command::Cryptids(match rest[0].to_ascii_lowercase().as_str(){"cycle"=>CryptidMode::Cycle,"night"=>CryptidMode::Night,"off"=>CryptidMode::Off,_=>return Err("use :cryptids cycle, :cryptids night, or :cryptids off".into())}));}
    if first.eq_ignore_ascii_case("jumbotron") {if rest.len()!=1{return Err("use :jumbotron on/off".into())}return parse_switch(rest.first().copied(),"use :jumbotron on/off").map(Command::Jumbotron)}
    if first.eq_ignore_ascii_case("hud") {if rest.len()!=1{return Err("use :hud on/off".into())}return parse_switch(rest.first().copied(),"use :hud on/off").map(Command::Hud)}
    if first.eq_ignore_ascii_case("chickens") {if rest.len()!=1{return Err("use :chickens on/off".into())}return parse_switch(rest.first().copied(),"use :chickens on/off").map(Command::Chickens)}
    if first.eq_ignore_ascii_case("debugstars") {if rest.len()!=1{return Err("use :debugstars on/off".into())}return parse_switch(rest.first().copied(),"use :debugstars on/off").map(Command::DebugStars)}
    if first.eq_ignore_ascii_case("now"){if !rest.is_empty(){return Err("use :now".into())}return Ok(Command::Normal)}
    if text.eq_ignore_ascii_case("q!"){return Ok(Command::Quit)}
    // Hidden compatibility aliases, all mapped to existing single engine actions.
    let upper=text.to_ascii_uppercase();match upper.as_str(){"FF"=>return Ok(Command::Rate(PRESET_FAST_RATE)),"BB"=>return Ok(Command::Rate(-PRESET_FAST_RATE)),"P"=>return Ok(Command::Rate(0.0)),"N"=>return Ok(Command::Normal),"R"=>return Ok(Command::Rate(-1.0)),_=>{}}
    if let Some(raw)=upper.strip_prefix('R'){if !raw.is_empty()&&raw.bytes().all(|b|b.is_ascii_digit()||b==b'.'){return positive_factor(raw,":R3 or :R0.5").map(|factor|Command::Rate(-factor))}}
    if let Some(speed)=parse_speed_x(text)?{return Ok(speed)}
    if text.eq_ignore_ascii_case("vega")||text.eq_ignore_ascii_case("lyra"){return Ok(Command::CelestialPlaceholder)}
    if valid_hip(text)?||valid_rd(text)?||valid_sexagesimal(text)?{return Ok(Command::CelestialPlaceholder)}
    // Earth coordinates are deliberately ahead of generic comma/name handling.
    if text.contains(',') {let (a,b)=text.split_once(',').ok_or("use latitude,longitude")?;let lat=a.trim().parse::<f64>().map_err(|_|"latitude must be numeric")?;let lon=b.trim().parse::<f64>().map_err(|_|"longitude must be numeric")?;return Location::new(lat,lon).map(Command::Location);}
    if let Some(dt)=parse_modern_datetime(text)?{return Ok(dt)}
    if text.split_whitespace().count()==1{return Ok(Command::Named(text.to_ascii_lowercase()))}
    Err("unknown command; use :help".into())
}

pub fn format_utc(utc:u64)->String {
    #[cfg(unix)] {let raw=utc as libc::time_t;let mut tm:libc::tm=unsafe{std::mem::zeroed()};if !unsafe{libc::gmtime_r(&raw,&mut tm)}.is_null(){return format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}",tm.tm_year+1900,tm.tm_mon+1,tm.tm_mday,tm.tm_hour,tm.tm_min,tm.tm_sec);}}
    format!("{utc}")
}

const MENU_GLYPH_ADVANCE_PX:usize=6;
const MAIN_LEFT_COLUMN_PX:usize=300;
const MAIN_COMMAND_PX:usize=120;
const DEFAULT_MENU_CHARS:usize=102;
const COMPACT_MAIN_THRESHOLD_CHARS:usize=100;
fn pad_to_pixels(text:&str,target_px:usize)->String {
    let width=text.chars().count()*MENU_GLYPH_ADVANCE_PX;
    let spaces=target_px.saturating_sub(width).div_ceil(MENU_GLYPH_ADVANCE_PX);
    format!("{text}{}"," ".repeat(spaces))
}
fn menu_entry(command:&str,description:&str)->String {format!("{}{}",pad_to_pixels(command,MAIN_COMMAND_PX),description)}
fn two_columns(left:&str,right:&str)->String {format!("{}{}",pad_to_pixels(left,MAIN_LEFT_COLUMN_PX),right)}
fn main_lines()->Vec<MenuLine>{
    let row=|lc:&str,ld:&str,rc:&str,rd:&str|two_columns(&menu_entry(lc,ld),&menu_entry(rc,rd));
    vec![
        MenuLine::heading(two_columns("LOCATION + VIEW","DATE + TIME / UTC")),
        MenuLine::row(row(":latitude,longitude","Go anywhere",":20261024 143000","Date + time")),
        MenuLine::muted(row("  e.g. :-60,-130","",":1430","Time on simulated date")),
        MenuLine::row(row("WASD","Look around",":3x / :-3x","Forward / back at 3x")),
        MenuLine::row(row("ARROWS","Move on Earth",":0x / :1x","Pause / normal speed")),
        MenuLine::row(row("R","Reset camera only",":now","Return to now")),
        MenuLine::row(""),
        MenuLine::heading(two_columns("EXPLORE","REFERENCE")),
        MenuLine::row(row(":venus","Go to a planet",":help","All commands + scene controls")),
        MenuLine::row(menu_entry(":chinese","Select a culture")),
        MenuLine::row(menu_entry(":planets","Planet directory")),
        MenuLine::row(menu_entry(":cultures","Culture directory")),
    ]
}
fn main_lines_compact()->Vec<MenuLine>{
    let row=|command:&str,description:&str|MenuLine::row(menu_entry(command,description));
    vec![
        MenuLine::heading("LOCATION + VIEW"),
        row(":latitude,longitude","Go anywhere"),
        MenuLine::muted(menu_entry("  e.g. :-60,-130","")),
        row("WASD","Look around"),row("ARROWS","Move on Earth"),row("R","Reset camera only"),
        MenuLine::row(""),MenuLine::heading("DATE + TIME / UTC"),
        row(":20261024 143000","Date + time"),row(":1430","Time on simulated date"),
        row(":3x / :-3x","Forward / back at 3x"),row(":0x / :1x","Pause / normal speed"),row(":now","Return to now"),
        MenuLine::row(""),MenuLine::heading("EXPLORE + REFERENCE"),
        row(":venus","Go to a planet"),row(":chinese","Select a culture"),row(":planets","Planet directory"),
        row(":cultures","Culture directory"),row(":help","All commands + scene controls"),
    ]
}
fn help_lines()->Vec<MenuLine>{
    let groups:[(&str,&[(&str,&str)]);6]=[
        ("TIME / UTC",&[(":NUMBERx","Signed absolute speed; 0x pauses; fractions accepted"),(":now","Real current UTC; forward at 1x"),(":HHMM / :HHMMSS","Time on current simulated UTC date"),(":YYYYMMDD","Date at midnight"),(":YYYYMMDD HHMM[SS]","Date + time"),(":YYYYMMDDHHMM[SS]","Joined date + time"),(":YYYY-MM-DD [HH:MM[:SS]]","Separated date + optional time"),(":YYYY/MM/DD [HH:MM[:SS]]","Year-first slash-date equivalent"),(":HH:MM[:SS]","Separated time; date/time jumps preserve speed")]),
        ("LOCATION + CAMERA",&[(":latitude,longitude","Set observer location; latitude first"),("W/S, A/D","Look up/down, left/right outside menu"),("ARROW KEYS","Move north/south/west/east outside menu"),("R","Reset camera; retain location and time")]),
        ("SKY + DIRECTORIES",&[(":venus / :planets venus","Same planet-navigation action"),(":planets / :list planets","Planet directory"),(":chinese / :culture chinese","Select exact registered culture"),(":culture / :cultures / :list cultures","Culture directory; active marked"),(":culture off","Hide constellation lines + names"),(":stars","Brightness: current, default, allowed range"),(":stars NUMBER / :stars reset","Set brightness / restore 3x; max 30x")]),
        ("SCENE CONTROLS",&[(":cryptids cycle / night / off","Continuous / Night phase only / hidden"),(":chickens on / off","All background day + night animals"),(":planets graphic / point","Sprite artwork / point markers"),(":jumbotron on / off","Telemetry billboard visibility"),(":hud on / off","Time, camera, observer/navigation overlays")]),
        ("CELESTIAL LOOKUP / PLANNED",&[(":VEGA","Star-name placeholder"),(":LYRA","Constellation-name placeholder"),(":HIP91262","Hipparcos-ID placeholder"),(":RD279.233,38.7836","RA/Dec decimal degrees placeholder"),(":18h36m56s,+38°47'","RA hours / Dec degrees placeholder"),("NOT ACTIVE YET","Recognized forms change no state")]),
        ("SYSTEM",&[(":debugstars on / off","Explicit diagnostic display control"),(":help","Full reference"),(":q!","IYKYK")]),
    ];
    let mut out=Vec::new();for (gi,(group,rows)) in groups.iter().enumerate(){if gi>0{out.push(MenuLine::row(""));}out.push(MenuLine::heading(*group));for (syntax,desc) in *rows{out.push(MenuLine::row(format!("{syntax:<31} {desc}")));}}out
}
fn dynamic_lines(view:MenuView,content:Vec<String>)->Vec<MenuLine>{
    let mut out=Vec::new();match view{MenuView::Planets=>out.push(MenuLine::heading("PLANETS / NAME · ALT · AZ")),MenuView::Cultures=>{},_=>{}}
    out.extend(content.into_iter().map(MenuLine::row));out
}
fn wrap_text(text:&str,max_chars:usize)->Vec<String>{
    if text.chars().count()<=max_chars{return vec![text.to_string()]}
    let max_chars=max_chars.max(8);let mut lines=Vec::new();let mut current=String::new();
    for word in text.split_whitespace() {
        if word.chars().count()>max_chars {
            if !current.is_empty(){lines.push(std::mem::take(&mut current));}
            let chars=word.chars().collect::<Vec<_>>();for chunk in chars.chunks(max_chars){lines.push(chunk.iter().collect());}
            continue;
        }
        let needed=word.chars().count()+usize::from(!current.is_empty());
        if current.chars().count()+needed>max_chars {lines.push(std::mem::take(&mut current));}
        if !current.is_empty(){current.push(' ');}current.push_str(word);
    }
    if !current.is_empty(){lines.push(current);}if lines.is_empty(){lines.push(String::new());}lines
}
fn wrap_menu_lines(lines:Vec<MenuLine>,max_chars:usize)->Vec<MenuLine>{
    let mut out=Vec::new();for line in lines {let wrapped=wrap_text(&line.text,max_chars);for (i,text) in wrapped.into_iter().enumerate(){out.push(MenuLine{text,heading:line.heading&&i==0,muted:line.muted});}}out
}

#[derive(Debug,Clone,PartialEq,Eq)]
pub struct Panel {
    pub open:bool,pub editing:bool,pub input:String,pub error:String,pub pending:bool,
    pub modern:bool,pub view:MenuView,pub scroll:usize,pub message:String,pub lines:Vec<MenuLine>,
    esc_ready:bool,viewport_rows:usize,viewport_chars:usize,view_content:Vec<String>,
}
impl Default for Panel {fn default()->Self{Self{open:false,editing:false,input:String::new(),error:String::new(),pending:false,modern:false,view:MenuView::Main,scroll:0,message:String::new(),lines:Vec::new(),esc_ready:true,viewport_rows:MENU_VISIBLE_ROWS,viewport_chars:DEFAULT_MENU_CHARS,view_content:Vec::new()}}}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum QuitKind {CtrlC,Command}
#[derive(Debug,PartialEq,Eq)]
pub enum Captured {Submit(String),Quit(QuitKind)}
impl Panel {
    pub fn modern()->Self{let mut p=Self::default();p.modern=true;p.rebuild_lines();p}
    fn close(&mut self){self.open=false;self.editing=false;self.input.clear();self.error.clear();self.message.clear();self.pending=false;self.scroll=0;}
    fn open_legacy(&mut self){self.open=true;self.editing=false;self.input.clear();self.error.clear();self.pending=false;}
    pub fn open_modern(&mut self){self.modern=true;self.open=true;self.editing=true;self.view=MenuView::Main;self.scroll=0;self.input.clear();self.error.clear();self.message.clear();self.pending=false;self.view_content.clear();self.rebuild_lines();self.esc_ready=false;}
    pub fn release_escape(&mut self){self.esc_ready=true;}
    fn rebuild_lines(&mut self){
        let base=match self.view {
            MenuView::Main=>if self.viewport_chars<COMPACT_MAIN_THRESHOLD_CHARS{main_lines_compact()}else{main_lines()},
            MenuView::Help=>help_lines(),
            MenuView::Planets|MenuView::Cultures=>dynamic_lines(self.view,self.view_content.clone()),
        };
        self.lines=wrap_menu_lines(base,self.viewport_chars.max(16));
        let max=self.lines.len().saturating_sub(self.viewport_rows.max(1));self.scroll=self.scroll.min(max);
    }
    pub fn set_viewport(&mut self,width_px:usize,visible_rows:usize){
        let chars=width_px.saturating_sub(28)/MENU_GLYPH_ADVANCE_PX;
        let rows=visible_rows.max(1);
        if chars!=self.viewport_chars||rows!=self.viewport_rows {self.viewport_chars=chars.max(16);self.viewport_rows=rows;self.rebuild_lines();}
    }
    fn set_view(&mut self,view:MenuView,content:Vec<String>){self.view=view;self.view_content=content;self.scroll=0;self.input.clear();self.error.clear();self.message.clear();self.rebuild_lines();}
    pub fn visible_lines(&self)->&[MenuLine]{let rows=self.viewport_rows.max(1);let start=self.scroll.min(self.lines.len());let end=(start+rows).min(self.lines.len());&self.lines[start..end]}
    pub fn scroll_indicator(&self)->String{let rows=self.viewport_rows.max(1);if self.lines.len()<=rows{"ALL VISIBLE".into()}else{let start=self.scroll.min(self.lines.len()-1)+1;let end=(self.scroll+rows).min(self.lines.len());if end<self.lines.len(){format!("{start}-{end}/{} · MORE BELOW",self.lines.len())}else{format!("{start}-{end}/{} · END",self.lines.len())}}}
    fn scroll_by(&mut self,delta:isize){let max=self.lines.len().saturating_sub(self.viewport_rows.max(1));self.scroll=(self.scroll as isize+delta).clamp(0,max as isize) as usize;}
    pub fn capture_modern(&mut self,key:KeyEvent)->Option<Captured>{
        if key.code==KeyCode::Esc && key.kind==KeyEventKind::Release {self.release_escape();return None}
        if key.kind==KeyEventKind::Release{return None}if key.code==KeyCode::Char('c')&&key.modifiers.contains(KeyModifiers::CONTROL){return Some(Captured::Quit(QuitKind::CtrlC))}
        if !self.open {if key.code==KeyCode::Esc && key.kind!=KeyEventKind::Repeat && self.esc_ready{self.open_modern();}return None}
        if key.code==KeyCode::Esc{self.close();self.esc_ready=false;return None}if self.pending{return None}
        match key.code {
            KeyCode::Up=>self.scroll_by(-1),KeyCode::Down=>self.scroll_by(1),KeyCode::PageUp=>self.scroll_by(-(self.viewport_rows.saturating_sub(2).max(1) as isize)),KeyCode::PageDown=>self.scroll_by(self.viewport_rows.saturating_sub(2).max(1) as isize),
            KeyCode::Backspace=>{self.input.pop();self.error.clear();},
            KeyCode::Enter=>match parse_modern(&self.input){Ok(Command::Quit)=>return Some(Captured::Quit(QuitKind::Command)),Ok(_)=>{self.pending=true;return Some(Captured::Submit(self.input.clone()))},Err(e)=>self.error=e},
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL)=>{if ch==':'&&self.input.is_empty(){}else if !ch.is_control()&&self.input.len()+ch.len_utf8()<=MAX_COMMAND{self.input.push(ch);self.error.clear();}},
            _=>{}
        } None
    }
    pub fn capture_reply(&mut self,result:Result<CommandReply,String>){self.pending=false;match result{Ok(CommandReply::Applied)=>self.close(),Ok(CommandReply::Info(message))=>{self.input.clear();self.error.clear();self.message=message;},Ok(CommandReply::View(view,content))=>self.set_view(view,content),Err(error)=>{self.error=error;self.message.clear();}}}
    /// Locked legacy TTY capture for --ascii.
    pub fn capture(&mut self,key:KeyEvent)->Option<Captured>{
        if key.kind==KeyEventKind::Release{return None}if key.code==KeyCode::Char('c')&&key.modifiers.contains(KeyModifiers::CONTROL){return Some(Captured::Quit(QuitKind::CtrlC))}if !self.open{if key.code==KeyCode::Esc{self.open_legacy();}return None}if self.pending{if key.code==KeyCode::Esc{self.close();}return None}if !self.editing{if key.code==KeyCode::Char(':')&&!key.modifiers.contains(KeyModifiers::CONTROL){self.editing=true;self.error.clear();}else{self.close();}return None}
        match key.code{KeyCode::Esc=>self.close(),KeyCode::Backspace=>{self.input.pop();self.error.clear();},KeyCode::Enter=>match parse(&self.input){Ok(Command::Quit)=>return Some(Captured::Quit(QuitKind::Command)),Ok(_)=>{self.pending=true;return Some(Captured::Submit(self.input.clone()))},Err(e)=>self.error=e},KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL)=>{if ch.is_ascii()&&!ch.is_ascii_control()&&self.input.len()<MAX_COMMAND{self.input.push(ch);self.error.clear();}},_=>{}}None
    }
    /// Legacy in-thread handler retained for locked non-modern compatibility paths.
    pub fn handle(&mut self,key:KeyEvent,sky:&mut SkyState)->bool {
        if key.code==KeyCode::Char('c')&&key.modifiers.contains(KeyModifiers::CONTROL){return false}if key.kind==KeyEventKind::Release{return true}if !self.open{if key.code==KeyCode::Esc{self.open_legacy();}return true}if self.pending{return true}if !self.editing{if key.code==KeyCode::Char(':')&&!key.modifiers.contains(KeyModifiers::CONTROL){self.editing=true;self.error.clear();sky.set_command_pause(true);}else{self.close();sky.set_command_pause(false);}return true}
        match key.code{KeyCode::Esc=>{self.close();sky.set_command_pause(false)},KeyCode::Backspace=>{self.input.pop();self.error.clear();},KeyCode::Enter=>match parse(&self.input){Ok(Command::Quit)=>return false,Ok(command)=>match sky.command(command){Ok(())=>{self.close();sky.set_command_pause(false)},Err(e)=>self.error=e},Err(e)=>self.error=e},KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL)=>{if ch.is_ascii()&&!ch.is_ascii_control()&&self.input.len()<MAX_COMMAND{self.input.push(ch);self.error.clear();}},_=>{}}true
    }
}

// Locked ASCII help. Modern pixel/X11 help is structured above.
pub const COMMAND_LINES:&[&str]=&[
    "SPACE TIME MACHINE","Press Esc, then Shift+:, type a command, and press Enter.",":FF                  PRESET FAST FORWARD",":BB                  PRESET FAST REVERSE",":P                   PAUSE",":N                   NOW / CURRENT DATE + TIME / 1X FORWARD",":R                   REVERSE / 1X",":#                   SET ABSOLUTE FORWARD SPEED TO #X",":R#                  SET ABSOLUTE REVERSE SPEED TO #X",":stars # / reset     STAR BRIGHTNESS, 1X MATH / 3X DEFAULT / 30X MAX",":HH:MM[:SS]          GO TO TIME ON CURRENT UTC DATE",":YYYY-MM-DD HH:MM:SS GO TO DATE + TIME (SECONDS OPTIONAL)",":XX.XXX,YY.YYY       SET LATITUDE, LONGITUDE",":culture ID          SWITCH CONSTELLATION CULTURE",":culture off         HIDE CONSTELLATION LINES AND NAMES",":culture             SHOW ACTIVE CULTURE",":cultures            LIST AVAILABLE CULTURES",":planets NAME        GO TO PLANET / MOVE OBSERVER",":list planets        LIST PLANET TARGETS",":debugstars on/off   SHOW OR HIDE DIAGNOSTIC STARS",":debugstars          SHOW DIAGNOSTIC-STAR STATUS",":q!                  IYKYK","ANY KEY EXCEPT SHIFT+: CLOSES THIS MENU",
];

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn command_menu_documents_culture_and_diagnostic_forms(){let help=help_lines().into_iter().map(|line|line.text).collect::<Vec<_>>().join("\n");for token in [":culture off",":cultures",":debugstars on / off",":planets graphic / point"]{assert!(help.contains(token),"missing {token}");}}
    #[test]fn legacy_ascii_grammar_stays_legacy(){assert_eq!(parse("3").unwrap(),Command::Rate(3.0));assert_eq!(parse("R").unwrap(),Command::NormalBackward);assert_eq!(parse("debugstars").unwrap(),Command::DebugStarsQuery);}
    #[test]fn modern_numeric_grammar_is_deterministic(){assert_eq!(parse_modern("2026").unwrap(),Command::TimeOfDay(20*3600+26*60));assert_eq!(parse_modern("20261024").unwrap(),Command::Date(1792800000));assert!(parse_modern("3").is_err());assert!(parse_modern("2400").is_err());assert_eq!(parse_modern("3x").unwrap(),Command::Rate(3.0));assert_eq!(parse_modern("-3x").unwrap(),Command::Rate(-3.0));assert_eq!(parse_modern("0x").unwrap(),Command::Rate(0.0));}
    #[test]fn modern_date_forms_and_leap_validation(){assert_eq!(parse_modern("2026-12-25 18:30:45").unwrap(),Command::Date(1798223445));assert_eq!(parse_modern("20261225183045").unwrap(),Command::Date(1798223445));assert_eq!(parse_modern("2026/12/25 1830").unwrap(),Command::Date(1798223400));assert!(parse_modern("2026-02-29").is_err());assert!(parse_modern("2024-02-30").is_err());}
    #[test]fn modern_chickens_toggle_is_one_global_background_fauna_switch(){assert_eq!(parse_modern("chickens on").unwrap(),Command::Chickens(true));assert_eq!(parse_modern(":chickens off").unwrap(),Command::Chickens(false));assert!(parse_modern("chickens").is_err());}
    #[test]fn modern_routes_and_placeholders(){assert_eq!(parse_modern("planets").unwrap(),Command::ListPlanets);assert_eq!(parse_modern("planets venus").unwrap(),Command::Planet("venus".into()));assert_eq!(parse_modern("cultures").unwrap(),Command::Cultures);assert_eq!(parse_modern("VEGA").unwrap(),Command::CelestialPlaceholder);assert_eq!(parse_modern("HIP91262").unwrap(),Command::CelestialPlaceholder);assert_eq!(parse_modern("RD279.233,38.7836").unwrap(),Command::CelestialPlaceholder);assert_eq!(parse_modern("18h36m56s,+38d47'").unwrap(),Command::CelestialPlaceholder);}
    #[test]fn registered_name_aliases_are_not_stolen_by_sexagesimal_recognition(){
        let cultures=["anutan","aztec","belarusian","boorong","chinese","chinese_chenzhuo","chinese_manchu","chinese_song_dynasty","chinese_xianglin","chinese_yuan_dynasty","egyptian","hawaiian_starlines","indian","indian_nakshatras","inuit","japanese","korean","macedonian","maori","modern","modern_chinese","modern_hlad","modern_iau","modern_rey","modern_st","mongolian","navajo","norse","norse_edda","northern_andes","romanian","ruanui_sky_tahiti_and_society_islands","russian_siberian","sami","samoan","sardinian","seri","tikuna","tongan","tukano","tupi","vanuatu_netwar"];
        for name in cultures {assert_eq!(parse_modern(name).unwrap(),Command::Named(name.into()),"alias {name}");}
        assert_eq!(parse_modern("earth").unwrap(),Command::Named("earth".into()));
        assert_eq!(parse_modern("18h36m56s,+38°47'").unwrap(),Command::CelestialPlaceholder);
    }
    #[test]fn compact_viewport_reflows_and_uses_viewport_row_count(){let mut p=Panel::modern();p.set_viewport(420,12);p.open_modern();assert!(p.lines.len()>12);assert_eq!(p.visible_lines().len(),12);assert!(p.lines.iter().all(|line|line.text.chars().count()<=65));p.set_view(MenuView::Help,vec![]);assert_eq!(p.visible_lines().len(),12);assert!(p.lines.iter().all(|line|line.text.chars().count()<=65));}
    #[test]fn modern_panel_persists_view_and_input(){let key=|code|KeyEvent::new(code,KeyModifiers::NONE);let mut p=Panel::modern();p.capture_modern(key(KeyCode::Esc));assert!(p.open&&p.editing);for ch in "help".chars(){p.capture_modern(key(KeyCode::Char(ch)));}assert_eq!(p.capture_modern(key(KeyCode::Enter)),Some(Captured::Submit("help".into())));p.capture_reply(Ok(CommandReply::View(MenuView::Help,vec![])));assert_eq!(p.view,MenuView::Help);assert!(p.input.is_empty());p.scroll=3;p.capture_modern(key(KeyCode::Char('1')));p.capture_modern(key(KeyCode::Char('8')));p.capture_modern(key(KeyCode::Char(':')));assert_eq!(p.scroll,3);assert_eq!(p.input,"18:");}
    #[test]fn modern_escape_requires_release_before_reopen(){let mut p=Panel::modern();let press=KeyEvent::new(KeyCode::Esc,KeyModifiers::NONE);p.capture_modern(press);assert!(p.open);p.capture_modern(press);assert!(!p.open);p.capture_modern(press);assert!(!p.open);let mut release=press;release.kind=KeyEventKind::Release;p.capture_modern(release);p.capture_modern(press);assert!(p.open);}
    #[test]fn modern_error_preserves_input_and_scroll(){let key=|code|KeyEvent::new(code,KeyModifiers::NONE);let mut p=Panel::modern();p.open_modern();p.set_view(MenuView::Help,vec![]);p.scroll=4;p.input="bogus".into();p.capture_reply(Err("nope".into()));assert_eq!(p.scroll,4);assert_eq!(p.input,"bogus");assert_eq!(p.error,"nope");p.capture_modern(key(KeyCode::Esc));assert!(!p.open);}
}
