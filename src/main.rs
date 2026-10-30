mod living_animals;
mod animations;
mod deer;
mod bats;
mod mythic;
mod billboard;
mod telemetry;
mod time_machine;
#[cfg(target_os = "linux")]
mod ff_launcher;
#[cfg(target_os = "linux")]
mod x11_motion;
mod background;
mod star_map;
mod constellation_render;
mod birds;
mod interactions;
mod keyboard;
mod ff_keyboard;
mod panic_log;
mod logging;
mod shutdown;
mod pixel_birds;
mod circadian;
mod coop;
mod console;
#[cfg(target_os = "linux")]
mod fbdev;
mod flock;
mod pixel_scenery;
mod null_island;
mod bipedal;
mod curiosity;
mod habitat;
mod routines;
mod staging;
mod shepherd;
mod sky;
mod astronomy;
mod lunar_astronomy;
mod admin1_geo;
mod display_settings;
mod constellations;
mod lunar;
mod solar;
mod planet_sprite;
mod planet_ephemeris;
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
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::{
    cursor::{Hide, Show},
    event::{Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Color, ResetColor, SetBackgroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};
use input::{Action, InputController};

fn apply_key(world:&mut World,input:&mut InputController,key:KeyEvent,
    time_panel:&mut Option<time_machine::Panel>)->bool {
    if let Some(panel)=time_panel.as_mut() {
        let keep_running=panel.handle(key,&mut world.sky);
        world.refresh_time_machine();
        return keep_running;
    }
    match input.handle(key,world.tick) {
        Some(action)=>apply(world,action),
        None=>true,
    }
}

use render::Renderer;
use keyboard::Keyboard;
use ff_keyboard::{FfKeyboard,FfEvent};
use world::{DisplayMode, World, FPS};
// Observer movement uses exact one-degree taps plus frame-time held motion.
// The held path consumes current key state, never a queue of autorepeat commands.
const OBSERVER_HOLD_SPEED_DEG_PER_SEC:f64=20.0;
const WINDOW_HOLD_DELAY:Duration=Duration::from_millis(120);
const GIMBAL_ACCEL_DEG_PER_SEC2:f64=15.0;
const GIMBAL_MAX_SPEED_DEG_PER_SEC:f64=60.0;
const GIMBAL_MAX_DT_SEC:f64=0.1;
static USER_TERMINATION_REQUESTED:AtomicBool=AtomicBool::new(false);
static USER_QUIT_COMMAND_REQUESTED:AtomicBool=AtomicBool::new(false);

fn log_ctrl_c_request() {
    if !USER_TERMINATION_REQUESTED.swap(true,Ordering::Relaxed) {
        logging::durable_event(logging::Severity::Warn,"TERMINATION_REQUEST","signal=CTRL_C_INPUT");
    }
}

fn log_signal_request() {
    if let Some(signal)=shutdown::signal_name() {logging::termination(signal);}
}

fn log_time_machine_exit(kind:time_machine::QuitKind) {
    match kind {
        time_machine::QuitKind::CtrlC=>log_ctrl_c_request(),
        time_machine::QuitKind::Command=>{
            USER_QUIT_COMMAND_REQUESTED.store(true,Ordering::Relaxed);
            logging::durable_event(logging::Severity::Info,"USER_EXIT_REQUEST","method=time_machine_quit_command");
        }
    }
}

struct CleanupScope(&'static str);
impl Drop for CleanupScope {
    fn drop(&mut self) {logging::cleanup(&format!("{} cleanup beginning",self.0));}
}
struct CleanupCompleteScope(&'static str);
impl Drop for CleanupCompleteScope {
    fn drop(&mut self) {logging::cleanup(&format!("{} cleanup complete",self.0));}
}
struct CleanupStage(&'static str);
impl Drop for CleanupStage {
    fn drop(&mut self) {logging::cleanup(self.0);}
}
fn move_observer_delta(world:&mut World,latitude_delta:f64,longitude_delta:f64)->bool {
    let Some(place)=world.sky.location() else {return false;};
    let lat=(place.latitude+latitude_delta).clamp(-90.0,90.0);
    let lon=(place.longitude+longitude_delta+180.0).rem_euclid(360.0)-180.0;
    if (lat-place.latitude).abs()<1e-12 && (lon-place.longitude).abs()<1e-12 {return false;}
    if let Ok(next)=astronomy::Location::new(lat,lon) {
        world.sky.relocate(next);
        true
    } else {false}
}
fn move_observer_step(world:&mut World,latitude_step:i8,longitude_step:i8)->bool {
    let step=space_zoo::star_projection::CAMERA_STEP_DEG;
    move_observer_delta(world,latitude_step.clamp(-1,1) as f64*step,longitude_step.clamp(-1,1) as f64*step)
}
fn move_observer_delta_navigation(world:&mut World,latitude_delta:f64,longitude_delta:f64)->bool {
    let Some(place)=world.sky.location() else {return false;};
    let lat=(place.latitude+latitude_delta).clamp(-90.0,90.0);
    let lon=(place.longitude+longitude_delta+180.0).rem_euclid(360.0)-180.0;
    if (lat-place.latitude).abs()<1e-12 && (lon-place.longitude).abs()<1e-12 {return false;}
    if let Ok(next)=astronomy::Location::new(lat,lon) {
        world.sky.relocate_navigation(next);
        true
    } else {false}
}

fn shortest_longitude_delta(from:f64,to:f64)->f64 {
    let delta=(to-from+180.0).rem_euclid(360.0)-180.0;
    if (delta+180.0).abs()<1e-12 && to-from>0.0 {180.0} else {delta}
}

fn simulation_direction(rate:f64)->&'static str {if rate>0.0{"forward"}else if rate<0.0{"reverse"}else{"stopped"}}
fn nav_point(place:astronomy::Location)->String {format!("lat={:.6} lon={:.6}",place.latitude,place.longitude)}
fn log_navigation_start(start:astronomy::Location,target:astronomy::Location,rate:f64) {
    logging::event(logging::Severity::Info,"NAVIGATION_START",&format!("start={}\ntarget={}\nsimulation_direction={} simulation_rate={}\ntrajectory=latitude-first then shortest-longitude accel=15deg/s2 cap=60deg/s",
        nav_point(start),nav_point(target),simulation_direction(rate),rate));
    logging::operation("gimbal navigation active");
}
fn log_navigation_arrival(target:astronomy::Location,rate:f64) {
    logging::event(logging::Severity::Info,"NAVIGATION_ARRIVAL",&format!("target={}\nsimulation_direction={} simulation_rate={}",nav_point(target),simulation_direction(rate),rate));
}
fn log_navigation_cancel(target:astronomy::Location,current:Option<astronomy::Location>,rate:f64) {
    let current=current.map(nav_point).unwrap_or_else(||"unknown".into());
    logging::event(logging::Severity::Info,"NAVIGATION_CANCELLATION",&format!("target={}\ncurrent={}\nsimulation_direction={} simulation_rate={}",nav_point(target),current,simulation_direction(rate),rate));
}
fn log_navigation_replacement(old:astronomy::Location,new:astronomy::Location,rate:f64) {
    logging::event(logging::Severity::Info,"NAVIGATION_REPLACEMENT",&format!("replaced_target={}\nnew_target={}\nsimulation_direction={} simulation_rate={}",nav_point(old),nav_point(new),simulation_direction(rate),rate));
}

#[derive(Clone,Copy,Debug)]
struct GimbalAxisProfile {
    distance_deg:f64,
    direction:f64,
    peak_speed_deg_per_sec:f64,
    accel_time_sec:f64,
    cruise_time_sec:f64,
}
impl GimbalAxisProfile {
    fn new(signed_distance_deg:f64)->Self {
        let distance_deg=signed_distance_deg.abs();
        let direction=if signed_distance_deg<0.0 {-1.0}else{1.0};
        if distance_deg==0.0 {
            return Self{distance_deg,direction,peak_speed_deg_per_sec:0.0,accel_time_sec:0.0,cruise_time_sec:0.0};
        }
        let max_accel_distance=GIMBAL_MAX_SPEED_DEG_PER_SEC*GIMBAL_MAX_SPEED_DEG_PER_SEC/
            (2.0*GIMBAL_ACCEL_DEG_PER_SEC2);
        if distance_deg>=2.0*max_accel_distance {
            let accel_time_sec=GIMBAL_MAX_SPEED_DEG_PER_SEC/GIMBAL_ACCEL_DEG_PER_SEC2;
            let cruise_distance=distance_deg-2.0*max_accel_distance;
            Self{distance_deg,direction,peak_speed_deg_per_sec:GIMBAL_MAX_SPEED_DEG_PER_SEC,
                accel_time_sec,cruise_time_sec:cruise_distance/GIMBAL_MAX_SPEED_DEG_PER_SEC}
        } else {
            let peak_speed_deg_per_sec=(GIMBAL_ACCEL_DEG_PER_SEC2*distance_deg).sqrt();
            Self{distance_deg,direction,peak_speed_deg_per_sec,
                accel_time_sec:peak_speed_deg_per_sec/GIMBAL_ACCEL_DEG_PER_SEC2,cruise_time_sec:0.0}
        }
    }
    fn duration_sec(self)->f64 {2.0*self.accel_time_sec+self.cruise_time_sec}
    fn progress_deg(self,elapsed_sec:f64)->f64 {
        if self.distance_deg==0.0 {return 0.0;}
        let t=elapsed_sec.clamp(0.0,self.duration_sec());
        let accel_distance=0.5*GIMBAL_ACCEL_DEG_PER_SEC2*self.accel_time_sec*self.accel_time_sec;
        let progress=if t<=self.accel_time_sec {
            0.5*GIMBAL_ACCEL_DEG_PER_SEC2*t*t
        } else if t<=self.accel_time_sec+self.cruise_time_sec {
            accel_distance+self.peak_speed_deg_per_sec*(t-self.accel_time_sec)
        } else {
            let brake_t=t-self.accel_time_sec-self.cruise_time_sec;
            accel_distance+self.peak_speed_deg_per_sec*self.cruise_time_sec+
                self.peak_speed_deg_per_sec*brake_t-
                0.5*GIMBAL_ACCEL_DEG_PER_SEC2*brake_t*brake_t
        };
        progress.clamp(0.0,self.distance_deg)
    }
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum GimbalPhase {Latitude,Longitude}

#[derive(Clone,Copy,Debug)]
struct GimbalTravel {
    target:astronomy::Location,
    phase:GimbalPhase,
    axis:GimbalAxisProfile,
    axis_elapsed_sec:f64,
    axis_progress_deg:f64,
    center_camera_on_arrival:bool,
    planet_time_rate:Option<f64>,
}

#[derive(Clone,Copy,Debug,PartialEq)]
enum GimbalAdvance {Idle,Moved,Arrived{center_camera:bool}}

#[derive(Default)]
pub(crate) struct GimbalController {
    travel:Option<GimbalTravel>,
    last_render:Option<Instant>,
    arrival_rate_restore:Option<f64>,
}
impl GimbalController {
    fn begin_prepared(&mut self,world:&World,target:astronomy::Location,center_camera_on_arrival:bool,
        planet_time_rate:Option<f64>)->bool {
        let Some(start)=world.sky.location() else{return false;};
        let latitude_distance=target.latitude-start.latitude;
        self.travel=Some(GimbalTravel{target,phase:GimbalPhase::Latitude,
            axis:GimbalAxisProfile::new(latitude_distance),
            axis_elapsed_sec:0.0,axis_progress_deg:0.0,center_camera_on_arrival,planet_time_rate});
        self.last_render=None;
        log_navigation_start(start,target,planet_time_rate.unwrap_or(world.sky.rate()));
        true
    }
    fn cleanup_travel(&mut self,world:&mut World)->bool {
        let travel=self.travel.take();
        let pending_rate=self.arrival_rate_restore.take();
        let active=travel.is_some() || pending_rate.is_some();
        self.last_render=None;
        world.sky.finish_navigation_relocation();
        if let Some(rate)=travel.and_then(|t|t.planet_time_rate).or(pending_rate) {
            world.sky.restore_navigation_rate(rate);
        }
        active
    }
    pub(crate) fn begin(&mut self,world:&mut World,target:astronomy::Location,center_camera_on_arrival:bool)->bool {
        let replaced=self.travel.map(|travel|travel.target);
        self.cleanup_travel(world);
        if let Some(old)=replaced {log_navigation_replacement(old,target,world.sky.rate());}
        self.begin_prepared(world,target,center_camera_on_arrival,None)
    }
    pub(crate) fn begin_planet(&mut self,world:&mut World,kind:crate::sky::PlanetKind)->Result<(),String> {
        // Finish any old trip completely before sampling the new original time rate.
        let replaced=self.travel.map(|travel|travel.target);
        self.cleanup_travel(world);
        let saved_rate=world.sky.begin_navigation_pause()
            .ok_or("planet navigation requires Time Machine")?;
        let utc=world.sky.now_secs();
        let observer=match crate::planet_ephemeris::default_camera_observer(kind,utc) {
            Some(observer)=>observer,
            None=>{
                world.sky.restore_navigation_rate(saved_rate);
                return Err(format!("planet ephemeris unavailable for this date (supported through {})",
                    crate::planet_ephemeris::LAST_SUPPORTED_YEAR));
            }
        };
        if let Some(old)=replaced {log_navigation_replacement(old,observer,saved_rate);}
        if !self.begin_prepared(world,observer,true,Some(saved_rate)) {
            world.sky.restore_navigation_rate(saved_rate);
            return Err("planet navigation requires observer location".into());
        }
        Ok(())
    }
    pub(crate) fn is_active(&self)->bool {self.travel.is_some()}
    pub(crate) fn planet_active(&self)->bool {self.travel.is_some_and(|travel|travel.planet_time_rate.is_some())}
    pub(crate) fn cancel(&mut self,world:&mut World)->bool {
        let prior=self.travel;
        let active=self.cleanup_travel(world);
        if let Some(travel)=prior {
            let current=world.sky.location();
            log_navigation_cancel(travel.target,current,travel.planet_time_rate.unwrap_or(world.sky.rate()));
        }
        active
    }
    fn finish_arrival(&mut self,world:&mut World) {
        if let Some(rate)=self.arrival_rate_restore.take() {world.sky.restore_navigation_rate(rate);}
    }
    fn start_longitude(travel:&mut GimbalTravel,current:f64) {
        travel.phase=GimbalPhase::Longitude;
        travel.axis=GimbalAxisProfile::new(shortest_longitude_delta(current,travel.target.longitude));
        travel.axis_elapsed_sec=0.0;
        travel.axis_progress_deg=0.0;
    }
    fn advance_for_render(&mut self,world:&mut World,now:Instant)->GimbalAdvance {
        if self.travel.is_none() {self.last_render=Some(now);return GimbalAdvance::Idle;}
        let dt=self.last_render.map(|previous|now.saturating_duration_since(previous).as_secs_f64())
            .unwrap_or(0.0).clamp(0.0,GIMBAL_MAX_DT_SEC);
        self.last_render=Some(now);
        let mut travel=self.travel.take().expect("checked above");
        let mut moved=false;
        // Zero-length axes are skipped in the same rendered update; nonzero axes
        // consume this frame's dt and hand off on the following rendered frame.
        for _ in 0..2 {
            travel.axis_elapsed_sec=(travel.axis_elapsed_sec+dt).min(travel.axis.duration_sec());
            let progress=travel.axis.progress_deg(travel.axis_elapsed_sec);
            let delta_progress=(progress-travel.axis_progress_deg).max(0.0);
            travel.axis_progress_deg=progress;
            if delta_progress>0.0 {
                let delta=travel.axis.direction*delta_progress;
                moved|=match travel.phase {
                    GimbalPhase::Latitude=>move_observer_delta_navigation(world,delta,0.0),
                    GimbalPhase::Longitude=>move_observer_delta_navigation(world,0.0,delta),
                };
            }
            if travel.axis_elapsed_sec+1e-12<travel.axis.duration_sec() {
                self.travel=Some(travel);
                return if moved{GimbalAdvance::Moved}else{GimbalAdvance::Idle};
            }
            let Some(current)=world.sky.location() else{self.travel=Some(travel);return GimbalAdvance::Idle;};
            match travel.phase {
                GimbalPhase::Latitude=>{
                    let exact=travel.target.latitude-current.latitude;
                    if exact!=0.0 {moved|=move_observer_delta_navigation(world,exact,0.0);}
                    let current=world.sky.location().expect("location survived latitude settle");
                    Self::start_longitude(&mut travel,current.longitude);
                    if moved {self.travel=Some(travel);return GimbalAdvance::Moved;}
                }
                GimbalPhase::Longitude=>{
                    let exact=shortest_longitude_delta(current.longitude,travel.target.longitude);
                    if exact!=0.0 {let _=move_observer_delta_navigation(world,0.0,exact);}
                    if world.sky.location()!=Some(travel.target) {world.sky.relocate_navigation(travel.target);}
                    world.sky.finish_navigation_relocation();
                    log_navigation_arrival(travel.target,travel.planet_time_rate.unwrap_or(world.sky.rate()));
                    self.arrival_rate_restore=travel.planet_time_rate;
                    self.travel=None;
                    return GimbalAdvance::Arrived{center_camera:travel.center_camera_on_arrival};
                }
            }
        }
        self.travel=Some(travel);
        if moved{GimbalAdvance::Moved}else{GimbalAdvance::Idle}
    }
}

fn advance_gimbal_for_render(gimbal:&mut GimbalController,world:&mut World,painter:&mut pixel::PixelRenderer) {
    let step=gimbal.advance_for_render(world,Instant::now());
    match step {
        GimbalAdvance::Idle=>{},
        GimbalAdvance::Moved=>world.refresh_time_machine(),
        GimbalAdvance::Arrived{center_camera}=>{
            // Planet simulation time remains paused until after the accepted arrival
            // camera reset has executed. Coordinate trips carry no saved time rate.
            if center_camera {painter.view_control(space_zoo::star_projection::ViewControl::Reset);}
            gimbal.finish_arrival(world);
            world.refresh_time_machine();
        }
    }
}

#[derive(Default)]
struct HoldTimer {since:Option<Instant>}
impl HoldTimer {
    fn active(&mut self,down:bool,now:Instant)->bool {
        if !down {self.since=None;return false;}
        let since=*self.since.get_or_insert(now);
        now.saturating_duration_since(since)>=WINDOW_HOLD_DELAY
    }
}
#[derive(Default)]
struct WindowMotion {w:HoldTimer,s:HoldTimer,a:HoldTimer,d:HoldTimer,up:HoldTimer,down:HoldTimer,left:HoldTimer,right:HoldTimer}

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
        if let Err(error)=execute!(io::stdout(),ResetColor,Show,LeaveAlternateScreen) {
            logging::cleanup_failure("terminal.screen_restore",&error.to_string());
        }
        if let Err(error)=terminal::disable_raw_mode() {
            logging::cleanup_failure("terminal.raw_mode_restore",&error.to_string());
        }
    }
}

fn apply(world: &mut World, action: Action) -> bool {
    match action {
        Action::Quit => {log_ctrl_c_request();return false;},
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

fn command_controls_time(command:&time_machine::Command)->bool {
    use time_machine::Command;
    matches!(command,Command::Forward | Command::Backward | Command::Stop | Command::Normal |
        Command::NormalBackward | Command::Rate(_) | Command::ReverseRate(_) |
        Command::Date(_) | Command::TimeOfDay(_))
}

fn cancel_gimbal_for_arrow(painter:&mut pixel::PixelRenderer,world:&mut World,gimbal:&mut GimbalController)->bool {
    if !gimbal.cancel(world) {return false;}
    painter.show_hud_message("GIMBAL CANCELLED",Duration::from_secs(2));
    world.refresh_time_machine();
    true
}

fn cancel_planet_for_time_command(gimbal:&mut GimbalController,world:&mut World,command:&time_machine::Command)->bool {
    if !gimbal.planet_active() || !command_controls_time(command) {return false;}
    gimbal.cancel(world)
}

fn apply_pixel_command(painter:&mut pixel::PixelRenderer,world:&mut World,gimbal:&mut GimbalController,raw:&str)
    ->Result<time_machine::CommandReply,String> {
    use time_machine::{Command,CommandReply};
    logging::operation("Time Machine command processing");
    let command=time_machine::parse_modern(raw)?;
    if let Command::Location(place)=&command {
        let _=gimbal.begin(world,*place,false);
        return Ok(CommandReply::Applied);
    }
    // A user time command supersedes the temporary planet-navigation pause.
    // Cleanup restores the pre-trip rate first; the command below then wins.
    cancel_planet_for_time_command(gimbal,world,&command);
    if matches!(&command,Command::Culture(_) | Command::CultureQuery | Command::Cultures |
        Command::DebugStars(_) | Command::DebugStarsQuery | Command::Planet(_) | Command::ListPlanets |
        Command::Help | Command::StarsQuery | Command::Cryptids(_) | Command::PlanetAppearance(_) |
        Command::Jumbotron(_) | Command::Hud(_) | Command::Chickens(_) | Command::Named(_) | Command::CelestialPlaceholder) {
        painter.visual_command(&command,world,gimbal)
    } else {
        world.sky.command(command)?;
        world.refresh_time_machine();
        Ok(CommandReply::Applied)
    }
}

fn play_ascii(location:astronomy::Location,fast:bool) -> Result<(), Box<dyn std::error::Error>> {
    let _cleanup_complete=CleanupCompleteScope("ASCII terminal backend");
    let (w, h) = terminal::size()?;
    if !render::ascii_dimensions_supported(w, h) {
        eprintln!("Space Zoo ASCII requires a terminal at least {}x{} and at most {} cells (got {}x{}).",
            render::MIN_ASCII_WIDTH, render::MIN_ASCII_HEIGHT, render::MAX_ASCII_CELLS, w, h);
        return Ok(());
    }
    let _cleanup_vt_stage=CleanupStage("ASCII virtual-console guard restored");
    let vt_guard = console::VtGuard::install()?;
    let _cleanup_terminal_stage=CleanupStage("ASCII terminal screen/raw mode restored");
    let _guard = TerminalGuard::enter()?;
    let mut world = World::new(w, h, seed(), DisplayMode::Ascii);
    world.configure_sky(location,fast);
    let mut painter = Renderer::new(w, h);
    let mut input = InputController::new();
    let mut time_panel=fast.then(time_machine::Panel::default);
    let _cleanup_input_stage=CleanupStage("ASCII input workers released");
    let keyboard = (!fast).then(Keyboard::new);
    let ff_keyboard = fast.then(FfKeyboard::new);
    let target = Duration::from_nanos(1_000_000_000 / FPS);
    let mut rendering_paused = false;
    let _cleanup_begin=CleanupScope("ASCII terminal backend");
    'game: loop {
        if shutdown::requested() {log_signal_request();break 'game; }
        let started = Instant::now();
        if let Some(guard) = &vt_guard { guard.poll(); }
        // Time Machine owns a separate input editor thread: characters are captured
        // immediately even when repainting the screen takes longer.
        if let Some(ff) = &ff_keyboard {
            for _ in 0..32 {
                let Some(next) = ff.try_next()? else {break;};
                match next {
                    FfEvent::Command(command) => {
                        // ASCII has no native culture/star overlay; its commands
                        // continue through the existing sky controller.
                        let result=time_machine::parse(&command)
                            .and_then(|parsed|world.sky.command(parsed));
                        if result.is_ok(){world.refresh_time_machine();}
                        ff.reply_legacy(result)?;
                    }
                    FfEvent::Quit(kind)=>{log_time_machine_exit(kind);break 'game;},
                    FfEvent::Resize(w,h)=>{
                        if render::ascii_dimensions_supported(w,h) {
                            world.resize(w,h);painter.resize(w,h);rendering_paused=false;
                        } else if !rendering_paused {
                            rendering_paused=true;
                            execute!(io::stdout(), Clear(ClearType::All),
                                crossterm::cursor::MoveTo(0,0),
                                crossterm::style::Print("Space Zoo paused: unsupported terminal size."))?;
                        }
                    }
                }
            }
            // ASCII has no movable star camera; consume and discard the bounded
            // movement snapshot captured by the shared Time Machine keyboard reader.
            let _ = ff.take_motion();
            let captured=ff.panel()?;
            time_panel=Some(captured);
        } else if let Some(keyboard) = &keyboard {
            for _ in 0..32 {
                let Some(next) = keyboard.try_next()? else {break;};
                match next {
                    Event::Key(key) => {
                        if !apply_key(&mut world,&mut input,key,&mut time_panel) {log_ctrl_c_request();break 'game;}
                    }
                    Event::Resize(w,h)=>{
                        if render::ascii_dimensions_supported(w,h) {
                            world.resize(w,h);painter.resize(w,h);rendering_paused=false;
                        } else if !rendering_paused {
                            rendering_paused=true;
                            execute!(io::stdout(), Clear(ClearType::All),
                                crossterm::cursor::MoveTo(0,0),
                                crossterm::style::Print("Space Zoo paused: unsupported terminal size."))?;
                        }
                    }
                    _=>{}
                }
            }
        }
        world.advance();
        if !rendering_paused { painter.draw_time_machine(&world,time_panel.as_ref())?; }
        if let Some(remaining) = target.checked_sub(started.elapsed()) { thread::sleep(remaining); }
    }
    Ok(())
}

// Linux framebuffer backend is direct console output, not a window.
// Same World, same PixelRenderer, same input and all existing behavior.
#[cfg(target_os = "linux")]
fn play_console_pixels(gm:bool,location:astronomy::Location,fast:bool) -> Result<(), Box<dyn std::error::Error>> {
    let _cleanup_complete=CleanupCompleteScope("framebuffer backend");
    // Load/validate the large production catalogue before taking over the VT or
    // entering raw input mode, so startup I/O cannot make an active console feel hung.
    let mut painter = pixel::PixelRenderer::new();
    if fast { painter.enable_star_map()?; }
    struct RawMode;
    impl Drop for RawMode {
        fn drop(&mut self) {if let Err(error)=terminal::disable_raw_mode(){logging::cleanup_failure("framebuffer.raw_mode_restore",&error.to_string());}}
    }
    // VT_PROCESS prevents switching away from the live console, without root.
    let _cleanup_vt_stage=CleanupStage("framebuffer virtual-console guard restored");
    let vt = console::VtGuard::install()?.ok_or(
        "--console requires a physical Linux virtual console, not SSH or a graphical terminal")?;
    terminal::enable_raw_mode()?;
    let _raw = RawMode;
    let fb_path = std::env::var("FRAMEBUFFER").unwrap_or_else(|_| "/dev/fb0".into());
    let mut framebuffer = fbdev::Framebuffer::open(&fb_path)?;
    let mut world = World::new(pixel::LOGICAL_W as u16,
        pixel::LOGICAL_H as u16, seed(), DisplayMode::Pixel);
    world.configure_sky(location,fast);
    painter.set_melies_moon(gm);
    let monitor = telemetry::spawn();
    let mut input = InputController::new();
    let mut time_panel=fast.then(time_machine::Panel::modern);
    let keyboard = (!fast).then(Keyboard::new);
    let ff_keyboard = fast.then(FfKeyboard::new_modern);
    let mut gimbal=GimbalController::default();
    let target = Duration::from_nanos(1_000_000_000 / FPS);
    let mut last_geo_motion=Instant::now();
    let _cleanup_begin=CleanupScope("framebuffer backend");
    'game: loop {
        if shutdown::requested() {log_signal_request();break 'game; }
        let started = Instant::now();
        vt.poll();
        if let Some(ff) = &ff_keyboard {
            for _ in 0..32 {
                let Some(next)=ff.try_next()? else {break;};
                match next {
                    FfEvent::Command(command)=>{
                        // Physical framebuffer commands use the same pixel
                        // visual-control path as the X11 renderer.
                        let result=apply_pixel_command(&mut painter,&mut world,&mut gimbal,&command);
                        ff.reply(result)?;
                    }
                    FfEvent::Quit(kind)=>{log_time_machine_exit(kind);break 'game;},
                    FfEvent::Resize(..)=>{},
                }
            }
            // Movement is a coalesced snapshot, not an autorepeat log. Taps are
            // bounded discrete steps; a confirmed hold is sampled as current state.
            let motion=ff.take_motion();
            if motion.reset_view {painter.view_control(space_zoo::star_projection::ViewControl::Reset);}
            for _ in 0..motion.view_step_altitude.unsigned_abs() {
                painter.view_control(if motion.view_step_altitude>0 {space_zoo::star_projection::ViewControl::Up} else {space_zoo::star_projection::ViewControl::Down});
            }
            for _ in 0..motion.view_step_azimuth.unsigned_abs() {
                painter.view_control(if motion.view_step_azimuth>0 {space_zoo::star_projection::ViewControl::Right} else {space_zoo::star_projection::ViewControl::Left});
            }
            let _=painter.set_view_input(motion.view_hold);
            let fresh_geo=motion.geo_step_latitude!=0 || motion.geo_step_longitude!=0;
            let cancelled_for_arrow=fresh_geo && gimbal.is_active() &&
                cancel_gimbal_for_arrow(&mut painter,&mut world,&mut gimbal);
            if fresh_geo {
                // The input layer separately records the most recent fresh arrow.
                // On a cancellation frame consume exactly that one press and discard
                // any bounded tap accumulation collected while navigation was active.
                let lat_step=if cancelled_for_arrow {motion.geo_latest_latitude} else {motion.geo_step_latitude};
                let lon_step=if cancelled_for_arrow {motion.geo_latest_longitude} else {motion.geo_step_longitude};
                let _=move_observer_delta(&mut world,lat_step as f64,lon_step as f64);
            }
            let geo_now=Instant::now();
            let geo_dt=geo_now.saturating_duration_since(last_geo_motion).as_secs_f64().clamp(0.0,0.1);
            last_geo_motion=geo_now;
            if !gimbal.is_active() && (motion.geo_hold_latitude!=0 || motion.geo_hold_longitude!=0) {
                let _=move_observer_delta(&mut world,
                    motion.geo_hold_latitude as f64*OBSERVER_HOLD_SPEED_DEG_PER_SEC*geo_dt,
                    motion.geo_hold_longitude as f64*OBSERVER_HOLD_SPEED_DEG_PER_SEC*geo_dt);
            }
            let captured=ff.panel()?;
            // Opening/editing the modern Command Center never pauses simulated time.
            time_panel=Some(captured);
        } else if let Some(keyboard)=&keyboard {
            for _ in 0..32 {
                let Some(event) = keyboard.try_next()? else {break;};
                if let Event::Key(key) = event {
                    if !apply_key(&mut world,&mut input,key,&mut time_panel) {log_ctrl_c_request();break 'game; }
                }
            }
        }
        world.advance();
        // FB0 often traverses a slow server-BMC display bus. Draw at 15 FPS,
        // but keep the 30Hz simulation cadence. No busy-wait or graphics stack.
        if world.tick % 2 == 0 {
            advance_gimbal_for_render(&mut gimbal,&mut world,&mut painter);
            if let Some(snapshot) = monitor.take() { painter.update_billboard(snapshot); }
            if let Some(panel)=time_panel.as_ref() {painter.set_time_panel(panel);}
            let (pixels,dirty)=painter.draw_console_with_damage(&world);
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

fn key_to_event(key: Key, control: bool, shift: bool) -> Option<KeyEvent> {
    let code = match key {
        Key::Escape => KeyCode::Esc,
        Key::Space => KeyCode::Char(' '),
        Key::Enter => KeyCode::Enter,
        Key::Backspace => KeyCode::Backspace,
        Key::Left => KeyCode::Left,
        Key::Right => KeyCode::Right,
        Key::Up => KeyCode::Up,
        Key::Down => KeyCode::Down,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,
        Key::A=>KeyCode::Char('a'),Key::B=>KeyCode::Char('b'),
        Key::C=>KeyCode::Char('c'),Key::D=>KeyCode::Char('d'),
        Key::E=>KeyCode::Char('e'),Key::F=>KeyCode::Char('f'),
        Key::G=>KeyCode::Char('g'),Key::H=>KeyCode::Char('h'),
        Key::I=>KeyCode::Char('i'),Key::J=>KeyCode::Char('j'),
        Key::K=>KeyCode::Char('k'),Key::L=>KeyCode::Char('l'),
        Key::M=>KeyCode::Char('m'),Key::N=>KeyCode::Char('n'),
        Key::O=>KeyCode::Char('o'),Key::P=>KeyCode::Char('p'),
        Key::Q=>KeyCode::Char('q'),Key::R=>KeyCode::Char('r'),
        Key::S=>KeyCode::Char('s'),Key::T=>KeyCode::Char('t'),
        Key::U=>KeyCode::Char('u'),Key::V=>KeyCode::Char('v'),
        Key::W=>KeyCode::Char('w'),Key::X=>KeyCode::Char('x'),
        Key::Y=>KeyCode::Char('y'),Key::Z=>KeyCode::Char('z'),
        Key::Key0=>KeyCode::Char('0'),Key::Key1=>KeyCode::Char(if shift {'!'} else {'1'}),
        Key::Key2=>KeyCode::Char('2'),Key::Key3=>KeyCode::Char('3'),
        Key::Key4=>KeyCode::Char('4'),Key::Key5=>KeyCode::Char('5'),
        Key::Key6=>KeyCode::Char('6'),Key::Key7=>KeyCode::Char('7'),
        Key::Key8=>KeyCode::Char('8'),Key::Key9=>KeyCode::Char('9'),
        Key::NumPad0=>KeyCode::Char('0'),Key::NumPad1=>KeyCode::Char('1'),
        Key::NumPad2=>KeyCode::Char('2'),Key::NumPad3=>KeyCode::Char('3'),
        Key::NumPad4=>KeyCode::Char('4'),Key::NumPad5=>KeyCode::Char('5'),
        Key::NumPad6=>KeyCode::Char('6'),Key::NumPad7=>KeyCode::Char('7'),
        Key::NumPad8=>KeyCode::Char('8'),Key::NumPad9=>KeyCode::Char('9'),
        Key::Period=>KeyCode::Char(if shift {'>'} else {'.'}),
        Key::Comma=>KeyCode::Char(if shift {'<'} else {','}),
        Key::Minus=>KeyCode::Char(if shift {'_'} else {'-'}),
        Key::Equal=>KeyCode::Char(if shift {'+'} else {'='}),
        Key::NumPadPlus=>KeyCode::Char('+'),
        Key::NumPadMinus=>KeyCode::Char('-'),
        Key::NumPadDot=>KeyCode::Char('.'),
        Key::Semicolon=>KeyCode::Char(if shift {':'} else {';'}),
        Key::Slash=>KeyCode::Char(if shift {'?'} else {'/'}),
        Key::Apostrophe=>KeyCode::Char(if shift {'\"'} else {'\''}),
        Key::NumPadSlash=>KeyCode::Char('/'),
        Key::LeftCtrl | Key::RightCtrl | Key::LeftShift | Key::RightShift => return None,
        _ => return None,
    };
    let modifiers = if control { KeyModifiers::CONTROL }
        else if shift {KeyModifiers::SHIFT} else {KeyModifiers::NONE};
    Some(KeyEvent::new(code, modifiers))
}

fn play_pixels(gm:bool,location:astronomy::Location,fast:bool) -> Result<(), Box<dyn std::error::Error>> {
    let _cleanup_complete=CleanupCompleteScope("X11 window backend");
    // Complete catalogue startup I/O before creating a visible window; once the
    // UI exists, command/input handling is never blocked by a full catalogue scan.
    let mut painter = pixel::PixelRenderer::new();
    if fast { painter.enable_star_map()?; }
    // Must precede creation of minifb's X11 Display connection.
    #[cfg(target_os = "linux")]
    let launcher_threading = !fast || ff_launcher::prepare_x11_threading();
    let _cleanup_window_stage=CleanupStage("X11 window and attached input resources released");
    let mut window = Window::new("SPACE ZOO - Ctrl+C to leave",
        pixel::WIDTH, pixel::HEIGHT, WindowOptions {
            resize: false, scale: Scale::X1, ..WindowOptions::default()
        })?;
    let mut world = World::new(pixel::LOGICAL_W as u16, pixel::LOGICAL_H as u16,
        seed(), DisplayMode::Pixel);
    world.configure_sky(location,fast);
    painter.set_melies_moon(gm);
    #[cfg(target_os = "linux")]
    let monitor = telemetry::spawn();
    let mut input = InputController::new();
    let mut time_panel=fast.then(time_machine::Panel::modern);
    #[cfg(target_os = "linux")]
    let mut launcher = if fast && launcher_threading {
        match ff_launcher::watch_escape(&window,world.sky.star_multiplier()) {
            Ok(events) => Some(events),
            Err(error) => {
                logging::error("RUNTIME_FAILURE","display.launcher",&format!("Time Machine launcher unavailable: {error}"));
                eprintln!("Time Machine launcher unavailable ({error}); using in-scene input");None
            }
        }
    } else {None};
    let target = Duration::from_nanos(1_000_000_000 / FPS);
    #[cfg(target_os = "linux")]
    let mut live_motion=if fast {Some(x11_motion::X11Motion::new(&window)?)} else {None};
    let mut launcher_open=false;
    let mut window_motion=WindowMotion::default();
    let mut last_geo_motion=Instant::now();
    let mut gimbal=GimbalController::default();
    let _cleanup_begin=CleanupScope("X11 window backend");
    while window.is_open() {
        if shutdown::requested() {log_signal_request();break; }
        let started = Instant::now();
        let control = window.is_key_down(Key::LeftCtrl) || window.is_key_down(Key::RightCtrl);
        #[cfg(target_os = "linux")]
        if let Some(receiver) = launcher.as_ref() {
            let mut disable_launcher=false;
            for _ in 0..32 {
                let Ok(event) = receiver.try_recv() else {break;};
                match event {
                    ff_launcher::LauncherEvent::Opened => {launcher_open=true;}
                    ff_launcher::LauncherEvent::Apply(command, reply) => {
                        let result = apply_pixel_command(&mut painter,&mut world,&mut gimbal,&command);
                        let _ = reply.send(result);
                    }
                    ff_launcher::LauncherEvent::Closed => {launcher_open=false;}
                    ff_launcher::LauncherEvent::Quit(kind) => {log_time_machine_exit(kind);return Ok(());},
                    ff_launcher::LauncherEvent::Failed(error) => {
                        logging::error("RUNTIME_FAILURE","display.launcher",&error);
                        eprintln!("Time Machine launcher: {error}; returning to in-scene input");
                        disable_launcher=true;
                        break;
                    }
                }
            }
            if disable_launcher {launcher=None;launcher_open=false;}
        }
        // Arrows remain level-triggered in old mode so dog steering feels immediate.
        for key in [Key::Left, Key::Right, Key::Up, Key::Down] {
            if !fast && window.is_key_down(key) {
                if !apply_key(&mut world,&mut input,
                    KeyEvent::new(match key {
                        Key::Left => KeyCode::Left,
                        Key::Right => KeyCode::Right,
                        Key::Up => KeyCode::Up,
                        _ => KeyCode::Down,
                    }, KeyModifiers::NONE),&mut time_panel) {log_ctrl_c_request();return Ok(());}
            }
        }
        // With the independent X11 watcher, this loop is NOT the command
        // editor or Esc detector. WASD/R are nevertheless direct scene controls
        // while the Time Machine launcher is closed and the main window has focus.
        #[cfg(target_os = "linux")]
        let launcher_enabled = fast && launcher.is_some();
        #[cfg(not(target_os = "linux"))]
        let launcher_enabled = false;
        if launcher_enabled && control && window.is_key_down(Key::C) {log_ctrl_c_request();return Ok(());}
        let shift=window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
        let view_available=fast && !launcher_open &&
            time_panel.as_ref().map_or(true,|panel|!panel.open);
        #[cfg(not(target_os = "linux"))]
        let view_available=view_available && !control;
        #[cfg(target_os = "linux")]
        let live_keys=live_motion.as_mut().map(|motion|motion.sample(view_available));
        #[cfg(target_os = "linux")]
        let view_available=view_available && live_keys.as_ref().is_some_and(|keys|keys.available);
        let movement_down=|key| {
            #[cfg(target_os = "linux")]
            if let Some(keys)=live_keys.as_ref() {return keys.down(key);}
            window.is_key_down(key)
        };
        let movement_pressed=|key| {
            #[cfg(target_os = "linux")]
            if let Some(keys)=live_keys.as_ref() {return keys.pressed(key);}
            window.is_key_pressed(key,KeyRepeat::No)
        };
        let motion_now=Instant::now();
        if view_available {
            // Initial presses are exact one-degree taps. Autorepeat is never consumed
            // as movement; once a key is genuinely held, current key state drives a
            // constant frame-time velocity until the physical release.
            if movement_pressed(Key::W) {painter.view_control(space_zoo::star_projection::ViewControl::Up);}
            if movement_pressed(Key::S) {painter.view_control(space_zoo::star_projection::ViewControl::Down);}
            if movement_pressed(Key::A) {painter.view_control(space_zoo::star_projection::ViewControl::Left);}
            if movement_pressed(Key::D) {painter.view_control(space_zoo::star_projection::ViewControl::Right);}
            if movement_pressed(Key::R) {
                painter.view_control(space_zoo::star_projection::ViewControl::Reset);
                // Reset motion state only; observer coordinates remain untouched.
                window_motion=WindowMotion::default();
            }
            let arrow_up=movement_pressed(Key::Up);
            let arrow_down=movement_pressed(Key::Down);
            let arrow_left=movement_pressed(Key::Left);
            let arrow_right=movement_pressed(Key::Right);
            if gimbal.is_active() && (arrow_up || arrow_down || arrow_left || arrow_right) {
                let _=cancel_gimbal_for_arrow(&mut painter,&mut world,&mut gimbal);
            }
            if arrow_up {let _=move_observer_step(&mut world,1,0);}
            if arrow_down {let _=move_observer_step(&mut world,-1,0);}
            if arrow_left {let _=move_observer_step(&mut world,0,-1);}
            if arrow_right {let _=move_observer_step(&mut world,0,1);}

            let view_input=space_zoo::star_projection::ViewInput::new(
                window_motion.w.active(movement_down(Key::W),motion_now) as i8-
                    window_motion.s.active(movement_down(Key::S),motion_now) as i8,
                window_motion.d.active(movement_down(Key::D),motion_now) as i8-
                    window_motion.a.active(movement_down(Key::A),motion_now) as i8);
            let _=painter.set_view_input(view_input);
            let geo_lat=window_motion.up.active(movement_down(Key::Up),motion_now) as i8-
                window_motion.down.active(movement_down(Key::Down),motion_now) as i8;
            let geo_lon=window_motion.right.active(movement_down(Key::Right),motion_now) as i8-
                window_motion.left.active(movement_down(Key::Left),motion_now) as i8;
            let geo_dt=motion_now.saturating_duration_since(last_geo_motion).as_secs_f64().clamp(0.0,0.1);
            last_geo_motion=motion_now;
            if !gimbal.is_active() && (geo_lat!=0 || geo_lon!=0) {
                let _=move_observer_delta(&mut world,
                    geo_lat as f64*OBSERVER_HOLD_SPEED_DEG_PER_SEC*geo_dt,
                    geo_lon as f64*OBSERVER_HOLD_SPEED_DEG_PER_SEC*geo_dt);
            }
        } else {
            let _=painter.set_view_input(space_zoo::star_projection::ViewInput::default());
            let _=window_motion.w.active(false,motion_now);let _=window_motion.s.active(false,motion_now);
            let _=window_motion.a.active(false,motion_now);let _=window_motion.d.active(false,motion_now);
            let _=window_motion.up.active(false,motion_now);let _=window_motion.down.active(false,motion_now);
            let _=window_motion.left.active(false,motion_now);let _=window_motion.right.active(false,motion_now);
            last_geo_motion=motion_now;
        }
        if fast && !launcher_enabled && !window.is_key_down(Key::Escape) {
            if let Some(panel)=time_panel.as_mut(){panel.release_escape();}
        }
        for key in window.get_keys_pressed(KeyRepeat::Yes).into_iter().take(32) {
            let command_open=time_panel.as_ref().is_some_and(|panel|panel.open);
            if fast && !command_open && matches!(key,Key::W|Key::A|Key::S|Key::D|Key::R|Key::Up|Key::Down|Key::Left|Key::Right) {continue;}
            if fast {
                if launcher_enabled {continue;}
                // minifb can surface Esc autorepeat; only a fresh physical press opens.
                if key==Key::Escape && !window.is_key_pressed(Key::Escape,KeyRepeat::No) {continue;}
            }
            if !fast && matches!(key, Key::Left | Key::Right | Key::Up | Key::Down) { continue; }
            if let Some(event) = key_to_event(key,control,shift) {
                if fast {
                    let panel=time_panel.as_mut().expect("fast fallback owns command panel");
                    if let Some(captured)=panel.capture_modern(event) {
                        match captured {
                            time_machine::Captured::Quit(kind)=>{log_time_machine_exit(kind);return Ok(());},
                            time_machine::Captured::Submit(raw)=>{
                                let reply=apply_pixel_command(&mut painter,&mut world,&mut gimbal,&raw);
                                panel.capture_reply(reply);
                            }
                        }
                    }
                } else if !apply_key(&mut world,&mut input,event,&mut time_panel) {log_ctrl_c_request();return Ok(());}
            }
        }
        world.advance();
        advance_gimbal_for_render(&mut gimbal,&mut world,&mut painter);
        #[cfg(target_os = "linux")]
        if let Some(snapshot) = monitor.take() { painter.update_billboard(snapshot); }
        if let Some(panel)=time_panel.as_ref() {painter.set_time_panel(panel);}
        window.update_with_buffer(painter.draw(&world), pixel::WIDTH, pixel::HEIGHT)?;
        if let Some(remaining) = target.checked_sub(started.elapsed()) { thread::sleep(remaining); }
    }
    Ok(())
}


// Headless A/B proof runner. This is invoked only with --bench-frames;
// the normal 30 Hz / 15 FPS game loop is unchanged.
fn benchmark(frames:u64) {
    use telemetry::{Row,Snapshot};
    let mut world=World::with_hour(pixel::LOGICAL_W as u16,pixel::LOGICAL_H as u16,
        20261010,DisplayMode::Pixel,Some(22));
    let mut painter=pixel::PixelRenderer::new();
    painter.update_billboard(Snapshot {system:telemetry::SystemStats::default(),dies:vec![
        telemetry::Die{package:0,temp_c:Some(39),cores:(0..16).map(|id|
            telemetry::CoreReading{id,state:telemetry::CoreState::Valid(telemetry::CpuBand::Yellow)}).collect()},
        telemetry::Die{package:1,temp_c:Some(41),cores:(16..32).map(|id|
            telemetry::CoreReading{id,state:telemetry::CoreState::Valid(telemetry::CpuBand::Orange)}).collect()},
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
        "night",
        pixel::WIDTH,pixel::HEIGHT);
    println!("total_ms={:.3} render_ms={:.3} render_ms_per_frame={:.4}",
        elapsed.as_secs_f64()*1000.,render_ns as f64/1e6,
        if samples==0 {0.} else {render_ns as f64/1e6/samples as f64});
}

fn run_application() -> Result<(), Box<dyn std::error::Error>> {
    // Explicit opt-in, no /dev/fb0 or X11 needed, does not affect game modes.
    if std::env::args().nth(1).as_deref()==Some("--bench-frames") {
        logging::startup(env!("CARGO_PKG_VERSION"),"headless-benchmark");
        logging::operation("headless benchmark");
        let frames=std::env::args().nth(2).ok_or("benchmark needs an integer frame count")?
            .parse::<u64>()?;
        if frames==0 || frames>100_000 {return Err("bench frame count must be 1..100000".into());}
        benchmark(frames);
        return Ok(());
    }
    let mut mode = DisplayMode::Pixel;
    let mut console_requested = false;
    let mut window_requested = false;
    let mut gm = false;
    let mut fast=true;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ascii" => mode = DisplayMode::Ascii,
            "-V" | "--version" => {
                logging::startup(env!("CARGO_PKG_VERSION"),"utility-version");
                println!("space_zoo {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--gm" => gm = true,
            "--og" => fast=false,
            "--console" => { mode = DisplayMode::Pixel; console_requested = true; },
            "--window" => { mode = DisplayMode::Pixel; window_requested = true; },
            "-h" | "--help" => {
                logging::startup(env!("CARGO_PKG_VERSION"),"utility-help");
                println!("space_zoo [--ascii | --console | --window] [--og] [--gm]\n\
                    -V, --version: print build version and exit\n\
                    Default: SPACE TIME MACHINE / star map at current date and time, forward 1x\n\
                    --og: launch the original normal game\n\
                    Time Machine: Esc opens command legend; Shift+: enters a Vim command\n\
                    Pixel Time Machine view: W/S altitude; A/D azimuth; R reset to north/+30 deg\n\
                    --console: force framebuffer; --window: force desktop window\n\
                    --gm: Georges Melies Moon (face and rocket); pixel mode only\n\
                    Pixel modes: live 2-second Bertha billboard on Linux\n\
                    --og: arrows steer dog; Enter heron; C daytime hen release; Esc helicopter; Ctrl+C leave\n\
                    At night: 1 Chupacabra; 2 Sasquatch (one EXTRA visit each/night).");
                return Ok(());
            }
            _ => { return Err(format!("Unknown option: {arg}. Try --help").into()); }
        }
    }
    if console_requested && window_requested {
        return Err("--console and --window cannot be combined".into());
    }
    if gm && mode==DisplayMode::Ascii {
        return Err("--gm requires pixel mode (--window or --console)".into());
    }
    // Null Island is the baked startup observer location. Arrow-key geolocation
    // remains live at runtime; bare R resets only the camera view.
    let location=astronomy::Location::new(0.0,0.0).expect("Null Island coordinates are valid");
    match mode {
        DisplayMode::Ascii => {
            logging::startup(env!("CARGO_PKG_VERSION"),"ascii-terminal");
            logging::operation("ASCII runtime loop");
            play_ascii(location,fast)
        },
        DisplayMode::Pixel => {
            #[cfg(target_os = "linux")]
            {
                // An actual virtual console takes priority over inherited
                // DISPLAY/WAYLAND_DISPLAY. PTYs must not access /dev/fb0.
                if console_requested || (!window_requested && console::is_virtual_console()) {
                    logging::startup(env!("CARGO_PKG_VERSION"),"linux-framebuffer");
                    logging::operation("framebuffer runtime loop");
                    return play_console_pixels(gm,location,fast);
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
                        if gm {
                            return Err("--gm requires a physical framebuffer console or an X11 window; use --window on a supported desktop".into());
                        }
                        logging::startup(env!("CARGO_PKG_VERSION"),"ascii-terminal");
                        logging::operation("ASCII runtime loop");
                        return play_ascii(location,fast);
                    }
                }
            }
            logging::startup(env!("CARGO_PKG_VERSION"),"x11-window");
            logging::operation("X11 runtime loop");
            play_pixels(gm,location,fast)
        },
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Keep the internal launcher path isolated from the game's logging/session
    // ownership exactly as before.
    if std::env::args().nth(1).as_deref()==Some("--internal-ff-launcher") {
        #[cfg(target_os = "linux")]
        return ff_launcher::run();
        #[cfg(not(target_os = "linux"))]
        return Err("Time Machine launcher requires Linux/X11".into());
    }
    logging::initialize();
    let _panic_log=panic_log::install();
    // Catchable process termination is deferred for every normal Space Zoo mode;
    // the internal launcher remains isolated above. The handler itself only sets
    // atomics, and this guard restores dispositions before clean-exit bookkeeping.
    let signals=shutdown::SignalGuard::install();
    let result=match signals.as_ref() {
        Ok(_)=>run_application(),
        Err(error)=>Err::<(),Box<dyn std::error::Error>>(std::io::Error::new(error.kind(),error.to_string()).into()),
    };
    drop(signals);
    if let Err(error)=&result {
        logging::error("RUNTIME_FAILURE","application.return",&error.to_string());
    }
    logging::cleanup("application subsystem teardown complete");
    let (reason,code)=if let Some(signal)=shutdown::signal_name() {
        (format!("termination_request_{signal}"),0)
    } else if USER_TERMINATION_REQUESTED.load(Ordering::Relaxed) {
        ("ctrl_c_input".to_string(),0)
    } else if USER_QUIT_COMMAND_REQUESTED.load(Ordering::Relaxed) {
        ("time_machine_quit_command".to_string(),0)
    } else if result.is_ok() {
        ("normal_return".to_string(),0)
    } else {
        ("runtime_failure".to_string(),1)
    };
    // Reaching here means Rust has returned through the backend scope and its
    // existing teardown guards have run. A panic never reaches this path.
    logging::finalize(&reason,code,logging::cleanup_succeeded());
    result
}

#[cfg(test)]
mod gimbal_tests {
    use super::*;

    fn configured_world(latitude:f64,longitude:f64)->World {
        let mut world=World::new(80,24,1,DisplayMode::Pixel);
        world.configure_sky(astronomy::Location::new(latitude,longitude).unwrap(),true);
        world
    }

    fn drive(gimbal:&mut GimbalController,world:&mut World,frames:usize)->Vec<astronomy::Location> {
        let base=Instant::now();
        let mut out=Vec::new();
        for n in 0..frames {
            let now=base+Duration::from_secs_f64(n as f64/30.0);
            let step=gimbal.advance_for_render(world,now);
            out.push(world.sky.location().unwrap());
            if matches!(step,GimbalAdvance::Arrived{..}) {break;}
        }
        out
    }

    #[test]
    fn profile_uses_fifteen_degree_acceleration_and_sixty_degree_cap() {
        assert_eq!(GIMBAL_ACCEL_DEG_PER_SEC2,15.0);
        assert_eq!(GIMBAL_MAX_SPEED_DEG_PER_SEC,60.0);
        let longest=GimbalAxisProfile::new(180.0);
        assert_eq!(longest.cruise_time_sec,0.0);
        assert!((longest.peak_speed_deg_per_sec-(2700.0f64).sqrt()).abs()<1e-12);
        assert!(longest.peak_speed_deg_per_sec<60.0);
        assert!((longest.progress_deg(1.0)-7.5).abs()<1e-12);
        assert!((longest.progress_deg(longest.duration_sec())-180.0).abs()<1e-12);
    }

    #[test]
    fn short_trip_is_triangular() {
        let short=GimbalAxisProfile::new(60.0);
        assert_eq!(short.cruise_time_sec,0.0);
        assert!((short.peak_speed_deg_per_sec-(900.0f64).sqrt()).abs()<1e-12);
        assert!(short.peak_speed_deg_per_sec<60.0);
    }

    #[test]
    fn relocation_renders_intermediate_positions_without_overshoot_and_settles_exactly() {
        let mut world=configured_world(0.0,0.0);
        let target=astronomy::Location::new(20.0,30.0).unwrap();
        let mut gimbal=GimbalController::default();
        assert!(gimbal.begin(&mut world,target,false));
        let path=drive(&mut gimbal,&mut world,2000);
        assert!(path.len()>10);
        assert_eq!(world.sky.location(),Some(target));
        assert!(path.iter().all(|p|p.latitude>=-1e-12 && p.latitude<=target.latitude+1e-9));
        assert!(path.iter().all(|p|p.longitude>=-1e-12 && p.longitude<=target.longitude+1e-9));
        assert!(!path.windows(2).any(|w|w[1].latitude+1e-9<w[0].latitude));
        assert!(!path.windows(2).any(|w|w[1].longitude+1e-9<w[0].longitude));
    }

    #[test]
    fn seam_route_170_to_minus_175_stays_on_positive_fifteen_degree_route() {
        let mut world=configured_world(0.0,170.0);
        let target=astronomy::Location::new(0.0,-175.0).unwrap();
        let mut gimbal=GimbalController::default();
        assert!(gimbal.begin(&mut world,target,false));
        let path=drive(&mut gimbal,&mut world,1000);
        assert_eq!(world.sky.location(),Some(target));
        let mut previous:f64=170.0;
        let mut unwrapped:f64=170.0;
        let mut max_unwrapped:f64=170.0;
        for place in path {
            let step=shortest_longitude_delta(previous,place.longitude);
            assert!(step>=-1e-9,"unexpected reverse step {step}");
            unwrapped+=step;
            previous=place.longitude;
            max_unwrapped=max_unwrapped.max(unwrapped);
        }
        assert!((unwrapped-185.0).abs()<1e-9);
        assert!(max_unwrapped<=185.0+1e-9);
    }

    #[test]
    fn pole_adjacent_target_stays_within_physical_latitude() {
        let mut world=configured_world(80.0,0.0);
        let target=astronomy::Location::new(90.0,0.0).unwrap();
        let mut gimbal=GimbalController::default();
        assert!(gimbal.begin(&mut world,target,false));
        let path=drive(&mut gimbal,&mut world,1000);
        assert_eq!(world.sky.location(),Some(target));
        assert!(path.iter().all(|place|place.latitude<=90.0));
    }

    #[test]
    fn thirty_fps_progress_never_exceeds_sixty_degree_speed_cap() {
        let mut world=configured_world(0.0,0.0);
        let target=astronomy::Location::new(0.0,180.0).unwrap();
        let mut gimbal=GimbalController::default();
        assert!(gimbal.begin(&mut world,target,false));
        let path=drive(&mut gimbal,&mut world,1000);
        let mut previous=0.0;
        let mut max_step:f64=0.0;
        for place in path {
            max_step=max_step.max(shortest_longitude_delta(previous,place.longitude).abs());
            previous=place.longitude;
        }
        assert!(max_step<=60.0/30.0+1e-6,"max step {max_step}");
    }


    #[test]
    fn both_axes_have_bounded_intermediate_frame_jumps() {
        let mut world=configured_world(-40.0,170.0);
        let start=world.sky.location().unwrap();
        let target=astronomy::Location::new(45.0,-150.0).unwrap();
        let mut gimbal=GimbalController::default();
        assert!(gimbal.begin(&mut world,target,false));
        let path=drive(&mut gimbal,&mut world,2000);
        let mut previous=start;
        for place in path {
            let lat_jump=(place.latitude-previous.latitude).abs();
            let lon_jump=shortest_longitude_delta(previous.longitude,place.longitude).abs();
            assert!(lat_jump<=60.0/30.0+1e-6,"latitude jump {lat_jump}");
            assert!(lon_jump<=60.0/30.0+1e-6,"longitude jump {lon_jump}");
            assert!(!(lat_jump>1e-9 && lon_jump>1e-9),"latitude-first phases moved together");
            previous=place;
        }
        assert_eq!(previous,target);
    }

    #[test]
    fn arrow_cancellation_during_latitude_travel_stops_old_trip_immediately() {
        let mut world=configured_world(0.0,0.0);
        let target=astronomy::Location::new(30.0,40.0).unwrap();
        let mut gimbal=GimbalController::default();
        assert!(gimbal.begin(&mut world,target,false));
        let base=Instant::now();
        assert_eq!(gimbal.advance_for_render(&mut world,base),GimbalAdvance::Idle);
        assert_eq!(gimbal.advance_for_render(&mut world,base+Duration::from_secs(1)),GimbalAdvance::Moved);
        let at_cancel=world.sky.location().unwrap();
        assert!(at_cancel.latitude>0.0 && at_cancel.latitude<target.latitude);
        assert!(gimbal.cancel(&mut world));
        assert!(!gimbal.is_active());
        assert!(!world.sky.navigation_cache_dirty());
        assert!(move_observer_step(&mut world,1,0));
        let manual=world.sky.location().unwrap();
        assert!((manual.latitude-(at_cancel.latitude+1.0)).abs()<1e-9);
        assert_eq!(gimbal.advance_for_render(&mut world,base+Duration::from_secs(2)),GimbalAdvance::Idle);
        assert_eq!(world.sky.location(),Some(manual));
        assert_ne!(manual,target,"cancelled trip must never snap to destination");
    }

    #[test]
    fn arrow_cancellation_during_longitude_and_just_before_arrival_never_settles_old_target() {
        let base=Instant::now();
        let mut world=configured_world(0.0,0.0);
        let target=astronomy::Location::new(0.0,30.0).unwrap();
        let mut gimbal=GimbalController::default();
        assert!(gimbal.begin(&mut world,target,true));
        for n in 0..300 {
            let _=gimbal.advance_for_render(&mut world,base+Duration::from_secs_f64(n as f64/30.0));
            if world.sky.location().unwrap().longitude>2.0 {break;}
        }
        let at_cancel=world.sky.location().unwrap();
        assert!(at_cancel.longitude>0.0 && at_cancel.longitude<target.longitude);
        assert!(gimbal.cancel(&mut world));
        assert!(move_observer_step(&mut world,0,-1));
        let manual=world.sky.location().unwrap();
        assert_eq!(gimbal.advance_for_render(&mut world,base+Duration::from_secs(20)),GimbalAdvance::Idle);
        assert_eq!(world.sky.location(),Some(manual));

        let near_target=astronomy::Location::new(0.0,1.0).unwrap();
        assert!(gimbal.begin(&mut world,near_target,true));
        let restart=base+Duration::from_secs(30);
        let mut remaining=180.0;
        for n in 0..300 {
            let step=gimbal.advance_for_render(&mut world,restart+Duration::from_secs_f64(n as f64/30.0));
            assert!(!matches!(step,GimbalAdvance::Arrived{..}),"test must cancel before arrival");
            remaining=shortest_longitude_delta(world.sky.location().unwrap().longitude,near_target.longitude).abs();
            if remaining<0.08 {break;}
        }
        assert!(remaining<0.08,"failed to reach pre-arrival window: {remaining}");
        assert!(gimbal.cancel(&mut world));
        assert!(move_observer_step(&mut world,0,-1));
        let cancelled=world.sky.location().unwrap();
        assert_eq!(gimbal.advance_for_render(&mut world,restart+Duration::from_secs(20)),GimbalAdvance::Idle);
        assert_eq!(world.sky.location(),Some(cancelled));
        assert_ne!(cancelled,near_target);
    }

    #[test]
    fn cancelled_trip_never_emits_arrival_camera_reset_signal() {
        let mut world=configured_world(0.0,0.0);
        let mut gimbal=GimbalController::default();
        assert!(gimbal.begin(&mut world,astronomy::Location::new(10.0,10.0).unwrap(),true));
        let base=Instant::now();
        let _=gimbal.advance_for_render(&mut world,base);
        let _=gimbal.advance_for_render(&mut world,base+Duration::from_millis(500));
        assert!(gimbal.cancel(&mut world));
        for n in 1..20 {
            assert_eq!(gimbal.advance_for_render(&mut world,base+Duration::from_secs(n)),GimbalAdvance::Idle);
        }
    }

    #[test]
    fn planet_trip_freezes_time_through_arrival_then_restores_saved_rate() {
        use crate::time_machine::Command;
        for saved_rate in [0.0,1.0,3600.0,-1.0] {
            let mut world=configured_world(10.0,20.0);
            world.sky.command(Command::Date(1_700_000_000)).unwrap();
            world.sky.command(Command::Rate(saved_rate)).unwrap();
            let mut gimbal=GimbalController::default();
            gimbal.begin_planet(&mut world,crate::sky::PlanetKind::Venus).unwrap();
            let frozen=world.sky.now_secs();
            let target=gimbal.travel.unwrap().target;
            assert_eq!(target,crate::planet_ephemeris::default_camera_observer(
                crate::sky::PlanetKind::Venus,frozen).unwrap());
            assert_eq!(world.sky.rate(),0.0);
            let base=Instant::now();
            let mut arrived=false;
            for n in 0..1000 {
                world.advance();
                let step=gimbal.advance_for_render(&mut world,base+Duration::from_secs_f64(n as f64/30.0));
                assert_eq!(world.sky.now_secs(),frozen,"planet navigation clock moved at saved rate {saved_rate}");
                if let GimbalAdvance::Arrived{center_camera}=step {
                    assert!(center_camera,"planet arrival must request the accepted camera reset");
                    assert_eq!(world.sky.rate(),0.0,"time must remain frozen through the arrival reset point");
                    assert!(!world.sky.navigation_cache_dirty());
                    arrived=true;
                    break;
                }
            }
            assert!(arrived,"planet trip did not arrive for rate {saved_rate}");
            assert_eq!(world.sky.location(),Some(target));
            gimbal.finish_arrival(&mut world);
            assert_eq!(world.sky.rate(),saved_rate,"saved rate was not restored");
        }
    }

    #[test]
    fn planet_trip_replacement_cleans_old_pause_before_saving_new_state() {
        use crate::time_machine::Command;
        let mut world=configured_world(0.0,0.0);
        world.sky.command(Command::Date(1_700_000_000)).unwrap();
        world.sky.command(Command::Rate(3600.0)).unwrap();
        let mut gimbal=GimbalController::default();
        gimbal.begin_planet(&mut world,crate::sky::PlanetKind::Mars).unwrap();
        assert_eq!(world.sky.rate(),0.0);
        assert_eq!(gimbal.travel.unwrap().planet_time_rate,Some(3600.0));
        gimbal.begin_planet(&mut world,crate::sky::PlanetKind::Venus).unwrap();
        assert_eq!(world.sky.rate(),0.0);
        assert_eq!(gimbal.travel.unwrap().planet_time_rate,Some(3600.0),"replacement saved temporary pause instead of original rate");
        assert!(gimbal.cancel(&mut world));
        assert_eq!(world.sky.rate(),3600.0);

        gimbal.begin_planet(&mut world,crate::sky::PlanetKind::Mars).unwrap();
        let coordinate=astronomy::Location::new(5.0,6.0).unwrap();
        assert!(gimbal.begin(&mut world,coordinate,false));
        assert_eq!(world.sky.rate(),3600.0,"ordinary coordinate navigation must not pause simulation");
        assert_eq!(gimbal.travel.unwrap().planet_time_rate,None);
    }

    #[test]
    fn explicit_time_command_cancels_planet_trip_and_new_setting_wins() {
        use crate::time_machine::Command;
        let mut world=configured_world(0.0,0.0);
        world.sky.command(Command::Date(1_700_000_000)).unwrap();
        world.sky.command(Command::Rate(3600.0)).unwrap();
        let mut gimbal=GimbalController::default();
        gimbal.begin_planet(&mut world,crate::sky::PlanetKind::Mars).unwrap();
        let command=Command::Rate(-3.0);
        assert!(cancel_planet_for_time_command(&mut gimbal,&mut world,&command));
        assert!(!gimbal.is_active());
        world.sky.command(command).unwrap();
        assert_eq!(world.sky.rate(),-3.0);
    }
}

#[cfg(test)]
mod geo_motion_tests {
    use super::*;

    #[test]
    fn observer_arrow_step_is_exactly_one_degree_and_symmetric() {
        let mut world=World::new(80,24,1,DisplayMode::Pixel);
        world.configure_sky(astronomy::Location::new(0.0,0.0).unwrap(),true);
        assert!(move_observer_step(&mut world,1,0));
        assert_eq!(world.sky.location().unwrap(),astronomy::Location::new(1.0,0.0).unwrap());
        assert!(move_observer_step(&mut world,-1,0));
        assert_eq!(world.sky.location().unwrap(),astronomy::Location::new(0.0,0.0).unwrap());
        assert!(move_observer_step(&mut world,0,1));
        assert_eq!(world.sky.location().unwrap(),astronomy::Location::new(0.0,1.0).unwrap());
        assert!(move_observer_step(&mut world,0,-1));
        assert_eq!(world.sky.location().unwrap(),astronomy::Location::new(0.0,0.0).unwrap());
    }

    #[test]
    fn observer_arrow_steps_clamp_latitude_and_wrap_longitude() {
        let mut world=World::new(80,24,1,DisplayMode::Pixel);
        world.configure_sky(astronomy::Location::new(89.5,179.5).unwrap(),true);
        assert!(move_observer_step(&mut world,1,1));
        let place=world.sky.location().unwrap();
        assert_eq!(place.latitude,90.0);
        assert_eq!(place.longitude,-179.5);
    }

    #[test]
    fn ten_arrow_repeat_events_move_ten_degrees_with_no_tail() {
        let mut world=World::new(80,24,1,DisplayMode::Pixel);
        world.configure_sky(astronomy::Location::new(0.0,0.0).unwrap(),true);
        for _ in 0..10 { assert!(move_observer_step(&mut world,0,1)); }
        let stopped=world.sky.location().unwrap();
        assert_eq!(stopped.longitude,10.0);
        world.advance();
        assert_eq!(world.sky.location().unwrap(),stopped);
    }
}
