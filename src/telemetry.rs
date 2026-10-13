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
}
#[derive(Clone, Debug, Default)]
pub struct Die {
    pub package: u32,
    pub temp_c: Option<i32>, // Package sensor, not a guessed/averaged core temp.
    pub cores: Vec<u8>,       // Individual logical CPU utilization, 0..100.
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
        let packages = detect_packages(&previous_system.cores);
        let temperature_paths = find_temperature_paths();
        let mut previous_processes = read_processes();
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
            snapshot.system.disk = read_root_disk();
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
        let total=values.iter().sum();
        let idle=values[3]+values[4];
        if label=="cpu" {result.aggregate=Some((total,idle));}
        else if let Some(id)=label.strip_prefix("cpu").and_then(|s|s.parse::<u32>().ok()) {
            result.cores.push((id,total,idle));
        }
    }
    result
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
fn calculate_dies(previous:&[(u32,u64,u64)],current:&[(u32,u64,u64)],
    packages:&HashMap<u32,u32>,temperatures:&HashMap<u32,PathBuf>)->Vec<Die>{
    let old:HashMap<u32,(u64,u64)>=previous.iter().map(|&(id,t,i)|(id,(t,i))).collect();
    let mut grouped:BTreeMap<u32,Vec<u8>>=BTreeMap::new();
    for &(id,total,idle) in current {
        let pct=old.get(&id).and_then(|&(pt,pi)|{
            let dt=total.saturating_sub(pt);
            if dt==0 {return None;}
            let di=idle.saturating_sub(pi).min(dt);
            Some((((dt-di) as f64/dt as f64)*100.0).round().clamp(0.0,100.0) as u8)
        }).unwrap_or(0);
        grouped.entry(*packages.get(&id).unwrap_or(&0)).or_default().push(pct);
    }
    grouped.into_iter().map(|(package,cores)|{
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

/// Root filesystem occupancy. statvfs respects the actual mounted filesystem
/// rather than scanning files, and needs no dependency on btop or shell tools.
#[cfg(target_os = "linux")]
fn read_root_disk() -> Meter {
    use std::mem::MaybeUninit;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    let ok = unsafe { libc::statvfs(b"/\0".as_ptr().cast(),stat.as_mut_ptr()) };
    if ok!=0 {return Meter::default();}
    let stat = unsafe {stat.assume_init()};
    let unit = stat.f_frsize as u64;
    Meter {
        used: (stat.f_blocks as u64).saturating_sub(stat.f_bfree as u64).saturating_mul(unit),
        total: (stat.f_blocks as u64).saturating_mul(unit),
    }
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
    let (host_cpu, total_ticks) = match (prev_cpu, cpu) {
        (Some((pt, pi)), Some((ct, ci))) if ct > pt => {
            let dt = ct - pt;
            let idle = ci.saturating_sub(pi).min(dt);
            (((dt - idle) as f64 / dt as f64) * 100.0, dt)
        },
        _ => (0.0, 0),
    };
    let mut ranked: Vec<(&str, u64, u64)> = current.iter().filter_map(|(pid, p)| {
        let before = previous.get(pid)?;
        if before.start != p.start { return None; }
        Some((p.name.as_str(), p.ticks.saturating_sub(before.ticks), p.rss))
    }).collect();
    // Highest CPU first. Name tie-break avoids gratuitous reshuffles at 0%.
    ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let mut rows = Vec::with_capacity(BILLBOARD_ROWS);
    rows.push(Row { name: "bertha".into(), mem: format_mem(used), cpu: format_cpu(host_cpu) });
    for (name, ticks, rss) in ranked.into_iter().take(BILLBOARD_ROWS - 1) {
        rows.push(Row {
            name: name.into(), mem: format_mem(rss),
            cpu: format_cpu(if total_ticks > 0 {
                (ticks as f64 / total_ticks as f64) * 100.0
            } else { 0.0 }),
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
    fn zero_total_ticks_is_safe() {
        let prev = HashMap::from([(7, Process {
            name: "worker".into(), ticks: 100, start: 1, rss: 3 * 1048576,
        })]);
        let cur = HashMap::from([(7, Process {
            name: "worker".into(), ticks: 132, start: 1, rss: 3 * 1048576,
        })]);
        let snap = make_snapshot(Some((100, 99)), Some((100, 99)), &prev, &cur, 123);
        assert_eq!(snap.rows[0].cpu, "0%");
        assert_eq!(snap.rows[1].cpu, "0%");
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
        assert_eq!(result[0].cores[0],100);
        assert_eq!(result[1].cores[0],0);
        assert!(result.iter().all(|die|die.temp_c.is_none()));
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
        let disk=read_root_disk();
        assert!(disk.total>=disk.used);
    }
    #[test]
    fn parses_own_linux_stat() {
        let s = fs::read_to_string(format!("/proc/{}/stat", std::process::id())).unwrap();
        let p = parse_process(&s, 4096).unwrap();
        assert!(p.start > 0);
    }
}
