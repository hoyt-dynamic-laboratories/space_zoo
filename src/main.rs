mod animal_viewer;
mod living_animals;
mod animations;
mod deer;
mod bats;
mod mythic;
mod debug_probe;
mod billboard;
mod telemetry;
mod background;
mod birds;
mod interactions;
mod keyboard;
mod pixel_birds;
mod pixel_dog;
mod circadian;
mod coop;
mod console;
#[cfg(target_os = "linux")]
mod fbdev;
mod flock;
mod pixel_scenery;
mod curiosity;
mod habitat;
mod routines;
mod staging;
mod shepherd;
mod sky;
mod constellations;
mod lunar;
mod solar;
mod events;
mod input;
mod npc;
mod pixel;
mod pixel_sprite;
mod render;
mod sprite;
mod world;

use std::io::{self};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::{
    cursor::{Hide, Show},
    event::{Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Color, ResetColor, SetBackgroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};
use input::{Action, InputController};

// Debug mode intercepts anchor and arrow keys BEFORE ordinary animal antics.
// The probe is never advanced by NPC AI, so it stays where the tester places it.
fn apply_key(world:&mut World,input:&mut InputController,key:KeyEvent)->bool {
    if world.debug_probe.is_some() {
        if key.code==KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return false;
        }
        // Release messages must not trigger extra snaps or movement.
        if key.kind==crossterm::event::KeyEventKind::Release {return true;}
        let dest=if !key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {KeyCode::Char(ch)=>debug_probe::key_anchor(ch),_=>None}
        } else {None};
        if let Some(anchor)=dest {world.debug_snap(anchor);return true;}
        match key.code {
            KeyCode::Left=>world.debug_nudge(-1,0),
            KeyCode::Right=>world.debug_nudge(1,0),
            KeyCode::Up=>world.debug_nudge(0,-1),
            KeyCode::Down=>world.debug_nudge(0,1),
            _=>{}, // no standard animal antics while inspecting geometry
        }
        return true;
    }
    match input.handle(key,world.tick) {
        Some(action)=>apply(world,action),
        None=>true,
    }
}

use render::Renderer;
use keyboard::Keyboard;
use world::{DisplayMode, World, FPS};

struct TerminalGuard;
impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        if let Err(err) = execute!(io::stdout(), EnterAlternateScreen, Hide,
            SetBackgroundColor(Color::Black), Clear(ClearType::All)) {
            let _ = terminal::disable_raw_mode();
            return Err(err);
        }
        Ok(Self)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), ResetColor, Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

fn apply(world: &mut World, action: Action) -> bool {
    match action {
        Action::Quit => return false,
        Action::Helicopter => world.helicopter(),
        Action::Dog(x, y) => world.steer_dog(x, y),
        Action::Greet => world.greet(),
        Action::Space => world.space(),
        Action::ReleaseChickens => {world.release_chickens();},
        Action::Notice => world.notice(),
        Action::Chupacabra => {world.summon_mythic(mythic::MythKind::Chupacabra);},
        Action::Sasquatch => {world.summon_mythic(mythic::MythKind::Sasquatch);},
    }
    true
}

fn seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64
}

fn play_ascii(hour: Option<u8>) -> Result<(), Box<dyn std::error::Error>> {
    let (w, h) = terminal::size()?;
    if w < 40 || h < 14 {
        eprintln!("Space Zoo ASCII requires a terminal at least 40x14.");
        return Ok(());
    }
    let vt_guard = console::VtGuard::install()?;
    let _guard = TerminalGuard::enter()?;
    let mut world = World::with_hour(w, h, seed(), DisplayMode::Ascii, hour);
    let mut painter = Renderer::new(w, h);
    let mut input = InputController::new();
    let keyboard = Keyboard::new();
    let target = Duration::from_nanos(1_000_000_000 / FPS);
    'game: loop {
        let started = Instant::now();
        if let Some(guard) = &vt_guard { guard.poll(); }
        for _ in 0..32 {
            let Some(next) = keyboard.try_next() else {break;};
            match next {
                Event::Key(key) => {
                    if !apply_key(&mut world,&mut input,key) { break 'game; }
                }
                Event::Resize(w, h) => {
                    world.resize(w, h);
                    painter.resize(w, h);
                }
                _ => {}
            }
        }
        world.advance();
        painter.draw(&world)?;
        if let Some(remaining) = target.checked_sub(started.elapsed()) { thread::sleep(remaining); }
    }
    Ok(())
}

// Linux framebuffer backend is direct console output, not a window.
// Same World, same PixelRenderer, same input and all existing behavior.
#[cfg(target_os = "linux")]
fn play_console_pixels(hour: Option<u8>, debug:bool, constellation:bool) -> Result<(), Box<dyn std::error::Error>> {
    struct RawMode;
    impl Drop for RawMode {
        fn drop(&mut self) { let _ = terminal::disable_raw_mode(); }
    }
    // VT_PROCESS prevents switching away from the live console, without root.
    let vt = console::VtGuard::install()?.ok_or(
        "--console requires a physical Linux virtual console, not SSH or a graphical terminal")?;
    terminal::enable_raw_mode()?;
    let _raw = RawMode;
    let fb_path = std::env::var("FRAMEBUFFER").unwrap_or_else(|_| "/dev/fb0".into());
    let mut framebuffer = fbdev::Framebuffer::open(&fb_path)?;
    let mut world = World::with_hour(pixel::LOGICAL_W as u16,
        pixel::LOGICAL_H as u16, seed(), DisplayMode::Pixel, hour);
    if debug {world.enable_debug();}
    let mut painter = pixel::PixelRenderer::new();
    painter.set_constellation_inspection(constellation);
    let monitor = telemetry::spawn();
    let mut input = InputController::new();
    let keyboard = Keyboard::new();
    let target = Duration::from_nanos(1_000_000_000 / FPS);
    'game: loop {
        let started = Instant::now();
        vt.poll();
        for _ in 0..32 {
            let Some(event) = keyboard.try_next() else {break;};
            if let Event::Key(key) = event {
                if !apply_key(&mut world,&mut input,key) { break 'game; }
            }
        }
        world.advance();
        // FB0 often traverses a slow server-BMC display bus. Draw at 15 FPS,
        // but keep the 30Hz simulation cadence. No busy-wait or graphics stack.
        if world.tick % 2 == 0 {
            if let Some(snapshot) = monitor.take() { painter.update_billboard(snapshot); }
            let (pixels,dirty)=painter.draw_with_damage(&world);
            framebuffer.draw_damage(pixels,pixel::WIDTH,pixel::HEIGHT,Some(dirty));
        }
        if let Some(wait) = target.checked_sub(started.elapsed()) { thread::sleep(wait); }
    }
    // Drop fbdev to KD_TEXT, then restore raw mode, then drop vt guard.
    drop(framebuffer);
    drop(_raw);
    drop(vt);
    use std::io::Write;
    let _ = io::stdout().write_all(b"\x1b[2J\x1b[H");
    let _ = io::stdout().flush();
    Ok(())
}

fn key_to_event(key: Key, control: bool) -> Option<KeyEvent> {
    let code = match key {
        Key::Escape => KeyCode::Esc,
        Key::Space => KeyCode::Char(' '),
        Key::Enter => KeyCode::Enter,
        Key::Left => KeyCode::Left,
        Key::Right => KeyCode::Right,
        Key::Up => KeyCode::Up,
        Key::Down => KeyCode::Down,
        Key::A=>KeyCode::Char('a'), Key::S=>KeyCode::Char('s'),
        Key::D=>KeyCode::Char('d'), Key::F=>KeyCode::Char('f'),
        Key::G=>KeyCode::Char('g'), Key::Z=>KeyCode::Char('z'),
        Key::X=>KeyCode::Char('x'), Key::C=>KeyCode::Char('c'),
        Key::V=>KeyCode::Char('v'), Key::B=>KeyCode::Char('b'),
        Key::N=>KeyCode::Char('n'), Key::M=>KeyCode::Char('m'),
        Key::Key1=>KeyCode::Char('1'), Key::Key2=>KeyCode::Char('2'),
        Key::H=>KeyCode::Char('h'), Key::J=>KeyCode::Char('j'),
        Key::R=>KeyCode::Char('r'), Key::T=>KeyCode::Char('t'),
        Key::Y=>KeyCode::Char('y'), Key::Q=>KeyCode::Char('q'),
        Key::LeftCtrl | Key::RightCtrl | Key::LeftShift | Key::RightShift => return None,
        _ => KeyCode::Char('!'),
    };
    let modifiers = if control { KeyModifiers::CONTROL } else { KeyModifiers::NONE };
    Some(KeyEvent::new(code, modifiers))
}

fn play_pixels(hour: Option<u8>,debug:bool,constellation:bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut window = Window::new("SPACE ZOO - Ctrl+C to leave",
        pixel::WIDTH, pixel::HEIGHT, WindowOptions {
            resize: false, scale: Scale::X1, ..WindowOptions::default()
        })?;
    let mut world = World::with_hour(pixel::LOGICAL_W as u16, pixel::LOGICAL_H as u16,
        seed(), DisplayMode::Pixel, hour);
    if debug {world.enable_debug();}
    let mut painter = pixel::PixelRenderer::new();
    painter.set_constellation_inspection(constellation);
    #[cfg(target_os = "linux")]
    let monitor = telemetry::spawn();
    let mut input = InputController::new();
    let mut last_debug_title:Option<String>=None;
    let target = Duration::from_nanos(1_000_000_000 / FPS);
    while window.is_open() {
        let started = Instant::now();
        let control = window.is_key_down(Key::LeftCtrl) || window.is_key_down(Key::RightCtrl);
        // Arrows are level-triggered so dog steering feels immediate.
        for key in [Key::Left, Key::Right, Key::Up, Key::Down] {
            if window.is_key_down(key) {
                if !apply_key(&mut world,&mut input,
                    KeyEvent::new(match key {
                        Key::Left => KeyCode::Left,
                        Key::Right => KeyCode::Right,
                        Key::Up => KeyCode::Up,
                        _ => KeyCode::Down,
                    }, KeyModifiers::NONE)) {return Ok(());}
            }
        }
        for key in window.get_keys_pressed(KeyRepeat::Yes).into_iter().take(32) {
            if matches!(key, Key::Left | Key::Right | Key::Up | Key::Down) { continue; }
            if let Some(event) = key_to_event(key, control) {
                if !apply_key(&mut world,&mut input,event) {return Ok(());}
            }
        }
        world.advance();
        if let Some(probe)=world.debug_probe.as_ref() {
            let title=format!("SPACE ZOO - {} | Ctrl+C to leave",probe.status());
            // Set only when the text changes; no per-frame window-title update.
            if last_debug_title.as_deref()!=Some(title.as_str()) {
                window.set_title(&title);
                last_debug_title=Some(title);
            }
        }
        #[cfg(target_os = "linux")]
        if let Some(snapshot) = monitor.take() { painter.update_billboard(snapshot); }
        window.update_with_buffer(painter.draw(&world), pixel::WIDTH, pixel::HEIGHT)?;
        if let Some(remaining) = target.checked_sub(started.elapsed()) { thread::sleep(remaining); }
    }
    Ok(())
}


// Headless A/B proof runner. This is invoked only with --bench-frames;
// the normal 30 Hz / 15 FPS game loop is unchanged.
fn benchmark(frames:u64,daylight:bool) {
    use telemetry::{Row,Snapshot};
    let mut world=World::with_hour(pixel::LOGICAL_W as u16,pixel::LOGICAL_H as u16,
        20261010,DisplayMode::Pixel,Some(if daylight {14} else {22}));
    let mut painter=pixel::PixelRenderer::new();
    painter.update_billboard(Snapshot {system:telemetry::SystemStats::default(),dies:vec![
        telemetry::Die{package:0,temp_c:Some(39),cores:vec![4;16]},
        telemetry::Die{package:1,temp_c:Some(41),cores:vec![35;16]},
    ],rows:vec![
        Row{name:"bertha".into(),mem:"1.4g".into(),cpu:"1%".into()},
        Row{name:"systemd".into(),mem:"12m".into(),cpu:"0.9%".into()},
        Row{name:"btop".into(),mem:"4m".into(),cpu:"0.4%".into()},
        Row{name:"worker".into(),mem:"50m".into(),cpu:"0.3%".into()},
        Row{name:"ssh".into(),mem:"7m".into(),cpu:"0.2%".into()},
        Row{name:"zoo".into(),mem:"8m".into(),cpu:"0.1%".into()},
        Row{name:"kworker".into(),mem:"2m".into(),cpu:"0.1%".into()},
        Row{name:"ssh-agent".into(),mem:"1m".into(),cpu:"0%".into()},
    ]});
    let started=Instant::now();
    let mut render_ns:u128=0;
    let mut samples=0u64;
    let mut signature=0u64;
    for n in 0..frames {
        world.advance();
        if world.tick % 2==0 {
            let t=Instant::now();
            // Both revisions render the same native 960x600 output.
            let rgb=painter.draw(&world);
            render_ns+=t.elapsed().as_nanos();
            // Observe generated pixels to defeat dead-code removal.
            let offset=((n as usize*7919)%(pixel::WIDTH*pixel::HEIGHT)).min(rgb.len()-1);
            signature=signature.wrapping_mul(33).wrapping_add(rgb[offset] as u64);
            samples+=1;
        }
    }
    let elapsed=started.elapsed();
    println!("scene={} frames={frames} rendered={samples} canvas={}x{} checksum={signature:016x}",
        if daylight {"day/sun"} else {"night"},
        pixel::WIDTH,pixel::HEIGHT);
    println!("total_ms={:.3} render_ms={:.3} render_ms_per_frame={:.4}",
        elapsed.as_secs_f64()*1000.,render_ns as f64/1e6,
        if samples==0 {0.} else {render_ns as f64/1e6/samples as f64});
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The graphics console hides stderr until graphics mode is restored.
    // Zero normal-frame overhead; if a button sequence triggers a Rust panic,
    // retain the exact failing location for diagnosis on Bertha.
    std::panic::set_hook(Box::new(|info| {
        let detail = format!("Space Zoo panic: {info}\n{:?}\n", std::backtrace::Backtrace::force_capture());
        let _ = std::fs::write("/tmp/space-zoo-panic.log", detail);
    }));
    // Explicit opt-in, no /dev/fb0 or X11 needed, does not affect game modes.
    if matches!(std::env::args().nth(1).as_deref(), Some("--bench-frames" | "--bench-sun")) {
        let daylight=std::env::args().nth(1).as_deref()==Some("--bench-sun");
        let frames=std::env::args().nth(2).ok_or("benchmark needs an integer frame count")?
            .parse::<u64>()?;
        if frames==0 || frames>100_000 {return Err("bench frame count must be 1..100000".into());}
        benchmark(frames,daylight);
        return Ok(());
    }
    let mut mode = DisplayMode::Pixel;
    let mut console_requested = false;
    let mut window_requested = false;
    let mut hour: Option<u8> = None;
    let mut debug = false;
    let mut constellation = false;
    let mut animal_view = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ascii" => mode = DisplayMode::Ascii,
            "-V" | "--version" => {
                println!("space_zoo {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--debug" => debug = true,
            "--animal" => animal_view = true,
            "--constellation" => constellation = true,
            "--pixel" => mode = DisplayMode::Pixel,
            "--console" => { mode = DisplayMode::Pixel; console_requested = true; },
            "--window" => { mode = DisplayMode::Pixel; window_requested = true; },
            "--hour" => {
                let value = args.next().ok_or("--hour needs 0-23")?;
                let parsed: u8 = value.parse()?;
                if parsed > 23 { return Err("--hour must be between 0 and 23".into()); }
                hour = Some(parsed);
            }
            "-h" | "--help" => {
                println!("space_zoo [--pixel | --ascii | --console | --window] [--hour 0..23] [--debug] [--constellation] [--animal]\n\
                    -V, --version: print build version and exit\n\
                    --pixel: window on desktop, framebuffer on physical TTY\n\
                    --console: force framebuffer; --window: force desktop window\n\
                    --animal: desktop sprite viewer; arrows animal, C action, F facing, Space pause\n\
                    --debug: isolated existing chicken for testing pixel-world anchors\n\
                    A/S billboard perches; D/F/G coop roof; Z/X/C/V billboard leg approaches\n\
                    B/N/M/H/J coop ground; R/T/Y/Q natural anchors; arrows nudge\n\
                    Pixel modes: live 2-second Bertha billboard on Linux\n\
                    Normal: arrows steer dog; Enter heron; C daytime hen release; Esc helicopter; Ctrl+C leave\n\
                    At night: 1 Chupacabra; 2 Sasquatch (one EXTRA visit each/night).\n\
                    --hour 22: frozen night sky for pixel/sprite review\n\
                    --constellation: accelerate the already-labeled sky, 30min celestial time/second");
                return Ok(());
            }
            _ => { return Err(format!("Unknown option: {arg}. Try --help").into()); }
        }
    }
    if animal_view {
        if console_requested || mode==DisplayMode::Ascii {return Err("--animal uses a desktop window; use --window --animal".into());}
        return animal_viewer::run();
    }
    if console_requested && window_requested {
        return Err("--console and --window cannot be combined".into());
    }
    if constellation && mode==DisplayMode::Ascii {
        return Err("--constellation requires pixel mode".into());
    }
    // Star inspection is a night-sky-only tool; do not change the normal clock.
    if constellation && hour.is_none() {hour=Some(22);}
    if debug && mode==DisplayMode::Ascii {
        return Err("--debug anchor probe requires pixel mode".into());
    }
    match mode {
        DisplayMode::Ascii => play_ascii(hour),
        DisplayMode::Pixel => {
            #[cfg(target_os = "linux")]
            {
                // An actual virtual console takes priority over inherited
                // DISPLAY/WAYLAND_DISPLAY. PTYs must not access /dev/fb0.
                if console_requested || (!window_requested && console::is_virtual_console()) {
                    return play_console_pixels(hour,debug,constellation);
                }
                if !window_requested {
                    // A remote SSH shell gets the terminal zoo by default,
                    // even when X11 forwarding sets DISPLAY. --window remains explicit.
                    let ssh = std::env::var_os("SSH_CONNECTION").is_some()
                        || std::env::var_os("SSH_TTY").is_some();
                    // The existing desktop backend is X11 (via minifb),
                    // so WAYLAND_DISPLAY alone is not enough to open it.
                    let x11 = std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty());
                    if ssh || !x11 {
                        if debug || constellation {
                            return Err("--debug and --constellation require a physical framebuffer console or an X11 window; use --window on a supported desktop".into());
                        }
                        return play_ascii(hour);
                    }
                }
            }
            play_pixels(hour,debug,constellation)
        },
    }
}
