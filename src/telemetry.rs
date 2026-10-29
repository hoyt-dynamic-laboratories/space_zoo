//! Bertha's tiny Linux /proc monitor. Only the worker touches /proc;
//! rendering receives a finished luxury-console snapshot every 2000 ms.
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::fs;
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

/// Bertha host aggregate (for compatibility) plus precisely five CPU-ranked processes.
pub const BILLBOARD_ROWS: usize = 6;
/// Number of process rows actually drawn. Host stats live in the meters above.
pub const TOP_PROCESSES: usize = BILLBOARD_ROWS - 1;
pub const NET_HISTORY: usize = 37;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub mem: String,
    pub cpu: String,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub rows: Vec<Row>, // Bertha aggregate then exactly 5 ranked processes when available.
    pub dies: Vec<Die>, // CPU package groups, 1 LED per logical CPU
    pub system: SystemStats,
}

#[derive(Clone, Debug, Default)]
pub struct Meter {
    pub used: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Network {
    pub interface: String,
    pub rx_per_sec: u64,
    pub tx_per_sec: u64,
    pub rx_history: Vec<u64>,
    pub tx_history: Vec<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct SystemStats {
    pub memory: Meter,
    pub disk: Meter,
    pub network: Network,
    pub hostname: String,
    pub cpu_model: String,
    /// Representative live CPU frequency: arithmetic mean of every readable
    /// logical CPU's current cpufreq value, expressed in MHz.
    pub cpu_mhz: Option<u64>,
}
// The display is deliberately categorical: an invalid reading must never
// masquerade as measured idle, and rounding a tiny load must never make it green.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuBand { Idle, Yellow, Orange, Red }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreState { Invalid, Valid(CpuBand) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreReading {
    pub id: u32,         // Stable Linux logical CPU identity.
    pub state: CoreState,
}

#[derive(Clone, Debug, Default)]
pub struct Die {
    pub package: u32,
    pub temp_c: Option<i32>, // Package sensor, not a guessed/averaged core temp.
    pub cores: Vec<CoreReading>, // Stable CPU-id order; missing samples stay in place.
}
#[derive(Clone, Debug, Default)]
struct CpuPoll {
    aggregate: Option<(u64,u64)>,
    cores: Vec<(u32,u64,u64)>, // cpu index, total ticks, idle+iowait ticks
}

#[derive(Debug)]
struct Process {
    name: String,
    ticks: u64,
    start: u64, // Detect PID reuse between two polls.
    rss: u64,   // Resident memory, bytes.
}

type Processes = HashMap<u32, Process>;

// Always publish the newest values. The old 1-slot channel dropped NEW data
// whenever the consumer had not yet fetched an older snapshot. Never queue
// historical billboard states or hold a graphics frame waiting for /proc.
#[derive(Clone)]
pub struct Monitor {
    latest: Arc<Mutex<Option<Snapshot>>>,
}

impl Monitor {
    pub fn take(&self) -> Option<Snapshot> {
        self.latest.try_lock().ok()?.take()
    }
}

#[cfg(target_os = "linux")]
pub fn spawn() -> Monitor {
    let latest = Arc::new(Mutex::new(None));
    let shared: Weak<Mutex<Option<Snapshot>>> = Arc::downgrade(&latest);
    thread::spawn(move || {
        let mut previous_system = read_cpu_poll();
        let mut packages = detect_packages(&previous_system.cores);
        let temperature_paths = find_temperature_paths();
        let mut previous_processes = read_processes();
        // Host identity is effectively static for one Space Zoo session. Keep
        // it out of the two-second hot path while sampling frequency live.
        let hostname = read_hostname();
        let cpu_model = read_cpu_model();
        // Prefer the kernel's default-route interface; never include loopback
        // or an unrelated Docker/veth interface in the machine's traffic.
        let interface = preferred_network_interface();
        let mut previous_network = interface.as_deref().and_then(read_net_bytes);
        let mut previous_net_at = Instant::now();
        let mut rx_history = std::collections::VecDeque::with_capacity(NET_HISTORY);
        let mut tx_history = std::collections::VecDeque::with_capacity(NET_HISTORY);
        let mut next = Instant::now() + Duration::from_millis(2000);
        loop {
            thread::sleep(next.saturating_duration_since(Instant::now()));
            let sampled_at = Instant::now();
            let current_system = read_cpu_poll();
            // Only retry discovery if startup /proc/stat was unreadable.
            // CPU hotplug/reconfiguration is intentionally out of scope.
            if packages.is_empty() && !current_system.cores.is_empty() {
                packages = detect_packages(&current_system.cores);
            }
            let current_processes = read_processes();
            let memory = read_mem_stats();
            let mut snapshot = make_snapshot(
                previous_system.aggregate, current_system.aggregate,
                &previous_processes, &current_processes,
                memory.used,
            );
            snapshot.dies = calculate_dies(&previous_system.cores,&current_system.cores,
                &packages,&temperature_paths);
            snapshot.system.memory = memory;
            snapshot.system.disk = read_local_disks();
            snapshot.system.hostname = hostname.clone();
            snapshot.system.cpu_model = cpu_model.clone();
            snapshot.system.cpu_mhz = read_cpu_frequency_mhz();
            let current_network = interface.as_deref().and_then(read_net_bytes);
            let elapsed = sampled_at.duration_since(previous_net_at).as_secs_f64();
            let (rx,tx) = match (previous_network,current_network) {
                (Some((pr,pt)),Some((cr,ct))) if elapsed>0.0 => (
                    ((cr.saturating_sub(pr) as f64)/elapsed) as u64,
                    ((ct.saturating_sub(pt) as f64)/elapsed) as u64,
                ),
                _ => (0,0),
            };
            previous_network = current_network;
            previous_net_at = sampled_at;
            rx_history.push_back(rx);
            tx_history.push_back(tx);
            if rx_history.len()>NET_HISTORY {rx_history.pop_front();}
            if tx_history.len()>NET_HISTORY {tx_history.pop_front();}
            snapshot.system.network = Network {
                interface: interface.clone().unwrap_or_default(),
                rx_per_sec: rx, tx_per_sec: tx,
                rx_history: rx_history.iter().copied().collect(),
                tx_history: tx_history.iter().copied().collect(),
            };
            let Some(consumer) = shared.upgrade() else { break; };
            if let Ok(mut slot) = consumer.lock() {
                *slot = Some(snapshot);
            } else { break; }
            previous_system = current_system;
            previous_processes = current_processes;
            next = sampled_at + Duration::from_millis(2000);
        }
    });
    Monitor { latest }
}

// /proc/stat's first aggregate 'cpu' line; guest fields are already counted
// in user/nice and must not be counted a second time.
// One /proc/stat read supplies both the whole-machine display and all cores.
fn read_cpu_poll()->CpuPoll {
    let mut result=CpuPoll::default();
    let Ok(text)=fs::read_to_string("/proc/stat") else {return result;};
    for line in text.lines().take_while(|s|s.starts_with("cpu")){
        let mut parts=line.split_ascii_whitespace();
        let Some(label)=parts.next() else {continue;};
        let Ok(values)=parts.take(8).map(str::parse::<u64>).collect::<Result<Vec<_>,_>>() else {continue;};
        if values.len()!=8 {continue;}
        let Some(total)=values.iter().try_fold(0u64,|sum,&v|sum.checked_add(v)) else {continue;};
        let Some(idle)=values[3].checked_add(values[4]) else {continue;};
        if label=="cpu" {result.aggregate=Some((total,idle));}
        else if let Some(id)=label.strip_prefix("cpu").and_then(|s|s.parse::<u32>().ok()) {
            result.cores.push((id,total,idle));
        }
    }
    result
}

fn read_hostname() -> String {
    ["/proc/sys/kernel/hostname", "/etc/hostname"].into_iter()
        .find_map(|path| fs::read_to_string(path).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

fn parse_cpu_model(text:&str) -> String {
    for key in ["model name", "Hardware", "Processor"] {
        if let Some(value)=text.lines().find_map(|line| {
            let (name,value)=line.split_once(':')?;
            (name.trim()==key).then(|| value.trim().to_string())
        }) {
            if !value.is_empty() {return value;}
        }
    }
    String::new()
}

fn read_cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo").ok()
        .map(|text| parse_cpu_model(&text)).unwrap_or_default()
}

fn parse_cpuinfo_mhz(text:&str) -> Vec<f64> {
    text.lines().filter_map(|line| {
        let (name,value)=line.split_once(':')?;
        if name.trim()!="cpu MHz" {return None;}
        let mhz=value.trim().parse::<f64>().ok()?;
        (mhz.is_finite() && mhz>0.0).then_some(mhz)
    }).collect()
}

/// Sample the current scaling frequency rather than advertised/base clock.
/// Sysfs is preferred because its units and meaning are explicit; /proc is a
/// portable Linux fallback for systems without exposed cpufreq policy files.
fn read_cpu_frequency_mhz() -> Option<u64> {
    let mut khz=Vec::new();
    if let Ok(entries)=fs::read_dir("/sys/devices/system/cpu") {
        for entry in entries.flatten() {
            let name=entry.file_name();
            let name=name.to_string_lossy();
            let Some(id)=name.strip_prefix("cpu") else {continue;};
            if id.is_empty() || !id.bytes().all(|b|b.is_ascii_digit()) {continue;}
            let dir=entry.path().join("cpufreq");
            let value=["scaling_cur_freq","cpuinfo_cur_freq"].into_iter().find_map(|file| {
                fs::read_to_string(dir.join(file)).ok()
                    .and_then(|s|s.trim().parse::<u64>().ok())
                    .filter(|&v|v>0)
            });
            if let Some(v)=value {khz.push(v);}
        }
    }
    if !khz.is_empty() {
        let sum=khz.iter().fold(0u128,|sum,&v|sum+v as u128);
        let mean_khz=(sum+(khz.len() as u128/2))/khz.len() as u128;
        return u64::try_from((mean_khz+500)/1000).ok();
    }
    let text=fs::read_to_string("/proc/cpuinfo").ok()?;
    let values=parse_cpuinfo_mhz(&text);
    if values.is_empty() {return None;}
    let mean=values.iter().sum::<f64>()/values.len() as f64;
    Some(mean.round().clamp(1.0,u64::MAX as f64) as u64)
}

// Package IDs come from the Linux scheduler topology, not guesses based on
// cpuN numbering or hyperthread sibling layout. Read once on worker startup.
fn detect_packages(cores:&[(u32,u64,u64)])->HashMap<u32,u32>{
    cores.iter().map(|(id,_,_)|{
        let path=format!("/sys/devices/system/cpu/cpu{id}/topology/physical_package_id");
        let pkg=fs::read_to_string(path).ok().and_then(|s|s.trim().parse().ok()).unwrap_or(0);
        (*id,pkg)
    }).collect()
}
fn find_temperature_paths()->HashMap<u32,PathBuf>{
    let mut paths=HashMap::new();
    let Ok(entries)=fs::read_dir("/sys/class/hwmon") else {return paths;};
    for entry in entries.flatten(){
        let dir=entry.path();
        let Ok(name)=fs::read_to_string(dir.join("name")) else {continue;};
        if name.trim()!="coretemp" {continue;}
        for idx in 1..=64 {
            let base=format!("temp{idx}");
            let Ok(label)=fs::read_to_string(dir.join(format!("{base}_label"))) else {continue;};
            if let Some(pkg)=label.trim().strip_prefix("Package id ").and_then(|s|s.parse().ok()){
                paths.insert(pkg,dir.join(format!("{base}_input")));
            }
        }
    }
    paths
}
// Counter differences are checked, not saturated: impossible time travel,
// missing baselines, and zero elapsed ticks are not evidence of a 0% CPU.
fn classify_cpu(previous:Option<(u64,u64)>,current:Option<(u64,u64)>)->CoreState {
    let (Some((pt,pi)),Some((total,idle)))=(previous,current) else {return CoreState::Invalid;};
    if pi>pt || idle>total || total<=pt || idle<pi {return CoreState::Invalid;}
    let dt=total-pt;
    let di=idle-pi;
    if di>dt {return CoreState::Invalid;}
    let busy=dt-di;
    // u128 keeps threshold comparisons exact even with large u64 counters.
    let band=if busy==0 {CpuBand::Idle}
        else if (busy as u128)*4 < dt as u128 {CpuBand::Yellow}
        else if (busy as u128)*4 < (dt as u128)*3 {CpuBand::Orange}
        else {CpuBand::Red};
    CoreState::Valid(band)
}

fn calculate_dies(previous:&[(u32,u64,u64)],current:&[(u32,u64,u64)],
    packages:&HashMap<u32,u32>,temperatures:&HashMap<u32,PathBuf>)->Vec<Die>{
    let old:HashMap<u32,(u64,u64)>=previous.iter().map(|&(id,t,i)|(id,(t,i))).collect();
    let now:HashMap<u32,(u64,u64)>=current.iter().map(|&(id,t,i)|(id,(t,i))).collect();
    let mut grouped:BTreeMap<u32,Vec<CoreReading>>=BTreeMap::new();
    // Build each matrix from the stable topology, NOT from the last read's
    // surviving CPU lines. An omitted/malformed line keeps its original tile.
    for (&id,&package) in packages {
        let state=classify_cpu(old.get(&id).copied(),now.get(&id).copied());
        grouped.entry(package).or_default().push(CoreReading{id,state});
    }
    grouped.into_iter().map(|(package,mut cores)|{
        cores.sort_unstable_by_key(|core|core.id);
        let temp_c=temperatures.get(&package)
            .and_then(|p|fs::read_to_string(p).ok())
            .and_then(|s|s.trim().parse::<i32>().ok())
            .map(|milli|milli/1000);
        Die{package,temp_c,cores}
    }).collect()
}

/// Same available-memory accounting as btop's default Linux RAM gauge.
fn read_mem_stats() -> Meter {
    let Ok(text) = fs::read_to_string("/proc/meminfo") else { return Meter::default(); };
    let mut total = None;
    let mut available = None;
    for line in text.lines() {
        if let Some(s) = line.strip_prefix("MemTotal:") {
            total = s.split_ascii_whitespace().next().and_then(|n| n.parse::<u64>().ok());
        } else if let Some(s) = line.strip_prefix("MemAvailable:") {
            available = s.split_ascii_whitespace().next().and_then(|n| n.parse::<u64>().ok());
        }
        if total.is_some() && available.is_some() { break; }
    }
    let total = total.unwrap_or(0).saturating_mul(1024);
    let available = available.unwrap_or(0).saturating_mul(1024);
    Meter {used: total.saturating_sub(available), total}
}

// A device can have several bind mounts; Btrfs subvolumes and local FUSE
// filesystems can also have different synthetic st_dev IDs for one source.
#[cfg(target_os = "linux")]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum DiskIdentity {
    Device(u32,u32),
    Source(PathBuf),
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct DiskMount {
    identity: DiskIdentity,
    device: (u32,u32),
    path: PathBuf,
}

// /proc mount paths escape whitespace and backslashes as octal bytes.
// Preserve non-UTF-8 Linux filenames instead of lossy string conversion.
#[cfg(target_os = "linux")]
fn decode_mount_path(raw:&[u8])->Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let mut bytes=Vec::with_capacity(raw.len());
    let mut i=0;
    while i<raw.len() {
        if raw[i]==b'\\' {
            let oct=raw.get(i+1..i+4)?;
            if !oct.iter().all(|b|(b'0'..=b'7').contains(b)) {return None;}
            let value=((oct[0]-b'0') as u16)*64+((oct[1]-b'0') as u16)*8+(oct[2]-b'0') as u16;
            if value==0 || value>255 {return None;}
            bytes.push(value as u8);i+=4;
        } else {
            if raw[i]==0 {return None;}
            bytes.push(raw[i]);i+=1;
        }
    }
    Some(std::ffi::OsString::from_vec(bytes).into())
}

#[cfg(target_os = "linux")]
fn parse_disk_mounts(text:&[u8])->Vec<DiskMount> {
    text.split(|b|*b==b'\n').filter_map(|line| {
        let fields:Vec<&[u8]>=line.split(|b|b.is_ascii_whitespace())
            .filter(|s|!s.is_empty()).collect();
        let separator=fields.iter().position(|s|*s==b"-")?;
        if separator<6 || fields.len()<separator+4 {return None;}
        let kind=std::str::from_utf8(fields[separator+1]).ok()?;
        // Positive local-filesystem list: never probe network, autofs, RAM,
        // overlay, pseudo, or Snap/squashfs mounts in the telemetry worker.
        if !matches!(kind,"ext2"|"ext3"|"ext4"|"xfs"|"btrfs"|"bcachefs"|
            "f2fs"|"jfs"|"reiserfs"|"nilfs2"|"vfat"|"msdos"|"exfat"|
            "ntfs"|"ntfs3"|"fuseblk"|"fuse.exfat"|"fuse.ntfs-3g"|"udf"|"iso9660") {
            return None;
        }
        let (major,minor)=std::str::from_utf8(fields[2]).ok()?.split_once(':')?;
        let device=(major.parse::<u32>().ok()?,minor.parse::<u32>().ok()?);
        let source=decode_mount_path(fields[separator+2])?;
        if !source.starts_with("/dev") {return None;}
        let source=fs::canonicalize(&source).unwrap_or(source);
        let name=source.file_name()?.to_str()?;
        // Fixed loop/ram majors plus canonical source names cover /dev aliases
        // and Btrfs's synthetic (major 0) device numbers on loop-backed images.
        if matches!(device.0,1|7) || name.starts_with("loop") ||
            name.starts_with("ram") || name.starts_with("zram") {return None;}
        let path=decode_mount_path(fields[4])?;
        if !path.is_absolute() {return None;}
        let identity=if kind=="btrfs" || kind.starts_with("fuse") {
            DiskIdentity::Source(source)
        } else {DiskIdentity::Device(device.0,device.1)};
        Some(DiskMount{identity,device,path})
    }).collect()
}

// Query duplicate mounts only if the first path is unreadable. A failed
// filesystem makes the overall gauge N/A, never a misleading partial total.
#[cfg(target_os = "linux")]
fn sum_disk_mounts(mounts:&[DiskMount],mut query:impl FnMut(&DiskMount)->Option<Meter>)->Meter {
    let mut groups:HashMap<&DiskIdentity,Vec<&DiskMount>>=HashMap::new();
    for mount in mounts {groups.entry(&mount.identity).or_default().push(mount);}
    let mut result=Meter::default();
    for paths in groups.values() {
        let Some(meter)=paths.iter().find_map(|m|query(m)) else {return Meter::default();};
        if meter.total==0 || meter.used>meter.total {return Meter::default();}
        let (Some(used),Some(total))=(result.used.checked_add(meter.used),result.total.checked_add(meter.total))
            else {return Meter::default();};
        result=Meter{used,total};
    }
    result
}

/// Mounted local filesystem occupancy, refreshed each telemetry poll so mount
/// changes are picked up automatically. No file scanning or shell commands.
#[cfg(target_os = "linux")]
fn read_local_disks() -> Meter {
    let Ok(text)=fs::read("/proc/self/mountinfo") else {return Meter::default();};
    sum_disk_mounts(&parse_disk_mounts(&text),read_disk_mount)
}

#[cfg(target_os = "linux")]
fn read_disk_mount(mount:&DiskMount)->Option<Meter> {
    use std::mem::MaybeUninit;
    use std::os::unix::{ffi::OsStrExt,fs::MetadataExt};
    let path=std::ffi::CString::new(mount.path.as_os_str().as_bytes()).ok()?;
    let same_device=|| {
        fs::metadata(&mount.path).ok().map(|m| {
            // SAFETY: libc 0.2.159 marks these scalar device-number extractors
            // unsafe. Both receive the kernel-provided st_dev value; neither
            // dereferences pointers or accesses external memory.
            let device=unsafe {
                (libc::major(m.dev()) as u32,libc::minor(m.dev()) as u32)
            };
            device==mount.device
        }).unwrap_or(false)
    };
    // A disappearing or covered mount must not silently count its parent.
    if !same_device() {return None;}
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    let rc = unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) };
    if rc != 0 || !same_device() {return None;}
    // The syscall succeeded, so the output struct is initialized.
    let stat = unsafe { stat.assume_init() };
    let unit = u64::try_from(stat.f_frsize).unwrap_or(0);
    let blocks = u64::try_from(stat.f_blocks).unwrap_or(0);
    let free = u64::try_from(stat.f_bfree).unwrap_or(0);
    if unit==0 {return None;}
    Some(Meter {
        used: blocks.checked_sub(free)?.checked_mul(unit)?,
        total: blocks.checked_mul(unit)?,
    })
}

/// Default IPv4-route device; fallback to the first non-loopback active link.
/// Select once on startup to avoid spuriously changing counter baselines.
#[cfg(target_os = "linux")]
fn preferred_network_interface() -> Option<String> {
    if let Ok(routes)=fs::read_to_string("/proc/net/route") {
        for line in routes.lines().skip(1) {
            let cols:Vec<_>=line.split_ascii_whitespace().collect();
            if cols.len()>3 && cols[1]=="00000000" && u32::from_str_radix(cols[3],16)
                .map(|v|v&1!=0).unwrap_or(false) && cols[0]!="lo" {
                return Some(cols[0].into());
            }
        }
    }
    let mut candidates=Vec::new();
    for e in fs::read_dir("/sys/class/net").ok()?.flatten() {
        let name=e.file_name().to_string_lossy().to_string();
        if name=="lo" || name.starts_with("veth") || name.starts_with("docker") {continue;}
        let state=fs::read_to_string(e.path().join("operstate")).unwrap_or_default();
        if state.trim()=="up" || state.trim()=="unknown" {candidates.push(name);}
    }
    candidates.sort();
    candidates.into_iter().next()
}

/// Parse the kernel's RX/TX byte counters for one interface (not packets).
#[cfg(target_os = "linux")]
fn read_net_bytes(iface:&str) -> Option<(u64,u64)> {
    let text=fs::read_to_string("/proc/net/dev").ok()?;
    parse_net_bytes(&text,iface)
}
fn parse_net_bytes(text:&str,iface:&str)->Option<(u64,u64)> {
    for line in text.lines() {
        let Some((name,data))=line.split_once(':') else {continue;};
        if name.trim()!=iface {continue;}
        let nums:Vec<u64>=data.split_ascii_whitespace()
            .map(|x|x.parse::<u64>()).collect::<Result<_,_>>().ok()?;
        if nums.len()>=9 {return Some((nums[0],nums[8]));}
    }
    None
}

fn read_processes() -> Processes {
    let mut found = HashMap::new();
    let Ok(entries) = fs::read_dir("/proc") else { return found; };
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let page_size = if page_size > 0 { page_size as u64 } else { 4096 };
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue; };
        let Ok(line) = fs::read_to_string(entry.path().join("stat")) else { continue; };
        if let Some(process) = parse_process(&line, page_size) { found.insert(pid, process); }
    }
    found
}

fn parse_process(line: &str, page_size: u64) -> Option<Process> {
    // 'comm' may contain spaces or ')' characters; locate the LAST closing
    // parenthesis, then parse the numbered fields following state (field 3).
    let left = line.find('(')?;
    let right = line.rfind(')')?;
    if right <= left { return None; }
    let name = line[left + 1..right].to_string();
    let fields: Vec<&str> = line[right + 1..].split_ascii_whitespace().collect();
    // fields[0] = state (3), [11] = utime (14), [12] = stime (15),
    // [19] = starttime (22), [21] = rss in pages (24).
    if fields.len() < 22 { return None; }
    let ticks = fields[11].parse::<u64>().ok()?.saturating_add(fields[12].parse::<u64>().ok()?);
    Some(Process {
        name, ticks,
        start: fields[19].parse().ok()?,
        rss: fields[21].parse::<u64>().ok()?.saturating_mul(page_size),
    })
}

fn make_snapshot(
    prev_cpu: Option<(u64, u64)>, cpu: Option<(u64, u64)>,
    previous: &Processes, current: &Processes, used: u64,
) -> Snapshot {
    // Both host and process percentages use the SAME aggregate /proc/stat
    // interval. This is total-machine capacity, not percent of one CPU core.
    // On 32 logical cores, a process using 16% of one core is ~0.5% here.
    // The denominator adapts automatically to any host CPU count.
    // A missing, stopped, or inconsistent aggregate is UNKNOWN, not idle.
    // Match the per-core validity checks without changing their classifier.
    // Use checked counter intervals, never saturate a corrupt idle delta.
    let aggregate_delta = match (prev_cpu, cpu) {
        (Some((pt, pi)), Some((ct, ci)))
            if pi <= pt && ci <= ct && ct > pt && ci >= pi && ci - pi <= ct - pt =>
        {
            Some((ct - pt, ci - pi)) // total ticks, idle ticks
        },
        _ => None,
    };
    let host_cpu = match aggregate_delta {
        Some((total, idle)) => format_cpu(((total - idle) as f64 / total as f64) * 100.0),
        None => "--".into(),
    };
    let mut ranked: Vec<(&str, u64, u64)> = current.iter().filter_map(|(pid, p)| {
        let before = previous.get(pid)?;
        if before.start != p.start { return None; }
        Some((p.name.as_str(), p.ticks.saturating_sub(before.ticks), p.rss))
    }).collect();
    // Highest CPU first. Name tie-break avoids gratuitous reshuffles at 0%.
    ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let mut rows = Vec::with_capacity(BILLBOARD_ROWS);
    rows.push(Row { name: "bertha".into(), mem: format_mem(used), cpu: host_cpu });
    for (name, ticks, rss) in ranked.into_iter().take(BILLBOARD_ROWS - 1) {
        rows.push(Row {
            name: name.into(), mem: format_mem(rss),
            cpu: match aggregate_delta {
                Some((total, _)) => format_cpu((ticks as f64 / total as f64) * 100.0),
                None => "--".into(),
            },
        });
    }
    Snapshot { rows, dies: Vec::new(), system: SystemStats::default() }
}

fn format_mem(bytes: u64) -> String {
    let mib = bytes as f64 / 1_048_576.0;
    let gib = mib / 1024.0;
    if gib >= 10.0 { format!("{:.0}g", gib) }
    else if gib >= 1.0 { format!("{:.1}g", gib) }
    else if mib < 100.0 {format!("{:.1}m",mib)}
    else { format!("{:.0}m", mib) }
}

fn format_cpu(percent: f64) -> String {
    if !percent.is_finite() || percent <= 0.0 { return "0%".into(); }
    if percent >= 0.95 { format!("{:.0}%", percent.min(9999.0)) }
    else {
        let tenths = (percent * 10.0).round() as u32;
        if tenths == 0 { "0%".into() } else { format!("0.{tenths}%") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_units_and_percentages() {
        assert_eq!(format_mem(3 * 1024 * 1024), "3.0m");
        assert_eq!(format_mem(1_288_490_189), "1.2g");
        assert_eq!(format_cpu(0.61), "0.6%");
        assert_eq!(format_cpu(3.23), "3%");
        assert_eq!(format_cpu(0.0), "0%");
    }
    #[test]
    fn parses_host_cpu_identity_and_live_frequency_fallback_fields() {
        let cpuinfo="processor : 0\nmodel name : AMD Ryzen 9 7950X 16-Core Processor\ncpu MHz : 823.500\n\
                     processor : 1\nmodel name : AMD Ryzen 9 7950X 16-Core Processor\ncpu MHz : 5123.250\n";
        assert_eq!(parse_cpu_model(cpuinfo),"AMD Ryzen 9 7950X 16-Core Processor");
        assert_eq!(parse_cpuinfo_mhz(cpuinfo),vec![823.5,5123.25]);
    }
    #[test]
    fn ranks_cpu_and_ignores_reused_pids() {
        let proc = |name: &str, ticks: u64, start: u64| Process { name: name.into(), ticks, start, rss: 3 * 1024 * 1024 };
        let prev = HashMap::from([(1, proc("slow", 10, 1)), (2, proc("fast", 10, 1)), (3, proc("reused", 99, 1))]);
        let now = HashMap::from([(1, proc("slow", 11, 1)), (2, proc("fast", 90, 1)), (3, proc("reused", 9999, 2))]);
        let snapshot = make_snapshot(Some((100, 30)), Some((200, 35)), &prev, &now, 1_288_490_189);
        assert_eq!(snapshot.rows[0].name, "bertha");
        assert_eq!(snapshot.rows[1].name, "fast");
        assert_eq!(snapshot.rows[2].name, "slow");
        assert_eq!(snapshot.rows.len(), 3);
    }
    #[test]
    fn cpu_ranked_processes_plus_host_are_retained() {
        let mut previous = HashMap::new();
        let mut current = HashMap::new();
        for pid in 1..=20 {
            previous.insert(pid, Process {name: format!("p{pid}"), ticks: 1,
                start: pid as u64, rss: 4096});
            current.insert(pid, Process {name: format!("p{pid}"), ticks: pid as u64 * 10,
                start: pid as u64, rss: 4096});
        }
        let snap=make_snapshot(Some((0,0)), Some((10000,8000)),
            &previous, &current, 1024 * 1024);
        assert_eq!(snap.rows.len(),BILLBOARD_ROWS);
        assert_eq!(snap.rows[0].name,"bertha");
        assert_eq!(snap.rows[1].name,"p20");
        assert_eq!(snap.rows[5].name,"p16");
    }
    #[test]
    fn snapshot_carries_both_metrics_for_every_row() {
        let proc = |name: &str, ticks: u64, rss: u64| Process {
            name: name.into(), ticks, start: 42, rss,
        };
        let previous = HashMap::from([
            (1, proc("alpha", 10, 3 * 1048576)),
            (2, proc("bravo", 10, 80 * 1048576)),
        ]);
        let current = HashMap::from([
            (1, proc("alpha", 12, 3 * 1048576)),
            (2, proc("bravo", 40, 80 * 1048576)),
        ]);
        let snap = make_snapshot(Some((100, 40)), Some((300, 80)),
            &previous, &current, 1288490189);
        assert_eq!((snap.rows[0].mem.as_str(), snap.rows[0].cpu.as_str()), ("1.2g", "80%"));
        assert_eq!((snap.rows[1].name.as_str(), snap.rows[1].mem.as_str(), snap.rows[1].cpu.as_str()),
            ("bravo", "80.0m", "15%"));
        assert_eq!((snap.rows[2].mem.as_str(), snap.rows[2].cpu.as_str()), ("3.0m", "1%"));
    }
    #[test]
    fn cpu_uses_full_machine_denominator_on_32_threads() {
        // A 2-second interval on 32 logical cores at HZ=100 has about
        // 6400 total ticks. A process spending 32 ticks gets ~0.5%, while
        // 64 non-idle aggregate ticks means the HOST is about 1% busy.
        let prev = HashMap::from([(7, Process {
            name: "worker".into(), ticks: 100, start: 1, rss: 3 * 1048576,
        })]);
        let cur = HashMap::from([(7, Process {
            name: "worker".into(), ticks: 132, start: 1, rss: 3 * 1048576,
        })]);
        let snap = make_snapshot(Some((1000, 900)), Some((7400, 7236)),
            &prev, &cur, 2 * 1048576 * 1024);
        assert_eq!(snap.rows[0].cpu, "1%");
        assert_eq!(snap.rows[1].cpu, "0.5%");
    }

    #[test]
    fn cpu_normalization_adapts_to_8_threads() {
        // Same process work becomes 2% when total machine capacity is 8
        // threads instead of 32. No hardcoded core count or timing assumption.
        let prev = HashMap::from([(7, Process {
            name: "worker".into(), ticks: 100, start: 1, rss: 3 * 1048576,
        })]);
        let cur = HashMap::from([(7, Process {
            name: "worker".into(), ticks: 132, start: 1, rss: 3 * 1048576,
        })]);
        let snap = make_snapshot(Some((1000, 900)), Some((2600, 2436)),
            &prev, &cur, 2 * 1048576 * 1024);
        assert_eq!(snap.rows[0].cpu, "4%");
        assert_eq!(snap.rows[1].cpu, "2%");
    }

    #[test]
    fn invalid_aggregate_readings_never_display_as_zero_percent() {
        let prev = HashMap::from([(7, Process {
            name: "worker".into(), ticks: 100, start: 1, rss: 3 * 1048576,
        })]);
        let cur = HashMap::from([(7, Process {
            name: "worker".into(), ticks: 132, start: 1, rss: 3 * 1048576,
        })]);
        let baseline = Some((1000, 700));
        for (prior, now) in [
            (None, Some((1100, 750))),    // missing baseline
            (baseline, None),              // missing/malformed current sample
            (baseline, baseline),          // stopped total ticks
            (baseline, Some((999, 700))), // total ticks moved backwards
            (baseline, Some((1100, 699))),// idle ticks moved backwards
            (baseline, Some((1100, 801))),// idle delta exceeds total delta
            (Some((1000, 1001)), Some((1100, 1050))), // corrupt prior sample
            (baseline, Some((1100, 1101))), // corrupt current sample
        ] {
            let snapshot = make_snapshot(prior, now, &prev, &cur, 123);
            assert_eq!(snapshot.rows[0].cpu, "--", "host: {prior:?} -> {now:?}");
            assert_eq!(snapshot.rows[1].cpu, "--", "process: {prior:?} -> {now:?}");
            assert_eq!(snapshot.rows[1].mem, "3.0m"); // CPU validity does not erase RAM
        }
    }

    #[test]
    fn measured_zero_stays_zero_and_valid_aggregate_recovers() {
        let prev = HashMap::from([(7, Process {
            name: "worker".into(), ticks: 100, start: 1, rss: 3 * 1048576,
        })]);
        let idle = make_snapshot(Some((1000, 700)), Some((1100, 800)),
            &prev, &prev, 123);
        assert_eq!(idle.rows[0].cpu, "0%");
        assert_eq!(idle.rows[1].cpu, "0%");
        let invalid = make_snapshot(Some((1100, 800)), None, &prev, &prev, 123);
        assert_eq!(invalid.rows[0].cpu, "--");
        assert_eq!(invalid.rows[1].cpu, "--");
        let recovered = make_snapshot(Some((1200, 850)), Some((1300, 900)),
            &prev, &prev, 123);
        assert_eq!(recovered.rows[0].cpu, "50%");
        assert_eq!(recovered.rows[1].cpu, "0%");
    }

    #[test]
    fn latest_snapshot_replaces_older_snapshot() {
        let monitor = Monitor { latest: Arc::new(Mutex::new(None)) };
        for text in ["old", "new"] {
            *monitor.latest.lock().unwrap() = Some(Snapshot {dies:Vec::new(),system:SystemStats::default(),rows: vec![Row {
                name: text.into(), mem: "3m".into(), cpu: "1%".into(),
            }]});
        }
        assert_eq!(monitor.take().unwrap().rows[0].name, "new");
        assert!(monitor.take().is_none());
    }
    #[test]
    fn test_32_cores_group_by_physical_package_and_show_individual_load(){
        let before:Vec<_>=(0..32).map(|n|(n,1000,950)).collect();
        let after:Vec<_>=(0..32).map(|n|(n,1100,if n==0 {950} else {1050})).collect();
        let packages=(0..32).map(|n|(n,if n<16{0}else{1})).collect();
        let result=calculate_dies(&before,&after,&packages,&HashMap::new());
        assert_eq!(result.len(),2);
        assert_eq!((result[0].cores.len(),result[1].cores.len()),(16,16));
        assert_eq!(result[0].cores[0],CoreReading{id:0,state:CoreState::Valid(CpuBand::Red)});
        assert_eq!(result[1].cores[0],CoreReading{id:16,state:CoreState::Valid(CpuBand::Idle)});
        assert!(result.iter().all(|die|die.temp_c.is_none()));
    }
    #[test]
    fn cpu_busy_thresholds_are_exact_and_nonzero_never_rounds_green() {
        let prior=Some((10_000,9_000));
        // dt=100; exactly 0 -> green, <25 -> yellow, exactly 25 ->
        // orange, exactly 75 -> red. No floating point or rounding.
        for (busy,want) in [(0,CpuBand::Idle),(1,CpuBand::Yellow),
            (24,CpuBand::Yellow),(25,CpuBand::Orange),(74,CpuBand::Orange),
            (75,CpuBand::Red),(100,CpuBand::Red)] {
            assert_eq!(classify_cpu(prior,Some((10_100,9_100-busy))),
                CoreState::Valid(want),"{busy}%");
        }
        // Measured 0.01% must also be yellow, not rounded to zero.
        assert_eq!(classify_cpu(prior,Some((20_000,18_999))),
            CoreState::Valid(CpuBand::Yellow));
        let almost_max=u64::MAX-1000;
        assert_eq!(classify_cpu(Some((almost_max,almost_max-200)),
            Some((almost_max+100,almost_max-126))),CoreState::Valid(CpuBand::Orange));
    }
    #[test]
    fn cpu_bad_samples_are_invalid_not_zero_activity() {
        let a=Some((1000,700));
        for (prev,current) in [
            (None,Some((1100,800))), // no baseline
            (a,None),                // missing/malformed read
            (a,Some((1000,700))),   // stopped counter
            (a,Some((999,700))),    // total moved backward
            (a,Some((1100,699))),   // idle moved backward
            (a,Some((1100,801))),   // idle grew more than total
            (Some((1000,1001)),Some((1100,1050))), // corrupt previous
            (a,Some((1100,1101))),  // corrupt current
        ] {assert_eq!(classify_cpu(prev,current),CoreState::Invalid);}
    }
    #[test]
    fn valid_counters_can_be_idle_forever_and_recover_after_failure() {
        let mut prior=Some((1000,700));
        for i in 1..=30 {
            let next=Some((1000+100*i,700+100*i));
            assert_eq!(classify_cpu(prior,next),CoreState::Valid(CpuBand::Idle));
            prior=next;
        }
        assert_eq!(classify_cpu(prior,None),CoreState::Invalid);
        // An unavailable interval cannot be computed; after two successful
        // reads the original tile recovers without restarting the process.
        let resumed=Some((4100,3800));
        assert_eq!(classify_cpu(None,resumed),CoreState::Invalid);
        assert_eq!(classify_cpu(resumed,Some((4200,3850))),
            CoreState::Valid(CpuBand::Orange));
        assert_eq!(classify_cpu(Some((4200,3850)),Some((4300,3950))),
            CoreState::Valid(CpuBand::Idle));
    }
    #[test]
    fn missing_cpu_line_stays_at_its_logical_slot_and_can_recover() {
        let topology=HashMap::from([(0,0),(1,0),(2,0),(17,1)]);
        let before=vec![(0,1000,900),(1,1000,900),(2,1000,900),(17,1000,900)];
        let partial=vec![(0,1100,1000),(2,1100,950),(17,1100,950)];
        let dies=calculate_dies(&before,&partial,&topology,&HashMap::new());
        assert_eq!(dies[0].cores.iter().map(|core|core.id).collect::<Vec<_>>(),vec![0,1,2]);
        assert_eq!(dies[0].cores[0].state,CoreState::Valid(CpuBand::Idle));
        assert_eq!(dies[0].cores[1].state,CoreState::Invalid);
        assert_eq!(dies[0].cores[2].state,CoreState::Valid(CpuBand::Orange));
        assert_eq!(dies[1].cores[0].id,17);
        let after=vec![(0,1200,1100),(1,1200,1000),(2,1200,1000),(17,1200,1000)];
        let first=calculate_dies(&partial,&after,&topology,&HashMap::new());
        assert_eq!(first[0].cores[1].state,CoreState::Invalid);
        let recovered=calculate_dies(&after,
            &[(0,1300,1200),(1,1300,1050),(2,1300,1050),(17,1300,1050)],
            &topology,&HashMap::new());
        assert_eq!(recovered[0].cores[1].state,CoreState::Valid(CpuBand::Orange));
        let failed=calculate_dies(&after,&[],&topology,&HashMap::new());
        assert!(failed.iter().flat_map(|die|&die.cores)
            .all(|core|core.state==CoreState::Invalid));
        assert!(calculate_dies(&before,&before,&HashMap::new(),&HashMap::new()).is_empty());
    }
    #[test]
    fn parses_actual_network_byte_columns() {
        let s="Inter-| Receive | Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n  enp1s0: 123456 42 0 0 0 0 0 0 789012 52 0 0 0 0 0 0\n      lo: 500 5 0 0 0 0 0 0 500 5 0 0 0 0 0 0\n";
        assert_eq!(parse_net_bytes(s,"enp1s0"),Some((123456,789012)));
        assert_eq!(parse_net_bytes(s,"missing"),None);
    }
    #[test]
    fn filesystem_and_mem_accounting_are_bounded() {
        let mem=read_mem_stats();
        assert!(mem.total>=mem.used);
        let disk=read_local_disks();
        assert!(disk.total>=disk.used);
    }
    #[test]
    fn combined_disk_sums_root_home_var_and_counts_bind_mounts_once() {
        let mounts=parse_disk_mounts(b"20 1 8:1 / / rw - ext4 /dev/sda1 rw\n\
21 20 8:2 / /home rw shared:1 - ext4 /dev/sda2 rw\n\
22 20 8:3 / /var rw - xfs /dev/sda3 rw\n\
23 20 8:1 /srv /mnt/bind rw - ext4 /dev/root rw\n");
        assert_eq!(mounts.len(),4);
        let mut queried=Vec::new();
        let result=sum_disk_mounts(&mounts,|m| {
            queried.push(m.device);
            Some(match m.device.1 {
                1=>Meter{used:20,total:100},
                2=>Meter{used:80,total:400},
                3=>Meter{used:50,total:500},
                _=>panic!("unexpected disk"),
            })
        });
        queried.sort_unstable();
        assert_eq!(queried,vec![(8,1),(8,2),(8,3)]);
        assert_eq!((result.used,result.total),(150,1000)); // 15%, not the mean of percentages
        let single=sum_disk_mounts(&mounts[..1],|_|Some(Meter{used:20,total:100}));
        assert_eq!((single.used,single.total),(20,100)); // ordinary /home directories add no mount
    }
    #[test]
    fn combined_disk_excludes_virtual_remote_and_loop_mounts() {
        let mounts=parse_disk_mounts(b"1 0 8:1 / / rw - ext4 /dev/sda1 rw\n\
2 1 0:1 / /run rw - tmpfs tmpfs rw\n\
3 1 0:2 / /dev rw - devtmpfs devtmpfs rw\n\
4 1 0:3 / /proc rw - proc proc rw\n\
5 1 0:4 / /sys rw - sysfs sysfs rw\n\
6 1 0:5 / /mnt/nfs rw - nfs4 host:/export rw\n\
7 1 0:6 / /mnt/smb rw - cifs //host/share rw\n\
8 1 0:7 / /mnt/ssh rw - fuse.sshfs user@host:/ rw\n\
9 1 0:8 / /merged rw - overlay overlay rw\n\
10 1 7:0 / /snap ro - squashfs /dev/loop0 ro\n\
11 1 7:1 / /image rw - ext4 /dev/loop1 rw\n\
12 1 0:10 / /btrfs-image rw - btrfs /dev/loop2 rw\n\
13 1 252:0 / /ram rw - ext4 /dev/zram0 rw\n\
14 1 1:0 / /ramdisk rw - ext2 /dev/ram0 rw\n\
15 1 0:11 / /pool rw - zfs pool/dataset rw\n");
        assert_eq!(mounts.len(),1);
        assert_eq!(mounts[0].path,PathBuf::from("/"));
    }
    #[test]
    fn combined_disk_preserves_escaped_and_non_utf8_paths() {
        use std::os::unix::ffi::OsStrExt;
        let mounts=parse_disk_mounts(b"20 1 8:4 / /mnt/with\\040space\\134slash rw master:2 - ext4 /dev/sdb1 rw\n\
21 1 8:5 / /mnt/byte-\xff rw - ext4 /dev/sdb2 rw\n");
        assert_eq!(mounts.len(),2);
        assert_eq!(mounts[0].path.as_os_str().as_bytes(),b"/mnt/with space\\slash");
        assert_eq!(mounts[1].path.as_os_str().as_bytes(),b"/mnt/byte-\xff");
        for bad in [b"/bad\\04".as_slice(),b"/bad\\999",b"/bad\\000",b"/bad\\777"] {
            assert!(decode_mount_path(bad).is_none());
        }
        assert!(parse_disk_mounts(b"broken\n1 2 bad / / rw - ext4 /dev/sda1 rw\n").is_empty());
    }
    #[test]
    fn combined_disk_btrfs_subvolumes_share_one_capacity() {
        let mounts=parse_disk_mounts(b"1 0 0:30 /@ / rw - btrfs /dev/nvme0n1p2 rw\n\
2 1 0:31 /@home /home rw - btrfs /dev/nvme0n1p2 rw\n\
3 1 0:32 /@var /var rw - btrfs /dev/nvme0n1p2 rw\n");
        assert_eq!(mounts.len(),3);
        let mut calls=0;
        let result=sum_disk_mounts(&mounts,|_|{calls+=1;Some(Meter{used:100,total:500})});
        assert_eq!(calls,1);
        assert_eq!((result.used,result.total),(100,500));
    }
    #[test]
    fn combined_disk_retries_aliases_but_never_reports_partial_or_overflow_totals() {
        let mounts=parse_disk_mounts(b"1 0 8:1 / /gone rw - ext4 /dev/sda1 rw\n\
2 1 8:1 / /available rw - ext4 /dev/sda1 rw\n\
3 1 8:2 / /home rw - ext4 /dev/sda2 rw\n");
        let result=sum_disk_mounts(&mounts,|m|if m.path==std::path::Path::new("/gone") {
            None
        } else {Some(Meter{used:10,total:100})});
        assert_eq!((result.used,result.total),(20,200));
        let failed=sum_disk_mounts(&mounts,|m|if m.device==(8,2) {None}
            else {Some(Meter{used:10,total:100})});
        assert_eq!((failed.used,failed.total),(0,0));
        let overflow=sum_disk_mounts(&mounts,|_|Some(Meter{used:1,total:u64::MAX}));
        assert_eq!((overflow.used,overflow.total),(0,0));
        let invalid=sum_disk_mounts(&mounts,|_|Some(Meter{used:101,total:100}));
        assert_eq!((invalid.used,invalid.total),(0,0));
        assert_eq!(sum_disk_mounts(&[],|_|panic!("no mount to query")).total,0);
    }
    #[test]
    fn combined_disk_rejects_stale_mount_device_identity() {
        let mount=DiskMount{identity:DiskIdentity::Device(u32::MAX,u32::MAX),
            device:(u32::MAX,u32::MAX),path:PathBuf::from("/")};
        assert!(read_disk_mount(&mount).is_none());
    }
    #[test]
    fn parses_own_linux_stat() {
        let s = fs::read_to_string(format!("/proc/{}/stat", std::process::id())).unwrap();
        let p = parse_process(&s, 4096).unwrap();
        assert!(p.start > 0);
    }
}
