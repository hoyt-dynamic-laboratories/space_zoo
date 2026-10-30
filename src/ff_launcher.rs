//! X11 Command Center launcher. A separate X11 connection detects a fresh Esc
//! press independently of the simulation/paint loop. The helper is a short-lived
//! child process with its own focused window and ordered text-event stream.
//! No global grabs and no command text is ever passed to a shell.
use std::cell::RefCell;
use std::io::{self, BufRead, Write};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode,KeyEvent,KeyModifiers};
use minifb::{InputCallback, Key, Scale, Window, WindowOptions};
use x11_dl::xlib;

use crate::{pixel, time_machine};

const WINDOW_W: usize = pixel::COMMAND_CENTER_W;
const WINDOW_H: usize = pixel::COMMAND_CENTER_H;
const ESCAPE_KEYSYM: u64 = 0xff1b;
const MAX_EVENT_BATCH:usize = 4096;
const WINDOW_MARGIN_W:usize = 16;
const WINDOW_MARGIN_H:usize = 48;

fn display_size()->Option<(usize,usize)> {
    let x=xlib::Xlib::open().ok()?;
    let display=unsafe{(x.XOpenDisplay)(std::ptr::null())};if display.is_null(){return None;}
    let screen=unsafe{(x.XDefaultScreen)(display)};
    let width=unsafe{(x.XDisplayWidth)(display,screen)};let height=unsafe{(x.XDisplayHeight)(display,screen)};
    unsafe{(x.XCloseDisplay)(display);}
    (width>0&&height>0).then_some((width as usize,height as usize))
}
fn window_size_for_screen(screen_w:usize,screen_h:usize)->(usize,usize) {
    let usable_w=screen_w.saturating_sub(WINDOW_MARGIN_W).max(1);
    let usable_h=screen_h.saturating_sub(WINDOW_MARGIN_H).max(1);
    (WINDOW_W.min(usable_w),WINDOW_H.min(usable_h))
}
fn initial_window_size()->(usize,usize) {display_size().map_or((WINDOW_W,WINDOW_H),|(w,h)|window_size_for_screen(w,h))}

fn enqueue(events:&Rc<RefCell<Vec<Stroke>>>, event:Stroke) {
    let mut buffered=events.borrow_mut();
    if buffered.len()<MAX_EVENT_BATCH {buffered.push(event);}
}

/// Replies are single-use. Applied state changes close the child; information,
/// directories and validation errors update the existing shell and keep it open.
pub enum LauncherEvent {
    Opened,
    Apply(String, Sender<Result<time_machine::CommandReply, String>>),
    Closed,
    Failed(String),
    Quit(time_machine::QuitKind),
}

pub fn prepare_x11_threading() -> bool {
    let Ok(xlib) = xlib::Xlib::open() else { return false; };
    unsafe { (xlib.XInitThreads)() != 0 }
}

fn escape_down(x:&xlib::Xlib,display:*mut xlib::Display,keycode:u8)->bool {
    let mut state=[0 as std::ffi::c_char;32];
    unsafe {(x.XQueryKeymap)(display,state.as_mut_ptr());}
    let index=(keycode as usize)/8;
    let bit=1u8<<((keycode as usize)%8);
    index<state.len() && (state[index] as u8 & bit)!=0
}

/// After a launcher closes, consume only this watcher's stale Esc stream until
/// the physical key is released. A new panel therefore requires release + a
/// fresh KeyPress, while unrelated application input remains untouched.
fn wait_for_escape_release(x:&xlib::Xlib,display:*mut xlib::Display,keycode:u8) {
    while escape_down(x,display,keycode) {
        // Drain only events already queued on this private watcher. Do not block
        // in XNextEvent: focus can move before key-up, in which case this window
        // may never receive the release even though XQueryKeymap sees it.
        while unsafe{(x.XPending)(display)}>0 {
            let mut event:xlib::XEvent=unsafe{std::mem::zeroed()};
            unsafe{(x.XNextEvent)(display,&mut event);}
        }
        thread::sleep(Duration::from_millis(2));
    }
    // Drop queued autorepeat/stale events on the private watcher connection.
    unsafe{(x.XSync)(display,1);}
}

pub fn watch_escape(window: &Window, initial_star_multiplier:f64) -> Result<Receiver<LauncherEvent>, String> {
    let handle = window.get_window_handle() as usize as xlib::Window;
    if handle == 0 { return Err("X11 window handle unavailable".into()); }
    let executable = std::env::current_exe().map_err(|e|e.to_string())?;
    let (tx, rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let x = xlib::Xlib::open().map_err(|e|format!("Xlib: {e}"))?;
            let display = unsafe { (x.XOpenDisplay)(std::ptr::null()) };
            if display.is_null() {return Err("cannot connect to X11 for Esc events".into());}
            unsafe {
                (x.XSelectInput)(display, handle, xlib::KeyPressMask | xlib::KeyReleaseMask);
                (x.XSync)(display, 0);
            }
            let escape_code=unsafe{(x.XKeysymToKeycode)(display,ESCAPE_KEYSYM as xlib::KeySym)};
            let _ = ready_tx.send(Ok(()));
            let mut star_multiplier=initial_star_multiplier;
            loop {
                let mut event: xlib::XEvent = unsafe { std::mem::zeroed() };
                unsafe { (x.XNextEvent)(display, &mut event); }
                if event.get_type() != xlib::KeyPress { continue; }
                let mut key = unsafe { event.key };
                let symbol = unsafe { (x.XLookupKeysym)(&mut key, 0) } as u64;
                if symbol != ESCAPE_KEYSYM { continue; }
                if !run_child(&executable, &tx, handle, &mut star_multiplier) { break; }
                wait_for_escape_release(&x,display,escape_code);
            }
            unsafe { (x.XCloseDisplay)(display); }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = ready_tx.send(Err(error.clone()));
            let _ = tx.send(LauncherEvent::Failed(error));
        }
    });
    ready_rx.recv().map_err(|e|e.to_string())??;
    Ok(rx)
}

fn encode_field(text:&str)->String {
    let mut out=String::with_capacity(text.len()*2);
    for byte in text.as_bytes(){use std::fmt::Write as _;let _=write!(&mut out,"{byte:02x}");}
    out
}
fn decode_field(hex:&str)->Result<String,String>{
    if hex.len()%2!=0{return Err("launcher protocol field has odd length".into());}
    let mut bytes=Vec::with_capacity(hex.len()/2);
    let chars=hex.as_bytes();
    for i in (0..chars.len()).step_by(2){
        let pair=std::str::from_utf8(&chars[i..i+2]).map_err(|_|"launcher protocol field is not ASCII hex")?;
        bytes.push(u8::from_str_radix(pair,16).map_err(|_|"launcher protocol field is not hex")?);
    }
    String::from_utf8(bytes).map_err(|_|"launcher protocol field is not UTF-8".into())
}
fn view_code(view:time_machine::MenuView)->&'static str {match view{time_machine::MenuView::Main=>"MAIN",time_machine::MenuView::Help=>"HELP",time_machine::MenuView::Planets=>"PLANETS",time_machine::MenuView::Cultures=>"CULTURES"}}
fn parse_view_code(code:&str)->Option<time_machine::MenuView>{match code{"MAIN"=>Some(time_machine::MenuView::Main),"HELP"=>Some(time_machine::MenuView::Help),"PLANETS"=>Some(time_machine::MenuView::Planets),"CULTURES"=>Some(time_machine::MenuView::Cultures),_=>None}}

fn run_child(executable: &std::path::Path, tx: &Sender<LauncherEvent>, return_window:xlib::Window, star_multiplier:&mut f64) -> bool {
    use std::process::{Command, Stdio};
    let parent_pid = std::process::id();
    let mut child = match Command::new(executable)
        .arg("--internal-ff-launcher")
        .env("SPACE_ZOO_FF_PARENT", parent_pid.to_string())
        .env("SPACE_ZOO_FF_RETURN_WINDOW", return_window.to_string())
        .env("SPACE_ZOO_STAR_MULTIPLIER", star_multiplier.to_string())
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {return tx.send(LauncherEvent::Failed(format!("launcher spawn failed: {e}"))).is_ok();}
    };
    if tx.send(LauncherEvent::Opened).is_err() {
        let _ = child.kill(); let _ = child.wait(); return false;
    }
    let Some(mut input) = child.stdin.take() else {
        let _ = child.kill(); let _ = child.wait();
        return tx.send(LauncherEvent::Failed("launcher stdin unavailable".into())).is_ok();
    };
    let Some(output) = child.stdout.take() else {
        let _ = child.kill(); let _ = child.wait();
        return tx.send(LauncherEvent::Failed("launcher stdout unavailable".into())).is_ok();
    };
    for result in io::BufReader::new(output).lines() {
        let Ok(line) = result else { break; };
        if line == "QUIT CTRL_C" {let _ = tx.send(LauncherEvent::Quit(time_machine::QuitKind::CtrlC));break;}
        if line == "QUIT COMMAND" {let _ = tx.send(LauncherEvent::Quit(time_machine::QuitKind::Command));break;}
        let Some(raw) = line.strip_prefix("APPLY ") else {continue;};
        if raw.len() > time_machine::MAX_COMMAND || raw.chars().any(char::is_control) {break;}
        let (reply_tx, reply_rx) = mpsc::channel();
        if tx.send(LauncherEvent::Apply(raw.into(), reply_tx)).is_err() {break;}
        let Ok(response) = reply_rx.recv() else {break;};
        if let Ok(time_machine::Command::Stars(value))=time_machine::parse_modern(raw){*star_multiplier=value;}
        let message=match response {
            Ok(time_machine::CommandReply::Applied)=>"OK\n".to_string(),
            Ok(time_machine::CommandReply::Info(info))=>format!("INFO {}\n",encode_field(&info)),
            Ok(time_machine::CommandReply::View(view,lines))=>format!("VIEW {} {}\n",view_code(view),encode_field(&lines.join("\n"))),
            Err(error)=>format!("ERR {}\n",encode_field(&error)),
        };
        if input.write_all(message.as_bytes()).is_err() || input.flush().is_err() { break; }
    }
    if child.try_wait().ok().flatten().is_none() { let _ = child.kill(); }
    let _ = child.wait();
    tx.send(LauncherEvent::Closed).is_ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stroke { Char(char), Backspace, Enter, Escape, Up, Down, PageUp, PageDown, Quit }

#[derive(Default)]
struct StrokeCollector {events: Rc<RefCell<Vec<Stroke>>>,ctrl: bool}
impl InputCallback for StrokeCollector {
    fn add_char(&mut self, unicode: u32) {
        if self.ctrl {return;}
        if let Some(ch) = char::from_u32(unicode) {
            if !ch.is_control(){enqueue(&self.events,Stroke::Char(ch));}
        }
    }
    fn set_key_state(&mut self, key: Key, pressed: bool) {
        match key {
            Key::LeftCtrl | Key::RightCtrl => self.ctrl = pressed,
            Key::LeftShift | Key::RightShift => {},
            Key::Escape if pressed => enqueue(&self.events,Stroke::Escape),
            Key::Enter if pressed => enqueue(&self.events,Stroke::Enter),
            Key::Backspace if pressed => enqueue(&self.events,Stroke::Backspace),
            Key::Up if pressed=>enqueue(&self.events,Stroke::Up),
            Key::Down if pressed=>enqueue(&self.events,Stroke::Down),
            Key::PageUp if pressed=>enqueue(&self.events,Stroke::PageUp),
            Key::PageDown if pressed=>enqueue(&self.events,Stroke::PageDown),
            Key::C if pressed && self.ctrl => enqueue(&self.events,Stroke::Quit),
            _=>{}
        }
    }
}

fn stroke_key(stroke:Stroke)->Option<KeyEvent>{
    let code=match stroke{
        Stroke::Char(ch)=>KeyCode::Char(ch),Stroke::Backspace=>KeyCode::Backspace,Stroke::Enter=>KeyCode::Enter,
        Stroke::Escape=>KeyCode::Esc,Stroke::Up=>KeyCode::Up,Stroke::Down=>KeyCode::Down,
        Stroke::PageUp=>KeyCode::PageUp,Stroke::PageDown=>KeyCode::PageDown,Stroke::Quit=>return None,
    };
    Some(KeyEvent::new(code,KeyModifiers::NONE))
}

fn focus_launcher(window: &Window, parent:xlib::Window) {
    let Ok(x) = xlib::Xlib::open() else{return;};
    let display = unsafe { (x.XOpenDisplay)(std::ptr::null()) };
    if display.is_null(){return;}
    let handle=window.get_window_handle() as usize as xlib::Window;
    unsafe {
        let mut focused:xlib::Window=0;let mut revert=0;
        (x.XGetInputFocus)(display,&mut focused,&mut revert);
        if focused==parent || focused==handle {
            let mut attributes:xlib::XWindowAttributes=std::mem::zeroed();
            if (x.XGetWindowAttributes)(display,handle,&mut attributes)!=0 && attributes.map_state==xlib::IsViewable {
                (x.XRaiseWindow)(display,handle);
                (x.XSetInputFocus)(display,handle,xlib::RevertToParent,xlib::CurrentTime);
                (x.XFlush)(display);
            }
        }
        (x.XCloseDisplay)(display);
    }
}

struct RestoreFocus { launcher:xlib::Window, parent:xlib::Window }
impl Drop for RestoreFocus {
    fn drop(&mut self) {
        let Ok(x)=xlib::Xlib::open() else{return;};
        let display=unsafe{(x.XOpenDisplay)(std::ptr::null())};if display.is_null(){return;}
        unsafe {
            let mut focused:xlib::Window=0;let mut revert=0;
            (x.XGetInputFocus)(display,&mut focused,&mut revert);
            if focused==self.launcher {(x.XSetInputFocus)(display,self.parent,xlib::RevertToParent,xlib::CurrentTime);(x.XFlush)(display);}
            (x.XCloseDisplay)(display);
        }
    }
}

pub fn run() -> Result<(),Box<dyn std::error::Error>> {
    let expected = std::env::var("SPACE_ZOO_FF_PARENT")?.parse::<u32>()?;
    #[cfg(target_os="linux")]
    unsafe {if libc::prctl(libc::PR_SET_PDEATHSIG,libc::SIGTERM) != 0 || libc::getppid() as u32 != expected {return Err("launcher parent no longer running".into());}}
    let _ = expected;
    let (initial_w,initial_h)=initial_window_size();
    let mut window=Window::new("SPACE ZOO - COMMAND CENTER",initial_w,initial_h,WindowOptions{resize:true,scale:Scale::X1,..WindowOptions::default()})?;
    let events=Rc::new(RefCell::new(Vec::<Stroke>::new()));
    window.set_input_callback(Box::new(StrokeCollector{events:events.clone(),ctrl:false}));
    let (reply_tx,reply_rx)=mpsc::channel::<String>();
    thread::spawn(move || {for result in io::stdin().lock().lines(){match result{Ok(line)=>if reply_tx.send(line).is_err(){break;},Err(_)=>break}}});
    let star_multiplier=std::env::var("SPACE_ZOO_STAR_MULTIPLIER").ok().and_then(|value|value.parse::<f64>().ok())
        .filter(|value|value.is_finite() && *value>0.0 && *value<=crate::display_settings::MAX_STAR_MULTIPLIER)
        .unwrap_or(crate::display_settings::DEFAULT_STAR_MULTIPLIER);
    let mut panel=time_machine::Panel::modern();panel.open_modern();
    let (mut viewport_w,mut viewport_h)=window.get_size();
    viewport_w=viewport_w.max(1);viewport_h=viewport_h.max(1);
    panel.set_viewport(viewport_w,pixel::command_center_visible_rows(viewport_h));
    let mut canvas=pixel::Canvas::new(viewport_w,viewport_h);
    let cursor_clock=Instant::now();
    pixel::paint_command_center_viewport(&mut canvas,&panel,star_multiplier,true,0,0,viewport_w,viewport_h);
    window.update_with_buffer(&canvas.cells,viewport_w,viewport_h)?;
    let mut dirty=false;
    let mut last_cursor_on=true;
    let parent_window=std::env::var("SPACE_ZOO_FF_RETURN_WINDOW")?.parse::<xlib::Window>()?;
    focus_launcher(&window,parent_window);
    let _focus_guard=RestoreFocus {launcher:window.get_window_handle() as usize as xlib::Window,parent:parent_window};
    while window.is_open() {
        if let Ok(reply) = reply_rx.try_recv() {
            if reply=="OK" {return Ok(());}
            if let Some(hex)=reply.strip_prefix("INFO ") {panel.capture_reply(Ok(time_machine::CommandReply::Info(decode_field(hex)?)));}
            else if let Some(rest)=reply.strip_prefix("VIEW ") {
                let (code,hex)=rest.split_once(' ').ok_or("launcher VIEW response missing payload")?;
                let view=parse_view_code(code).ok_or("launcher VIEW response has bad view")?;
                let body=decode_field(hex)?;let lines=if body.is_empty(){Vec::new()}else{body.split('\n').map(str::to_owned).collect()};
                panel.capture_reply(Ok(time_machine::CommandReply::View(view,lines)));
            } else if let Some(hex)=reply.strip_prefix("ERR ") {panel.capture_reply(Err(decode_field(hex)?));}
            else {panel.capture_reply(Err("launcher protocol failure".into()));}
            dirty=true;
        }
        let strokes=std::mem::take(&mut *events.borrow_mut());
        if !strokes.is_empty(){dirty=true;}
        for stroke in strokes {
            if stroke==Stroke::Quit {println!("QUIT CTRL_C");io::stdout().flush()?;return Ok(());}
            if let Some(key)=stroke_key(stroke) {
                let was_open=panel.open;
                if let Some(captured)=panel.capture_modern(key) {
                    match captured {
                        time_machine::Captured::Quit(kind)=>{println!("QUIT {}",match kind{time_machine::QuitKind::CtrlC=>"CTRL_C",time_machine::QuitKind::Command=>"COMMAND"});io::stdout().flush()?;return Ok(());}
                        time_machine::Captured::Submit(cmd)=>{println!("APPLY {cmd}");io::stdout().flush()?;}
                    }
                }
                if was_open && !panel.open {return Ok(());}
            }
        }
        let (next_w,next_h)=window.get_size();
        let next_w=next_w.max(1);let next_h=next_h.max(1);
        if (next_w,next_h)!=(viewport_w,viewport_h) {
            viewport_w=next_w;viewport_h=next_h;canvas=pixel::Canvas::new(viewport_w,viewport_h);
            panel.set_viewport(viewport_w,pixel::command_center_visible_rows(viewport_h));dirty=true;
        }
        let cursor_on=(cursor_clock.elapsed().as_millis()/500)%2==0;
        if cursor_on!=last_cursor_on {dirty=true;last_cursor_on=cursor_on;}
        if dirty {
            pixel::paint_command_center_viewport(&mut canvas,&panel,star_multiplier,cursor_on,0,0,viewport_w,viewport_h);
            window.update_with_buffer(&canvas.cells,viewport_w,viewport_h)?;
            dirty=false;
        } else {
            // Pump window/input events without rebuilding the unchanged menu.
            window.update();
        }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn expanded_command_help_fits_above_entry_area(){assert_eq!(WINDOW_W,640);assert_eq!(WINDOW_H,480);let mut panel=time_machine::Panel::modern();panel.open_modern();panel.capture_reply(Ok(time_machine::CommandReply::View(time_machine::MenuView::Help,Vec::new())));assert!(panel.lines.len()>time_machine::MENU_VISIBLE_ROWS);assert_eq!(panel.visible_lines().len(),time_machine::MENU_VISIBLE_ROWS);}
    #[test]fn launcher_clamps_to_small_displays(){assert_eq!(window_size_for_screen(1920,1080),(640,480));assert_eq!(window_size_for_screen(640,480),(624,432));assert_eq!(window_size_for_screen(480,320),(464,272));}
    #[test]fn protocol_hex_round_trip_preserves_multiline_and_unicode(){let text="line one\n18h36m56s,+38°47' | pipes";assert_eq!(decode_field(&encode_field(text)).unwrap(),text);}
    #[test]fn raw_character_callbacks_preserve_event_order_and_degree(){let events=Rc::new(RefCell::new(Vec::new()));let mut capture=StrokeCollector{events:Rc::clone(&events),ctrl:false};for ch in "1122°".chars(){capture.add_char(ch as u32);}capture.set_key_state(Key::Backspace,true);assert_eq!(&*events.borrow(),&[Stroke::Char('1'),Stroke::Char('1'),Stroke::Char('2'),Stroke::Char('2'),Stroke::Char('°'),Stroke::Backspace]);}
    #[test]fn panel_accepts_direct_typing_colons_and_scroll_without_view_loss(){let mut panel=time_machine::Panel::modern();panel.open_modern();for ch in "18:30:25".chars(){panel.capture_modern(KeyEvent::new(KeyCode::Char(ch),KeyModifiers::NONE));}assert_eq!(panel.input,"18:30:25");panel.capture_modern(KeyEvent::new(KeyCode::Up,KeyModifiers::NONE));assert_eq!(panel.view,time_machine::MenuView::Main);}
    #[test]fn bounds_input_by_utf8_bytes(){let mut panel=time_machine::Panel::modern();panel.open_modern();for _ in 0..2000{panel.capture_modern(KeyEvent::new(KeyCode::Char('7'),KeyModifiers::NONE));}assert_eq!(panel.input.len(),time_machine::MAX_COMMAND);}
}
