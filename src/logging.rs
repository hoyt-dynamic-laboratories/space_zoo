//! Bounded launch-directory event journal for Space Zoo.
//!
//! The log path is resolved once from the launch working directory. Each event is
//! appended by reopening the current pathname while a separate, stable lock file
//! is flocked, so atomic log replacement never strands another process on an old
//! unlinked log inode. Signal handlers never call this module.
use std::backtrace::Backtrace;
use std::collections::{hash_map::DefaultHasher, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, TryLockError};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const LOG_NAME:&str="space_zoo.log";
const LOCK_NAME:&str=".space_zoo.log.lock";
const MARKER_PREFIX:&str=".space_zoo.session.";
const MARKER_SUFFIX:&str=".marker";
const LOG_MAGIC:&[u8]=b"# SPACE_ZOO_LOG v1\n";
const LOCK_MAGIC:&[u8]=b"SPACE_ZOO_LOG_LOCK_V1\n";
const EVENT_BEGIN:&[u8]=b"----- SPACE_ZOO_EVENT_V1 BEGIN -----\n";
const EVENT_END:&[u8]=b"----- SPACE_ZOO_EVENT_V1 END -----\n";
const MAX_LOG_BYTES:u64=1024*1024;
const RETAIN_LOG_BYTES:usize=768*1024;
const MAX_EVENT_BYTES:usize=16*1024;
const LOCK_WAIT:Duration=Duration::from_millis(25);
const MAX_SUPPRESSED_KEYS:usize=32;
const MAX_KEY_CHARS:usize=128;
const MAX_SNIPPET_CHARS:usize=256;

static LOGGER:OnceLock<RuntimeLogger>=OnceLock::new();
static SESSION_COUNTER:AtomicU64=AtomicU64::new(0);
static CLEANUP_FAILED:AtomicBool=AtomicBool::new(false);
static INIT_WARNING:AtomicBool=AtomicBool::new(false);

#[derive(Clone,Copy)]
pub enum Severity {Info,Warn,Error,Fatal}
impl Severity {fn as_str(self)->&'static str {match self{Self::Info=>"INFO",Self::Warn=>"WARN",Self::Error=>"ERROR",Self::Fatal=>"FATAL"}}}

struct RuntimeLogger {
    launch_dir:PathBuf,
    log_path:PathBuf,
    session_id:String,
    pid:u32,
    process_identity:ProcessIdentity,
    state:Mutex<State>,
    warning_write:AtomicBool,
    warning_lock:AtomicBool,
    warning_trim:AtomicBool,
    warning_marker:AtomicBool,
    startup_logged:AtomicBool,
}

struct State {
    lock_file:File,
    marker:Option<Marker>,
    last_operation:String,
    suppressed:VecDeque<SuppressedError>,
}

struct Marker {path:PathBuf,file:File}

#[derive(Clone)]
struct SuppressedError {
    key:String,
    fingerprint:u64,
    event_type:String,
    message_snippet:String,
    suppressed:u64,
}

#[derive(Clone,Debug)]
struct ProcessIdentity {boot_id:String,start_ticks:u64}

#[derive(Debug)]
struct MarkerInfo {
    session_id:String,
    pid:u32,
    boot_id:String,
    start_ticks:u64,
    stage:String,
}

struct ProcessLock<'a>{file:&'a File}
impl Drop for ProcessLock<'_> {fn drop(&mut self){#[cfg(unix)] unsafe{libc::flock(self.file.as_raw_fd(),libc::LOCK_UN);}}}

fn warn_once(flag:&AtomicBool,message:impl AsRef<str>) {
    if !flag.swap(true,Ordering::Relaxed) {eprintln!("Space Zoo logging: {}",message.as_ref());}
}

fn short_text(s:&str,max:usize)->String {s.chars().take(max).collect()}
fn fingerprint(event_type:&str,message:&str)->u64 {let mut h=DefaultHasher::new();event_type.hash(&mut h);message.hash(&mut h);h.finish()}

#[cfg(unix)]
fn safe_open_existing(path:&Path,read:bool,write:bool,append:bool)->io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() {return Err(io::Error::new(io::ErrorKind::InvalidData,format!("unsafe nonregular target: {}",path.display())));}
    let file=OpenOptions::new().read(read).write(write).append(append)
        .custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC).open(path)?;
    if !file.metadata()?.is_file(){return Err(io::Error::new(io::ErrorKind::InvalidData,"opened target is not regular"));}
    Ok(file)
}
#[cfg(not(unix))]
fn safe_open_existing(path:&Path,read:bool,write:bool,append:bool)->io::Result<File> {
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_file(){return Err(io::Error::new(io::ErrorKind::InvalidData,"unsafe nonregular target"));}
    let file=OpenOptions::new().read(read).write(write).append(append).open(path)?;
    if !file.metadata()?.is_file(){return Err(io::Error::new(io::ErrorKind::InvalidData,"opened target is not regular"));}
    Ok(file)
}

#[cfg(unix)]
fn safe_create_new(path:&Path)->io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file=OpenOptions::new().read(true).write(true).create_new(true).mode(0o600)
        .custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC).open(path)?;
    if !file.metadata()?.is_file(){return Err(io::Error::new(io::ErrorKind::InvalidData,"created target is not regular"));}
    Ok(file)
}
#[cfg(not(unix))]
fn safe_create_new(path:&Path)->io::Result<File> {
    let file=OpenOptions::new().read(true).write(true).create_new(true).open(path)?;
    if !file.metadata()?.is_file(){return Err(io::Error::new(io::ErrorKind::InvalidData,"created target is not regular"));}
    Ok(file)
}

fn open_lock(path:&Path)->io::Result<File> {
    for _ in 0..3 {
        match safe_open_existing(path,true,true,false) {
            Ok(mut file)=>{
                let mut bytes=Vec::new();file.read_to_end(&mut bytes)?;
                if bytes.as_slice()!=LOCK_MAGIC {return Err(io::Error::new(io::ErrorKind::InvalidData,format!("refusing unrelated lock target {}",path.display())));}
                file.seek(SeekFrom::Start(0))?;
                return Ok(file);
            }
            Err(e) if e.kind()==io::ErrorKind::NotFound=>{
                match safe_create_new(path) {
                    Ok(mut file)=>{file.write_all(LOCK_MAGIC)?;file.flush()?;file.sync_data()?;file.seek(SeekFrom::Start(0))?;return Ok(file);},
                    Err(race) if race.kind()==io::ErrorKind::AlreadyExists=>continue,
                    Err(race)=>return Err(race),
                }
            }
            Err(e)=>return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::WouldBlock,"lock-file creation race did not settle"))
}

fn acquire_process_lock(file:&File)->io::Result<ProcessLock<'_>> {
    #[cfg(unix)] {
        use std::os::fd::AsRawFd;
        let started=Instant::now();
        loop {
            let rc=unsafe{libc::flock(file.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)};
            if rc==0{return Ok(ProcessLock{file});}
            let e=io::Error::last_os_error();
            if e.kind()!=io::ErrorKind::WouldBlock {return Err(e);}
            if started.elapsed()>=LOCK_WAIT{return Err(io::Error::new(io::ErrorKind::TimedOut,"log lock wait exceeded 25 ms"));}
            thread::sleep(Duration::from_millis(1));
        }
    }
    #[cfg(not(unix))] {Ok(ProcessLock{file})}
}

fn ensure_log_target(path:&Path)->io::Result<()> {
    let mut file=match safe_open_existing(path,true,true,false) {
        Ok(file)=>file,
        Err(e) if e.kind()==io::ErrorKind::NotFound=>{
            let mut f=safe_create_new(path)?;
            f.write_all(LOG_MAGIC)?;f.flush()?;f.sync_data()?;return Ok(());
        }
        Err(e)=>return Err(e),
    };
    let len=file.metadata()?.len();
    if len==0 {return Err(io::Error::new(io::ErrorKind::InvalidData,format!("refusing unrelated empty existing target {}",path.display())));}
    let mut prefix=vec![0u8;LOG_MAGIC.len()];
    let n=file.read(&mut prefix)?;
    if n<LOG_MAGIC.len() || prefix.as_slice()!=LOG_MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData,format!("refusing unrelated existing target {}",path.display())));
    }
    Ok(())
}

fn utc_timestamp()->String {
    let now=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs=now.as_secs() as i64;
    let millis=now.subsec_millis();
    #[cfg(unix)] unsafe {
        let raw=secs as libc::time_t;
        let mut tm:libc::tm=std::mem::zeroed();
        if !libc::gmtime_r(&raw,&mut tm).is_null() {
            return format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
                tm.tm_year+1900,tm.tm_mon+1,tm.tm_mday,tm.tm_hour,tm.tm_min,tm.tm_sec,millis);
        }
    }
    format!("unix:{secs}.{:03}Z",millis)
}

fn build_record(session:&str,severity:Severity,event_type:&str,body:&str)->Vec<u8> {
    let event_type=short_text(event_type,64).replace('\n',"_").replace('\r',"_");
    let body=body.replace("----- SPACE_ZOO_EVENT_V1 BEGIN -----","----- SPACE_ZOO_EVENT_V1 [BEGIN ESCAPED] -----")
        .replace("----- SPACE_ZOO_EVENT_V1 END -----","----- SPACE_ZOO_EVENT_V1 [END ESCAPED] -----");
    let header=format!("utc={} severity={} session={} event={}\n",utc_timestamp(),severity.as_str(),session,event_type);
    let fixed=EVENT_BEGIN.len()+header.len()+EVENT_END.len();
    let body_budget=MAX_EVENT_BYTES.saturating_sub(fixed);
    let mut record=Vec::with_capacity(MAX_EVENT_BYTES.min(fixed+body.len()+1));
    record.extend_from_slice(EVENT_BEGIN);record.extend_from_slice(header.as_bytes());
    let needs_newline=!body.ends_with('\n');
    let needed=body.len()+if needs_newline{1}else{0};
    if needed<=body_budget {
        record.extend_from_slice(body.as_bytes());
        if needs_newline{record.push(b'\n');}
    } else {
        let note=format!("\n[event_truncated=true original_body_bytes={}]\n",body.len());
        let note_bytes=note.as_bytes();
        let note_len=note_bytes.len().min(body_budget);
        let mut take=body_budget.saturating_sub(note_len).min(body.len());
        while take>0 && !body.is_char_boundary(take){take-=1;}
        record.extend_from_slice(&body.as_bytes()[..take]);
        record.extend_from_slice(&note_bytes[..note_len]);
    }
    record.extend_from_slice(EVENT_END);
    debug_assert!(record.len()<=MAX_EVENT_BYTES);
    record
}

fn read_complete_records(bytes:&[u8])->Vec<(usize,usize)> {
    let mut records=Vec::new();let mut cursor=if bytes.starts_with(LOG_MAGIC){LOG_MAGIC.len()}else{0};
    while cursor<bytes.len() {
        let Some(rel_start)=find_bytes(&bytes[cursor..],EVENT_BEGIN) else {break;};
        let start=cursor+rel_start;
        let after_start=start+EVENT_BEGIN.len();
        let Some(rel_end)=find_bytes(&bytes[after_start..],EVENT_END) else {break;};
        let end=after_start+rel_end+EVENT_END.len();
        records.push((start,end));cursor=end;
    }
    records
}
fn find_bytes(haystack:&[u8],needle:&[u8])->Option<usize> {
    if needle.is_empty(){return Some(0);} haystack.windows(needle.len()).position(|w|w==needle)
}

fn trim_locked(log_path:&Path,launch_dir:&Path)->io::Result<()> {
    let mut source=safe_open_existing(log_path,true,false,false)?;
    let size=source.metadata()?.len();
    if size<=MAX_LOG_BYTES{return Ok(());}
    let mut prefix=vec![0u8;LOG_MAGIC.len()];
    source.read_exact(&mut prefix)?;
    if prefix.as_slice()!=LOG_MAGIC{return Err(io::Error::new(io::ErrorKind::InvalidData,"log magic changed during trim"));}
    let window_cap=(RETAIN_LOG_BYTES+MAX_EVENT_BYTES*2) as u64;
    let window_len=size.min(window_cap) as usize;
    source.seek(SeekFrom::Start(size-window_len as u64))?;
    let mut bytes=vec![0u8;window_len];source.read_exact(&mut bytes)?;
    let records=read_complete_records(&bytes);
    let keep_end=records.last().map(|(_,end)|*end).unwrap_or(0);
    let mut keep_start=keep_end;let mut kept=0usize;
    for &(start,end) in records.iter().rev() {
        let len=end-start;
        if kept>0 && kept+len>RETAIN_LOG_BYTES {break;}
        if kept==0 && len>RETAIN_LOG_BYTES {keep_start=start;break;}
        kept+=len;keep_start=start;
    }
    let suffix=if keep_start<keep_end{&bytes[keep_start..keep_end]}else{&[]};
    let pid=std::process::id();let nonce=SESSION_COUNTER.fetch_add(1,Ordering::Relaxed);
    let temp_path=launch_dir.join(format!(".space_zoo.log.trim.{pid}.{nonce}.tmp"));
    let result=(||{
        let mut temp=safe_create_new(&temp_path)?;
        temp.write_all(LOG_MAGIC)?;temp.write_all(suffix)?;temp.flush()?;temp.sync_data()?;
        // Revalidate the destination immediately before replacement; rename replaces
        // only this directory entry and never follows a symlink target.
        let mut current=safe_open_existing(log_path,true,false,false)?;
        let mut current_prefix=vec![0u8;LOG_MAGIC.len()];
        current.read_exact(&mut current_prefix)?;
        if current_prefix.as_slice()!=LOG_MAGIC{return Err(io::Error::new(io::ErrorKind::InvalidData,"log target changed before trim replacement"));}
        drop(current);
        fs::rename(&temp_path,log_path)?;
        #[cfg(unix)] {let dir=File::open(launch_dir)?;let _=dir.sync_all();}
        Ok(())
    })();
    if result.is_err(){let _=fs::remove_file(&temp_path);} result
}

fn open_append_current(path:&Path)->io::Result<File> {
    let mut file=safe_open_existing(path,true,true,true)?;
    let mut prefix=vec![0u8;LOG_MAGIC.len()];
    file.seek(SeekFrom::Start(0))?;let n=file.read(&mut prefix)?;
    if n<LOG_MAGIC.len() || prefix.as_slice()!=LOG_MAGIC {return Err(io::Error::new(io::ErrorKind::InvalidData,"log target is not Space Zoo log"));}
    file.seek(SeekFrom::End(0))?;Ok(file)
}

fn append_locked(logger:&RuntimeLogger,state:&mut State,severity:Severity,event_type:&str,body:&str,sync:bool)->io::Result<()> {
    let _process=acquire_process_lock(&state.lock_file).map_err(|e|{if e.kind()==io::ErrorKind::TimedOut{warn_once(&logger.warning_lock,&e.to_string());}e})?;
    ensure_log_target(&logger.log_path)?;
    let record=build_record(&logger.session_id,severity,event_type,body);
    let mut log=open_append_current(&logger.log_path)?;
    log.write_all(&record)?;log.flush()?;if sync{log.sync_data()?;}
    let current_size=log.metadata()?.len();
    drop(log);
    if current_size>MAX_LOG_BYTES {
        match trim_locked(&logger.log_path,&logger.launch_dir) {
            Ok(())=>{logger.warning_trim.store(false,Ordering::Relaxed);},
            Err(e)=>{warn_once(&logger.warning_trim,format!("trim failed; existing log preserved: {e}"));}
        }
    }
    Ok(())
}

fn marker_body(logger:&RuntimeLogger,stage:&str)->String {
    format!("SPACE_ZOO_SESSION_MARKER_V1\nsession={}\npid={}\nboot_id={}\nstart_ticks={}\nstage={}\nupdated_utc={}\n",
        logger.session_id,logger.pid,logger.process_identity.boot_id,logger.process_identity.start_ticks,short_text(stage,256),utc_timestamp())
}

fn update_marker(logger:&RuntimeLogger,state:&mut State,stage:&str,sync:bool)->io::Result<()> {
    let body=marker_body(logger,stage);
    let Some(marker)=state.marker.as_mut() else{return Ok(());};
    marker.file.set_len(0)?;marker.file.seek(SeekFrom::Start(0))?;marker.file.write_all(body.as_bytes())?;marker.file.flush()?;if sync{marker.file.sync_data()?;} Ok(())
}

fn parse_marker(path:&Path)->io::Result<MarkerInfo> {
    let mut file=safe_open_existing(path,true,false,false)?;let mut text=String::new();file.take(4096).read_to_string(&mut text)?;
    if !text.starts_with("SPACE_ZOO_SESSION_MARKER_V1\n"){return Err(io::Error::new(io::ErrorKind::InvalidData,"not a Space Zoo marker"));}
    let get=|key:&str| text.lines().find_map(|line|line.strip_prefix(&format!("{key}="))).map(str::to_string);
    Ok(MarkerInfo{session_id:get("session").ok_or_else(||io::Error::new(io::ErrorKind::InvalidData,"marker session missing"))?,
        pid:get("pid").and_then(|s|s.parse().ok()).ok_or_else(||io::Error::new(io::ErrorKind::InvalidData,"marker pid missing"))?,
        boot_id:get("boot_id").unwrap_or_default(),start_ticks:get("start_ticks").and_then(|s|s.parse().ok()).unwrap_or(0),stage:get("stage").unwrap_or_else(||"unknown".into())})
}

#[cfg(target_os="linux")]
fn process_start_ticks(pid:u32)->io::Result<u64> {
    let text=fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let close=text.rfind(')').ok_or_else(||io::Error::new(io::ErrorKind::InvalidData,"malformed proc stat"))?;
    let tail=text.get(close+2..).ok_or_else(||io::Error::new(io::ErrorKind::InvalidData,"malformed proc stat tail"))?;
    tail.split_whitespace().nth(19).ok_or_else(||io::Error::new(io::ErrorKind::InvalidData,"proc start time missing"))?.parse()
        .map_err(|_|io::Error::new(io::ErrorKind::InvalidData,"proc start time invalid"))
}
#[cfg(not(target_os="linux"))]
fn process_start_ticks(_pid:u32)->io::Result<u64>{Ok(0)}

fn current_process_identity()->io::Result<ProcessIdentity> {
    #[cfg(target_os="linux")] let boot_id=fs::read_to_string("/proc/sys/kernel/random/boot_id")?.trim().to_string();
    #[cfg(not(target_os="linux"))] let boot_id=String::from("non-linux");
    Ok(ProcessIdentity{boot_id,start_ticks:process_start_ticks(std::process::id())?})
}

fn marker_is_running(info:&MarkerInfo,current_boot:&str)->Option<bool> {
    #[cfg(target_os="linux")] {
        if !info.boot_id.is_empty() && info.boot_id!=current_boot{return Some(false);}
        match process_start_ticks(info.pid) {Ok(ticks)=>Some(ticks==info.start_ticks),Err(e) if e.kind()==io::ErrorKind::NotFound=>Some(false),Err(_)=>None}
    }
    #[cfg(not(target_os="linux"))] {let _=(info,current_boot);None}
}

fn create_marker(launch_dir:&Path,session_id:&str)->io::Result<Marker> {
    for slot in 0..16u8 {
        let suffix=if slot==0{String::new()}else{format!(".{slot}")};
        let path=launch_dir.join(format!("{MARKER_PREFIX}{session_id}{suffix}{MARKER_SUFFIX}"));
        match safe_create_new(&path){Ok(file)=>return Ok(Marker{path,file}),Err(e) if e.kind()==io::ErrorKind::AlreadyExists=>continue,Err(e)=>return Err(e)}
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists,"could not allocate session marker"))
}

fn report_abandoned_markers(logger:&RuntimeLogger,state:&mut State)->io::Result<()> {
    let entries=fs::read_dir(&logger.launch_dir)?;
    for entry in entries {
        let entry=match entry{Ok(v)=>v,Err(_)=>continue};
        let name=entry.file_name();let name=name.to_string_lossy();
        if !name.starts_with(MARKER_PREFIX)||!name.ends_with(MARKER_SUFFIX){continue;}
        let path=entry.path();
        let info=match parse_marker(&path){Ok(v)=>v,Err(_)=>continue};
        if info.session_id==logger.session_id{continue;}
        match marker_is_running(&info,&logger.process_identity.boot_id) {
            Some(true)=>continue,
            None=>continue,
            Some(false)=>{
                if info.stage=="CLEAN_EXIT_RECORDED" {
                    if fs::remove_file(&path).is_ok() {
                        #[cfg(unix)] if let Ok(dir)=File::open(&logger.launch_dir){let _=dir.sync_all();}
                    }
                    continue;
                }
                let cause=if info.stage=="RUST_PANIC"{"rust_panic"}else{"unknown"};
                let body=format!("PREVIOUS SESSION ENDED WITHOUT CLEAN SHUTDOWN\nprevious_session={}\nprevious_pid={}\nlast_recorded_stage={}\ncause={}\nnote=incomplete marker proves interrupted clean-exit bookkeeping only; it does not establish that Space Zoo caused a host reboot",
                    info.session_id,info.pid,info.stage,cause);
                if append_locked(logger,state,Severity::Warn,"PREVIOUS_INCOMPLETE_SHUTDOWN",&body,true).is_ok(){
                    if fs::remove_file(&path).is_ok() {
                        #[cfg(unix)] if let Ok(dir)=File::open(&logger.launch_dir){let _=dir.sync_all();}
                    }
                }
            }
        }
    }
    Ok(())
}

impl RuntimeLogger {
    fn new()->io::Result<Self> {
        let launch_dir=std::env::current_dir()?;
        let process_identity=current_process_identity()?;
        let pid=std::process::id();
        let nanos=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let sequence=SESSION_COUNTER.fetch_add(1,Ordering::Relaxed);
        let session_id=format!("{:x}-{pid:x}-{:x}-{:x}",nanos,process_identity.start_ticks,sequence);
        let log_path=launch_dir.join(LOG_NAME);let lock_path=launch_dir.join(LOCK_NAME);
        let lock_file=open_lock(&lock_path)?;
        let logger=Self{launch_dir,log_path,session_id,pid,process_identity,
            state:Mutex::new(State{lock_file,marker:None,last_operation:"startup initialization".into(),suppressed:VecDeque::new()}),
            warning_write:AtomicBool::new(false),warning_lock:AtomicBool::new(false),warning_trim:AtomicBool::new(false),warning_marker:AtomicBool::new(false),startup_logged:AtomicBool::new(false)};
        {
            let mut state=logger.state.lock().map_err(|_|io::Error::other("logger state poisoned during startup"))?;
            let _process=acquire_process_lock(&state.lock_file)?;
            ensure_log_target(&logger.log_path)?;
            drop(_process);
            let _=report_abandoned_markers(&logger,&mut state);
            match create_marker(&logger.launch_dir,&logger.session_id) {
                Ok(marker)=>{
                    state.marker=Some(marker);
                    if let Err(e)=update_marker(&logger,&mut state,"STARTUP_MARKER_CREATED",true) {
                        warn_once(&logger.warning_marker,format!("startup marker write failed: {e}"));
                    } else {
                        #[cfg(unix)] if let Ok(dir)=File::open(&logger.launch_dir){let _=dir.sync_all();}
                    }
                },
                Err(e)=>warn_once(&logger.warning_marker,format!("session marker unavailable: {e}")),
            }
        }
        Ok(logger)
    }

    fn try_state(&self)->Option<std::sync::MutexGuard<'_,State>> {
        let started=Instant::now();
        loop {match self.state.try_lock(){Ok(g)=>return Some(g),Err(TryLockError::Poisoned(e))=>return Some(e.into_inner()),Err(TryLockError::WouldBlock)=>{if started.elapsed()>=LOCK_WAIT{return None;}thread::yield_now();}}}
    }

    fn record(&self,severity:Severity,event_type:&str,body:&str,sync:bool)->bool {
        let Some(mut state)=self.try_state() else {warn_once(&self.warning_lock,"in-process log lock wait exceeded 25 ms");return false;};
        match append_locked(self,&mut state,severity,event_type,body,sync) {
            Ok(())=>{
                state.last_operation=short_text(event_type,256);
                if let Err(e)=update_marker(self,&mut state,event_type,sync){warn_once(&self.warning_marker,format!("marker update failed: {e}"));}
                true
            }
            Err(e)=>{warn_once(&self.warning_write,format!("event write failed: {e}"));false}
        }
    }

    fn set_operation(&self,operation:&str) {if let Some(mut state)=self.try_state(){state.last_operation=short_text(operation,256);let marker_event=format!("OPERATION: {}",state.last_operation);let _=update_marker(self,&mut state,&marker_event,false);}}

    fn error(&self,event_type:&str,key:&str,message:&str) {
        let Some(mut state)=self.try_state() else {warn_once(&self.warning_lock,"in-process log lock wait exceeded 25 ms");return;};
        let key=short_text(key,MAX_KEY_CHARS);let fp=fingerprint(event_type,message);
        if let Some(pos)=state.suppressed.iter().position(|e|e.key==key) {
            if state.suppressed[pos].fingerprint==fp {state.suppressed[pos].suppressed=state.suppressed[pos].suppressed.saturating_add(1);return;}
            let prior=state.suppressed.remove(pos).unwrap();
            if prior.suppressed>0 {let _=append_locked(self,&mut state,Severity::Warn,"ERROR_REPEAT_SUMMARY",&format!("key={} event={} suppressed_repeats={} last_message={}",prior.key,prior.event_type,prior.suppressed,prior.message_snippet),false);}
        }
        if state.suppressed.len()>=MAX_SUPPRESSED_KEYS {
            if let Some(prior)=state.suppressed.pop_front(){if prior.suppressed>0{let _=append_locked(self,&mut state,Severity::Warn,"ERROR_REPEAT_SUMMARY",&format!("key={} event={} suppressed_repeats={} reason=bookkeeping_eviction last_message={}",prior.key,prior.event_type,prior.suppressed,prior.message_snippet),false);}}
        }
        let _=append_locked(self,&mut state,Severity::Error,event_type,message,false);
        state.suppressed.push_back(SuppressedError{key,fingerprint:fp,event_type:short_text(event_type,64),message_snippet:short_text(message,MAX_SNIPPET_CHARS),suppressed:0});
    }

    fn recovery(&self,key:&str) {
        let Some(mut state)=self.try_state() else{return;};let key=short_text(key,MAX_KEY_CHARS);
        if let Some(pos)=state.suppressed.iter().position(|e|e.key==key) {let prior=state.suppressed.remove(pos).unwrap();if prior.suppressed>0{let _=append_locked(self,&mut state,Severity::Info,"ERROR_REPEAT_SUMMARY",&format!("key={} event={} suppressed_repeats={} status=recovered last_message={}",prior.key,prior.event_type,prior.suppressed,prior.message_snippet),false);}}
    }

    fn flush_suppressed(&self,state:&mut State) {while let Some(prior)=state.suppressed.pop_front(){if prior.suppressed>0{let _=append_locked(self,state,Severity::Warn,"ERROR_REPEAT_SUMMARY",&format!("key={} event={} suppressed_repeats={} status=shutdown last_message={}",prior.key,prior.event_type,prior.suppressed,prior.message_snippet),false);}}}

    fn panic_record(&self,info:&std::panic::PanicHookInfo<'_>) {
        let payload=if let Some(s)=info.payload().downcast_ref::<&str>(){(*s).to_string()}else if let Some(s)=info.payload().downcast_ref::<String>(){s.clone()}else{"<non-string panic payload>".into()};
        let location=info.location().map(|l|format!("{}:{}:{}",l.file(),l.line(),l.column())).unwrap_or_else(||"unknown".into());
        let mut state=match self.state.try_lock() {
            Ok(g)=>g,
            Err(TryLockError::Poisoned(e))=>e.into_inner(),
            Err(TryLockError::WouldBlock)=>{
                eprintln!("Space Zoo panic logging fallback: logger busy; panic={payload} location={location}\n{:?}",Backtrace::force_capture());
                return;
            }
        };
        let last=state.last_operation.clone();
        let body=format!("message={payload}\nsource_location={location}\nlast_known_operation={last}\nforced_backtrace:\n{:?}",Backtrace::force_capture());
        if append_locked(self,&mut state,Severity::Fatal,"RUST_PANIC",&body,true).is_ok(){let _=update_marker(self,&mut state,"RUST_PANIC",true);}else{eprintln!("Space Zoo panic logging fallback: {body}");}
    }

    fn finalize(&self,reason:&str,exit_code:i32,cleanup_succeeded:bool) {
        let Some(mut state)=self.try_state() else{return;};
        self.flush_suppressed(&mut state);
        let _=append_locked(self,&mut state,Severity::Info,"NORMAL_EXIT",&format!("reason={} intended_exit_code={}",reason,exit_code),true);
        if cleanup_succeeded {
            match append_locked(self,&mut state,Severity::Info,"CLEAN_EXIT",&format!("cleanup_succeeded=true intended_exit_code={}",exit_code),true) {
                Ok(())=>{
                    let _=update_marker(self,&mut state,"CLEAN_EXIT_RECORDED",true);
                    if let Some(marker)=state.marker.take(){
                        drop(marker.file);
                        if let Err(e)=fs::remove_file(&marker.path){warn_once(&self.warning_marker,format!("clean marker removal failed: {e}"));}
                        #[cfg(unix)] if let Ok(dir)=File::open(&self.launch_dir){let _=dir.sync_all();}
                    }
                }
                Err(e)=>warn_once(&self.warning_write,format!("clean-exit event write failed: {e}")),
            }
        }
    }
}

/// Resolve and initialize launch-directory logging. Failure is nonfatal and is
/// reported once to stderr; no alternate path is selected.
pub fn initialize() {
    if LOGGER.get().is_some(){return;}
    CLEANUP_FAILED.store(false,Ordering::Relaxed);
    match RuntimeLogger::new(){Ok(logger)=>{let _=LOGGER.set(logger);},Err(e)=>warn_once(&INIT_WARNING,format!("initialization failed in launch directory; continuing without persistent log: {e}"))}
}
pub fn launch_dir()->Option<PathBuf>{LOGGER.get().map(|l|l.launch_dir.clone())}
pub fn session_id()->Option<String>{LOGGER.get().map(|l|l.session_id.clone())}
pub fn startup(version:&str,backend:&str){if let Some(l)=LOGGER.get(){if l.startup_logged.swap(true,Ordering::Relaxed){return;}let body=format!("version={} build=v{}\npid={}\nlaunch_directory={}\ndisplay_backend={}",version,version,l.pid,l.launch_dir.display(),backend);let _=l.record(Severity::Info,"STARTUP",&body,true);}}
pub fn event(severity:Severity,event_type:&str,body:&str){if let Some(l)=LOGGER.get(){let _=l.record(severity,event_type,body,false);}}
pub fn durable_event(severity:Severity,event_type:&str,body:&str){if let Some(l)=LOGGER.get(){let _=l.record(severity,event_type,body,true);}}
pub fn error(event_type:&str,key:&str,message:&str){if let Some(l)=LOGGER.get(){l.error(event_type,key,message);}}
pub fn recovered(key:&str){if let Some(l)=LOGGER.get(){l.recovery(key);}}
pub fn operation(operation:&str){if let Some(l)=LOGGER.get(){l.set_operation(operation);}}
pub fn panic(info:&std::panic::PanicHookInfo<'_>){if let Some(l)=LOGGER.get(){l.panic_record(info);}else{eprintln!("Space Zoo panic logging fallback: persistent logger unavailable\n{:?}",Backtrace::force_capture());}}
pub fn termination(signal:&str){durable_event(Severity::Warn,"TERMINATION_REQUEST",&format!("signal={signal}"));}
pub fn cleanup(stage:&str){durable_event(Severity::Info,"CLEANUP",&format!("stage={stage}"));operation(&format!("cleanup: {stage}"));}
pub fn cleanup_failure(stage:&str,message:&str){CLEANUP_FAILED.store(true,Ordering::Relaxed);error("CLEANUP_FAILURE",&format!("cleanup.{stage}"),message);}
pub fn cleanup_succeeded()->bool{!CLEANUP_FAILED.load(Ordering::Relaxed)}
pub fn finalize(reason:&str,exit_code:i32,cleanup_succeeded:bool){if let Some(l)=LOGGER.get(){l.finalize(reason,exit_code,cleanup_succeeded);}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn event_cap_marks_truncation(){let body="X".repeat(MAX_EVENT_BYTES*2);let record=build_record("s",Severity::Error,"TEST",&body);assert!(record.len()<=MAX_EVENT_BYTES);assert!(String::from_utf8_lossy(&record).contains("event_truncated=true"));}
    #[test] fn parser_ignores_incomplete_tail(){let a=build_record("s",Severity::Info,"A","one\ntwo");let b=build_record("s",Severity::Info,"B","three");let mut all=LOG_MAGIC.to_vec();all.extend(&a);all.extend(&b[..b.len()/2]);let records=read_complete_records(&all);assert_eq!(records.len(),1);}
}
