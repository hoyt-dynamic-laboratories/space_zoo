//! Space Zoo v0.50.04: headless Tycho-2 ICRS catalogue reader.
//! This is an independent data interface; no renderer, observer ENU or 17-star fallback.
//! Binary schema is documented in docs/TYCHO2_0504_FORMAT.md.
use std::collections::{HashMap, HashSet, VecDeque};
use std::f64::consts::PI;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const MAGIC: &[u8; 8] = b"SZT2CAT\0";
const HEADER_LEN: usize = 128;
const TILE_LEN: usize = 40;
const RECORD_LEN: usize = 160;
const NR: usize = 72;
const ND: usize = 36;
const TILES: usize = NR * ND;
const START: usize = HEADER_LEN + TILES * TILE_LEN;
const MAS_RAD: f64 = PI / (180.0 * 3_600_000.0);
const TOL_DEG: f64 = 2e-10;

const MEAN: u16 = 1;
const PM: u16 = 2;
const BT: u16 = 4;
const VT: u16 = 8;
const EBT: u16 = 16;
const EVT: u16 = 32;
const OBS: u16 = 64;
const STATIONARY: u16 = 128;
const ELIGIBLE: u16 = 256;
const SHARED: u16 = 512;
const TYCHO1: u16 = 1024;
const ALLOWED: u16 = MEAN | PM | BT | VT | EBT | EVT | OBS | STATIONARY | ELIGIBLE | SHARED | TYCHO1;

#[derive(Debug)]
pub enum CatalogueError {
    Io(std::io::Error),
    Corrupt(&'static str),
    BadQuery(&'static str),
    Capacity { needed_at_least: usize, limit: usize },
    TileTooLarge { bytes: usize, budget: usize },
}
impl From<std::io::Error> for CatalogueError {
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
}
impl std::fmt::Display for CatalogueError {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
        match self {
            Self::Io(e)=>write!(f,"{e}"),
            Self::Corrupt(msg)=>write!(f,"corrupt catalogue: {msg}"),
            Self::BadQuery(msg)=>write!(f,"bad catalogue query: {msg}"),
            Self::Capacity{needed_at_least,limit}=>write!(f,"catalogue query needs at least {needed_at_least} results; limit is {limit}"),
            Self::TileTooLarge{bytes,budget}=>write!(f,"catalogue tile needs {bytes} bytes; cache budget is {budget}"),
        }
    }
}
impl std::error::Error for CatalogueError {}
type Result<T> = std::result::Result<T, CatalogueError>;
fn fail<T>(msg: &'static str) -> Result<T> { Err(CatalogueError::Corrupt(msg)) }
fn require(condition: bool, msg: &'static str) -> Result<()> {
    if condition { Ok(()) } else { fail(msg) }
}
fn u16le(b: &[u8], o: usize) -> u16 { u16::from_le_bytes([b[o],b[o+1]]) }
fn u32le(b: &[u8], o: usize) -> u32 { u32::from_le_bytes([b[o],b[o+1],b[o+2],b[o+3]]) }
fn u64le(b: &[u8], o: usize) -> u64 { u64::from_le_bytes(b[o..o+8].try_into().expect("fixed-length slice")) }
fn f64le(b: &[u8], o: usize) -> f64 { f64::from_bits(u64le(b,o)) }
fn f32le(b: &[u8], o: usize) -> f32 { f32::from_bits(u32le(b,o)) }
fn crc32(b: &[u8]) -> u32 { crc32_update(0,b) }
fn crc32_update(mut state: u32, b: &[u8]) -> u32 {
    state = !state;
    for &v in b {
        state ^= u32::from(v);
        for _ in 0..8 { state = (state >> 1) ^ (0xedb88320_u32 & (0_u32.wrapping_sub(state & 1))); }
    }
    !state
}

/// Stable, compact *complete* published TYC identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TychoId { pub region: u16, pub number: u16, pub component: u8 }
impl std::fmt::Display for TychoId {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
        write!(f,"TYC {}-{}-{}",self.region,self.number,self.component)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band { Bt, Vt }
#[derive(Clone, Copy, Debug)]
pub struct MagnitudeFilter {
    pub band: Band,
    pub max_magnitude: f64,
    pub include_missing: bool,
}
/// ICRS catalogue directions at Julian year in TT, 2000.0 through 2050.0 inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogueFrame { Icrs }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpochScale { JulianYearTt }
#[derive(Clone, Copy, Debug)]
pub struct ConeQuery {
    pub frame: CatalogueFrame, pub epoch_scale: EpochScale,
    pub ra_deg: f64, pub dec_deg: f64, pub radius_deg: f64, pub year_tt: f64,
    pub magnitude: Option<MagnitudeFilter>,
    /// Hard bound on returned stable IDs. No results are silently dropped.
    pub max_results: usize,
}
#[derive(Clone, Debug)]
pub struct QueryResult { pub ids: Vec<TychoId>, pub candidate_count: u64 }

#[derive(Clone, Debug)]
pub struct TychoStar {
    pub id: TychoId,
    /// MEAN flag distinguishes published J2000 mean coordinates from absent ones.
    pub mean_ra_deg: Option<f64>, pub mean_dec_deg: Option<f64>,
    pub observed_ra_deg: f64, pub observed_dec_deg: f64,
    pub observed_epoch_ra: f64, pub observed_epoch_dec: f64,
    /// µα* = dα/dt cosδ, µδ; both mas/Julian year.
    pub pm_ra_star: Option<f64>, pub pm_dec: Option<f64>,
    pub bt: Option<f32>, pub vt: Option<f32>,
    pub bt_error: Option<f32>, pub vt_error: Option<f32>,
    pub proximity_tenth_arcsec: u16,
    /// Published source error, mean-epoch and fit-quality values in fixed source order.
    /// None means missing in the published catalogue, not zero.
    pub source_quality: [Option<f32>;12],
    pub source_position_count: Option<u16>,
    pub hip: Option<u32>,
    pub ccdm: [u8;3],
    pub observed_correlation: Option<f32>,
    pub pflag: u8, pub posflag: u8, pub tycho1_flag: u8, pub quality: u16,
}
impl TychoStar {
    fn decode(b: &[u8]) -> Result<Self> {
        require(b.len()==RECORD_LEN,"bad record size")?;
        let id=TychoId{region:u16le(b,0),number:u16le(b,2),component:b[4]};
        let quality=u16le(b,90);
        require((1..=9537).contains(&id.region) && (1..=12121).contains(&id.number) && (1..=3).contains(&id.component),"invalid TYC identity")?;
        require([32,80,88].contains(&b[5]) && [32,68,80].contains(&b[6]) && [32,84].contains(&b[7]),"invalid source flags")?;
        require(quality & !ALLOWED==0,"unsupported record flags")?;
        require((quality&MEAN!=0)==(quality&PM!=0) && (quality&MEAN!=0)==(b[5]!=88),"inconsistent mean/motion flags")?;
        require((quality&STATIONARY!=0)==(quality&PM==0),"inconsistent stationary policy")?;
        require((quality&SHARED!=0)==(b[5]==80),"inconsistent photocentre flag")?;
        require((quality&TYCHO1!=0)==(b[7]==84),"inconsistent Tycho1 flag")?;
        require(quality&OBS!=0 && quality&ELIGIBLE!=0,"unsupported ineligible record")?;
        let fs: Vec<f64>=(0..8).map(|i|f64le(b,8+i*8)).collect();
        let [mr,md,ra,de,er,ed,pmra,pmde]: [f64;8]=fs.try_into().map_err(|_|CatalogueError::Corrupt("field count"))?;
        let [bt,vt,ebt,evt]=[f32le(b,72),f32le(b,76),f32le(b,80),f32le(b,84)];
        require([mr,md,ra,de,er,ed,pmra,pmde].iter().all(|v|v.is_finite()) && [bt,vt,ebt,evt].iter().all(|v|v.is_finite()),"nonfinite record field")?;
        require((0.0..=360.0).contains(&ra) && (-90.0..=90.0).contains(&de) && (1990.0..=2000.0).contains(&er) && (1990.0..=2000.0).contains(&ed),"bad observed astrometry")?;
        if quality&MEAN!=0 {
            require((0.0..=360.0).contains(&mr) && (-90.0..=90.0).contains(&md),"bad mean astrometry")?;
        } else { require(mr==0.0 && md==0.0 && pmra==0.0 && pmde==0.0,"missing data not zeroed")?; }
        require((u16le(b,88)==0 || (3..=999).contains(&u16le(b,88))),"invalid proximity")?;
        require((quality&BT!=0 || bt==0.0) && (quality&VT!=0 || vt==0.0),"invalid missing photometry")?;
        require((quality&EBT!=0 || ebt==0.0) && (quality&EVT!=0 || evt==0.0),"invalid missing photometric error")?;
        require((quality&EBT==0 || quality&BT!=0) && (quality&EVT==0 || quality&VT!=0),"photometric uncertainty without measurement")?;
        require(b[156..160].iter().all(|v| *v==0),"nonzero reserved record bytes")?;
        let observed_corr=f32le(b,151);let corr_present=b[155];
        require((corr_present==0 || corr_present==1) && observed_corr.is_finite() && (-1.0..=1.0).contains(&observed_corr),"invalid observed correlation")?;
        require(corr_present!=0 || observed_corr==0.0,"nonzero absent observed correlation")?;
        let extra_mask=u16le(b,149);
        require(extra_mask&!0x3fff==0,"unsupported quality presence mask")?;
        let mut source_quality=[None;12];
        for (j, slot) in source_quality.iter_mut().enumerate() {
            let value=f32le(b,92+j*4);
            require(value.is_finite(),"nonfinite quality source field")?;
            if extra_mask&(1<<j)!=0{*slot=Some(value);}else{require(value==0.0,"invalid absent quality field")?;}
        }
        let num=u16le(b,140);let hip=u32le(b,142);
        require((extra_mask&(1<<12)!=0 && (2..=36).contains(&num)) || (extra_mask&(1<<12)==0 && num==0),"bad source Num")?;
        require((extra_mask&(1<<13)!=0 && (1..=120404).contains(&hip)) || (extra_mask&(1<<13)==0 && hip==0),"bad HIP reference")?;
        let ccdm=[b[146],b[147],b[148]];
        require(ccdm.iter().all(|v|*v<128),"invalid CCDM component")?;
        let maybe=|value:f32,flag:u16| if quality&flag!=0 {Some(value)} else {None};
        Ok(Self{id,mean_ra_deg:if quality&MEAN!=0{Some(mr)}else{None},mean_dec_deg:if quality&MEAN!=0{Some(md)}else{None},
            observed_ra_deg:ra,observed_dec_deg:de,observed_epoch_ra:er,observed_epoch_dec:ed,
            pm_ra_star:if quality&PM!=0{Some(pmra)}else{None},pm_dec:if quality&PM!=0{Some(pmde)}else{None},
            bt:maybe(bt,BT),vt:maybe(vt,VT),bt_error:maybe(ebt,EBT),vt_error:maybe(evt,EVT),
            proximity_tenth_arcsec:u16le(b,88),source_quality,
            source_position_count:if extra_mask&(1<<12)!=0{Some(num)}else{None},
            hip:if extra_mask&(1<<13)!=0{Some(hip)}else{None},ccdm,
            observed_correlation:if corr_present==1{Some(observed_corr)}else{None},
            pflag:b[5],posflag:b[6],tycho1_flag:b[7],quality})
    }
    pub fn direction_icrs(&self,year_tt:f64) -> (f64,f64) {
        if let (Some(ra),Some(de),Some(pmra),Some(pmde))=(self.mean_ra_deg,self.mean_dec_deg,self.pm_ra_star,self.pm_dec) {
            let (a,d)=(ra.to_radians(),de.to_radians());let (s,c)=a.sin_cos();let (sd,cd)=d.sin_cos();
            let (ma,md)=(pmra*MAS_RAD,pmde*MAS_RAD);
            let dt=year_tt-2000.0;
            let x=cd*c+(-s*ma-sd*c*md)*dt;
            let y=cd*s+(c*ma-sd*s*md)*dt;
            let z=sd+(cd*md)*dt;
            (y.atan2(x).to_degrees().rem_euclid(360.0),z.atan2(x.hypot(y)).to_degrees())
        } else { (self.observed_ra_deg.rem_euclid(360.0),self.observed_dec_deg) }
    }
    /// Cartesian ICRS unit direction using the catalogue's accepted proper-motion policy.
    pub fn direction_icrs_vector(&self,year_tt:f64) -> super::Vec3 {
        let (ra,dec)=self.direction_icrs(year_tt);
        let (sr,cr)=ra.to_radians().sin_cos();
        let (sd,cd)=dec.to_radians().sin_cos();
        super::Vec3::new(cd*cr,cd*sr,sd)
    }
    fn motion_bound_deg(&self)->f64 {
        self.pm_ra_star.unwrap_or(0.0).hypot(self.pm_dec.unwrap_or(0.0))*MAS_RAD*50.0*180.0/PI+0.000001
    }
}
#[derive(Clone,Copy)]
struct TileEntry { offset:u64, count:u32, cap_deg:f64, max_motion_deg:f64, checksum:u32 }
#[derive(Default)]
struct PageCache {
    pages:HashMap<usize,Vec<TychoStar>>,
    queue:VecDeque<usize>, bytes:usize, peak_bytes:usize,
}
/// Opened catalogue; no global cache or leaked reference IDs.
pub struct ProductionCatalogue {
    file:File, tiles:Vec<TileEntry>, cache:PageCache, cache_budget:usize,
    pub record_count:u64, pub stationary_count:u64,
    pub source_identity:[u8;32], data_crc:u32,
}
impl ProductionCatalogue {
    pub fn open(path:impl AsRef<Path>,cache_budget:usize)->Result<Self>{
        require(cache_budget>=1024,"cache budget too small")?;
        let mut file=File::open(path)?;
        let file_len=file.metadata()?.len();
        require(file_len>=START as u64,"asset truncated")?;
        let mut h=[0_u8;HEADER_LEN];file.read_exact(&mut h)?;
        require(&h[0..8]==MAGIC && u32le(&h,8)==1,"invalid magic or schema version")?;
        require(h[120..128].iter().all(|v|*v==0),"nonzero header reserved fields")?;
        let mut hc=h;hc[116..120].fill(0);
        require(crc32(&hc)==u32le(&h,116),"header checksum invalid")?;
        let nt=u32le(&h,12);
        let total=u64le(&h,16);let eligible=u64le(&h,24);let assumed=u64le(&h,32);let ineligible=u64le(&h,40);
        require(nt==TILES as u32 && u32le(&h,104)==RECORD_LEN as u32 && u32le(&h,108)==NR as u32 && u32le(&h,112)==ND as u32,"unsupported geometry")?;
        require(u64le(&h,48)==HEADER_LEN as u64 && u64le(&h,56)==START as u64,"inconsistent section offsets")?;
        require(total<=2_539_961 && eligible.checked_add(ineligible)==Some(total) && assumed<=eligible,"invalid header accounting")?;
        let bytes=total.checked_mul(RECORD_LEN as u64).and_then(|b|b.checked_add(START as u64)).ok_or(CatalogueError::Corrupt("record length overflow"))?;
        require(bytes==file_len,"record length does not match file")?;
        let mut table=vec![0u8;TILES*TILE_LEN];file.read_exact(&mut table)?;
        require(crc32(&table)==u32le(&h,96),"directory checksum mismatch")?;
        let mut tiles=Vec::with_capacity(TILES);
        let mut offset=START as u64;let mut sum=0_u64;
        for idx in 0..TILES {
            let b=&table[idx*TILE_LEN..(idx+1)*TILE_LEN];
            let n=u32le(b,8);let cap=f64le(b,12);let motion=f64le(b,20);
            require(b[32..40].iter().all(|v|*v==0),"nonzero tile reserved fields")?;
            require(u64le(b,0)==offset && n as u64<=total,"noncontiguous tile data")?;
            require(cap.is_finite() && motion.is_finite() && (5.0..=180.0).contains(&cap) && (0.0..=175.0).contains(&motion) && (cap-(5.0+motion)).abs()<1e-10,"invalid motion cap")?;
            offset=offset.checked_add((n as u64)*RECORD_LEN as u64).ok_or(CatalogueError::Corrupt("tile length overflow"))?;
            sum=sum.checked_add(n as u64).ok_or(CatalogueError::Corrupt("tile count overflow"))?;
            tiles.push(TileEntry{offset:u64le(b,0),count:n,cap_deg:cap,max_motion_deg:motion,checksum:u32le(b,28)});
        }
        require(sum==total && offset==file_len,"incorrect catalogue counts")?;
        let mut source_identity=[0u8;32];source_identity.copy_from_slice(&h[64..96]);
        Ok(Self{file,tiles,cache:PageCache::default(),cache_budget,record_count:total,stationary_count:assumed,source_identity,data_crc:u32le(&h,100)})
    }
    /// Source-tile ownership at year 2000, declination bins [-90,90], RA wraps at 360.
    fn index(ra:f64,de:f64)->usize {
        let a=((ra.rem_euclid(360.0)/5.0) as usize).min(NR-1);
        let d=(((de+90.0)/5.0).max(0.0) as usize).min(ND-1);
        d*NR+a
    }
    fn tile_center(idx:usize)->(f64,f64) { (((idx%NR) as f64)*5.0+2.5, ((idx/NR) as f64)*5.0-87.5) }
    fn sep(a:f64,d:f64,b:f64,e:f64)->f64 {
        let (a,d,b,e)=(a.to_radians(),d.to_radians(),b.to_radians(),e.to_radians());
        let h=((d-e)/2.0).sin().powi(2)+d.cos()*e.cos()*((a-b)/2.0).sin().powi(2);
        (2.0*h.clamp(0.0,1.0).sqrt().atan2((1.0-h).max(0.0).sqrt())).to_degrees()
    }
    fn load_tile(&mut self,idx:usize)->Result<&[TychoStar]> {
        if self.cache.pages.contains_key(&idx) {
            self.cache.queue.retain(|&v|v!=idx);self.cache.queue.push_back(idx);
        } else {
            let entry=self.tiles[idx];
            let byte_count=(entry.count as usize).checked_mul(RECORD_LEN).ok_or(CatalogueError::Corrupt("tile allocation overflow"))?;
            let decoded_count=(entry.count as usize).checked_mul(std::mem::size_of::<TychoStar>()).ok_or(CatalogueError::Corrupt("decoded allocation overflow"))?;
            let needed=byte_count.checked_add(decoded_count).and_then(|v|v.checked_add(256)).ok_or(CatalogueError::Corrupt("cache accounting overflow"))?;
            if needed>self.cache_budget {return Err(CatalogueError::TileTooLarge{bytes:needed,budget:self.cache_budget});}
            let mut raw=vec![0u8;byte_count];
            self.file.seek(SeekFrom::Start(entry.offset))?;self.file.read_exact(&mut raw)?;
            require(crc32(&raw)==entry.checksum,"tile checksum mismatch")?;
            let mut stars=Vec::with_capacity(entry.count as usize);
            let mut prev=None;
            for item in raw.chunks_exact(RECORD_LEN) {
                let star=TychoStar::decode(item)?;
                require(prev.map_or(true,|old|old<star.id),"tile ordering / duplicate IDs")?;
                let (ra,de)=star.direction_icrs(2000.0);
                require(Self::index(ra,de)==idx,"misindexed star")?;
                require(star.motion_bound_deg()<=entry.max_motion_deg+1e-10,"tile motion bound too small")?;
                prev=Some(star.id);stars.push(star);
            }
            while self.cache.bytes+needed>self.cache_budget {
                let old=self.cache.queue.pop_front().ok_or(CatalogueError::Corrupt("cache state inconsistent"))?;
                if let Some(page)=self.cache.pages.remove(&old) {
                    self.cache.bytes=self.cache.bytes.saturating_sub(Self::page_charge(page.len()));
                }
            }
            self.cache.bytes+=needed;self.cache.peak_bytes=self.cache.peak_bytes.max(self.cache.bytes);
            self.cache.pages.insert(idx,stars);self.cache.queue.push_back(idx);
        }
        Ok(self.cache.pages.get(&idx).ok_or(CatalogueError::Corrupt("tile not cached"))?.as_slice())
    }
    fn page_charge(count:usize)->usize { count*RECORD_LEN+count*std::mem::size_of::<TychoStar>()+256 }
    pub fn cache_peak_bytes(&self)->usize { self.cache.peak_bytes }
    /// Exact catalogue-direction ICRS cone query using a conservative swept-tile cap.
    pub fn cone(&mut self,q:&ConeQuery)->Result<QueryResult>{
        if ![q.ra_deg,q.dec_deg,q.radius_deg,q.year_tt].iter().all(|v|v.is_finite()) {
            return Err(CatalogueError::BadQuery("nonfinite input"));
        }
        require(q.frame==CatalogueFrame::Icrs && q.epoch_scale==EpochScale::JulianYearTt,"unsupported reference frame").map_err(|_|CatalogueError::BadQuery("unsupported reference frame/time scale"))?;
        if !(0.0..360.0).contains(&q.ra_deg) || !(-90.0..=90.0).contains(&q.dec_deg) || !(0.0..=180.0).contains(&q.radius_deg) || !(2000.0..=2050.0).contains(&q.year_tt) {
            return Err(CatalogueError::BadQuery("invalid ICRS cone or epoch"));
        }
        if let Some(mag)=q.magnitude { if !mag.max_magnitude.is_finite(){return Err(CatalogueError::BadQuery("invalid magnitude"));} }
        let mut ids=Vec::new();let mut candidates=0u64;
        for idx in 0..TILES {
            let t=self.tiles[idx];if t.count==0 {continue;}
            let (a,d)=Self::tile_center(idx);
            if Self::sep(q.ra_deg,q.dec_deg,a,d)>(q.radius_deg+t.cap_deg).min(180.0)+TOL_DEG {continue;}
            for star in self.load_tile(idx)? {
                candidates+=1;
                let (ra,de)=star.direction_icrs(q.year_tt);
                if Self::sep(q.ra_deg,q.dec_deg,ra,de)>q.radius_deg+TOL_DEG {continue;}
                if let Some(m)=q.magnitude {
                    let chosen=match m.band {Band::Bt=>star.bt,Band::Vt=>star.vt};
                    if !chosen.map_or(m.include_missing,|v|(v as f64)<=m.max_magnitude){continue;}
                }
                if ids.len()>=q.max_results {return Err(CatalogueError::Capacity{needed_at_least:ids.len()+1,limit:q.max_results});}
                ids.push(star.id);
            }
        }
        ids.sort_unstable();
        require(ids.windows(2).all(|w|w[0]!=w[1]),"duplicate query identities")?;
        Ok(QueryResult{ids,candidate_count:candidates})
    }
    /// Same validated selection rules as `cone`, but returns decoded records for
    /// the v0.51 renderer bridge. The accepted `cone` implementation above is
    /// deliberately left unchanged. Results are bounded and sorted by stable TYC ID.
    pub fn records(&mut self,q:&ConeQuery)->Result<Vec<TychoStar>>{
        if ![q.ra_deg,q.dec_deg,q.radius_deg,q.year_tt].iter().all(|v|v.is_finite()) {
            return Err(CatalogueError::BadQuery("nonfinite input"));
        }
        require(q.frame==CatalogueFrame::Icrs && q.epoch_scale==EpochScale::JulianYearTt,"unsupported reference frame").map_err(|_|CatalogueError::BadQuery("unsupported reference frame/time scale"))?;
        if !(0.0..360.0).contains(&q.ra_deg) || !(-90.0..=90.0).contains(&q.dec_deg) || !(0.0..=180.0).contains(&q.radius_deg) || !(2000.0..=2050.0).contains(&q.year_tt) {
            return Err(CatalogueError::BadQuery("invalid ICRS cone or epoch"));
        }
        if let Some(mag)=q.magnitude { if !mag.max_magnitude.is_finite(){return Err(CatalogueError::BadQuery("invalid magnitude"));} }
        let mut stars=Vec::new();
        for idx in 0..TILES {
            let t=self.tiles[idx];if t.count==0 {continue;}
            let (a,d)=Self::tile_center(idx);
            if Self::sep(q.ra_deg,q.dec_deg,a,d)>(q.radius_deg+t.cap_deg).min(180.0)+TOL_DEG {continue;}
            for star in self.load_tile(idx)? {
                let (ra,de)=star.direction_icrs(q.year_tt);
                if Self::sep(q.ra_deg,q.dec_deg,ra,de)>q.radius_deg+TOL_DEG {continue;}
                if let Some(m)=q.magnitude {
                    let chosen=match m.band {Band::Bt=>star.bt,Band::Vt=>star.vt};
                    if !chosen.map_or(m.include_missing,|v|(v as f64)<=m.max_magnitude){continue;}
                }
                if stars.len()>=q.max_results {return Err(CatalogueError::Capacity{needed_at_least:stars.len()+1,limit:q.max_results});}
                stars.push(star.clone());
            }
        }
        stars.sort_unstable_by_key(|s|s.id);
        require(stars.windows(2).all(|w|w[0].id!=w[1].id),"duplicate query identities")?;
        Ok(stars)
    }
    /// Stream every accepted catalogue record in deterministic tile/file order.
    ///
    /// This is intentionally independent of cone/magnitude selection so offline indexes
    /// can reason about records that are present in the accepted catalogue but are not
    /// currently eligible for rendering. The scan validates every decoded record plus
    /// per-tile and whole-record-section CRCs while keeping memory bounded.
    pub fn for_each_record<F>(&mut self, mut visit:F)->Result<()>
    where F: FnMut(&TychoStar) {
        let mut crc=0u32;let mut count=0u64;
        for idx in 0..TILES {
            let t=self.tiles[idx];let mut local_crc=0u32;let mut prev=None;
            self.file.seek(SeekFrom::Start(t.offset))?;
            for _ in 0..t.count {
                let mut buf=[0u8;RECORD_LEN];self.file.read_exact(&mut buf)?;
                local_crc=crc32_update(local_crc,&buf);crc=crc32_update(crc,&buf);
                let star=TychoStar::decode(&buf)?;
                require(prev.map_or(true,|old|old<star.id),"unordered tile")?;
                let (ra,de)=star.direction_icrs(2000.0);
                require(Self::index(ra,de)==idx,"misindexed identity")?;
                require(star.motion_bound_deg()<=t.max_motion_deg+1e-10,"insufficient swept bound")?;
                prev=Some(star.id);count+=1;
                visit(&star);
            }
            require(local_crc==t.checksum,"corrupt tile checksum")?;
        }
        require(crc==self.data_crc && count==self.record_count,"asset accounting or data checksum mismatch")?;
        Ok(())
    }

    /// For operator acceptance, deep-scan all source tiles and detect cross-tile duplicate IDs.
    /// This audit intentionally uses ~200 MiB memory for the global identity set;
    /// normal indexed queries do not allocate this set.
    pub fn full_validate(&mut self)->Result<()> {
        let mut seen=HashSet::new();let mut crc=0u32;let mut count=0u64;let mut station=0u64;
        for idx in 0..TILES {
            let t=self.tiles[idx];let mut local_crc=0u32;let mut prev=None;
            self.file.seek(SeekFrom::Start(t.offset))?;
            for _ in 0..t.count {
                let mut buf=[0u8;RECORD_LEN];self.file.read_exact(&mut buf)?;
                local_crc=crc32_update(local_crc,&buf);crc=crc32_update(crc,&buf);
                let star=TychoStar::decode(&buf)?;
                require(prev.map_or(true,|old|old<star.id),"unordered tile")?;
                require(seen.insert(star.id),"duplicate identity across tiles")?;
                let (ra,de)=star.direction_icrs(2000.0);
                require(Self::index(ra,de)==idx,"misindexed identity")?;
                require(star.motion_bound_deg()<=t.max_motion_deg+1e-10,"insufficient swept bound")?;
                if star.quality&STATIONARY!=0 {station+=1;}
                prev=Some(star.id);count+=1;
            }
            require(local_crc==t.checksum,"corrupt tile checksum")?;
        }
        require(crc==self.data_crc && count==self.record_count && station==self.stationary_count,"asset accounting or data checksum mismatch")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn crc_zlib_reference() {assert_eq!(crc32(b"123456789"),0xcbf43926);}
    #[test] fn stable_id_format(){assert_eq!(TychoId{region:1,number:13,component:1}.to_string(),"TYC 1-13-1");}
    #[test] fn coordinate_wrap(){assert_eq!(ProductionCatalogue::index(360.0,0.0),ProductionCatalogue::index(0.0,0.0));}
    #[test] fn no_synthetic_17_star_fallback(){assert!(ProductionCatalogue::open("no-such-tycho2-file",1024).is_err());}
}

