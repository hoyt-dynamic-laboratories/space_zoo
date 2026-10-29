//! Dedicated --ff TTY input editor. Command text remains event-driven, while
//! movement is coalesced into a current-state snapshot so keyboard autorepeat
//! can never build a delayed movement backlog behind the renderer.
use std::io;
use std::sync::{mpsc::{self,Receiver,TryRecvError},Arc,Mutex};
use std::time::{Duration,Instant};
use crossterm::event::{self,Event,KeyCode,KeyEventKind,KeyModifiers};
use crate::time_machine::{Captured,CommandReply,Panel,QuitKind};

const MAX_CONSECUTIVE_ERRORS: usize=8;
const REPEAT_CANDIDATE_MIN: Duration=Duration::from_millis(140);
const REPEAT_CANDIDATE_MAX: Duration=Duration::from_millis(600);
const REPEAT_CONFIRM_MAX: Duration=Duration::from_millis(120);
const HOLD_STALE_AFTER: Duration=Duration::from_millis(90);
// Traditional Linux console input may not report KeyRelease. A new Esc press
// after the repeat cadence has gone quiet is therefore treated as a fresh
// press; explicit Release events still arm immediately when available.
const ESC_REPEAT_GAP_MAX: Duration=Duration::from_millis(120);

pub enum FfEvent {Command(String),Quit(QuitKind),Resize(u16,u16)}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct FfMotion {
    pub view_step_altitude:i8,
    pub view_step_azimuth:i8,
    pub geo_step_latitude:i8,
    pub geo_step_longitude:i8,
    // Most recent fresh geographic press since the previous snapshot. This is
    // separate from the bounded coalesced tap totals so Gimbal cancellation can
    // consume one current arrow without replaying navigation-time input.
    pub geo_latest_latitude:i8,
    pub geo_latest_longitude:i8,
    pub view_hold:space_zoo::star_projection::ViewInput,
    pub geo_hold_latitude:i8,
    pub geo_hold_longitude:i8,
    pub reset_view:bool,
}
type Message=io::Result<FfEvent>;

#[derive(Clone,Copy,Debug,Default)]
struct KeyTrack {
    last:Option<Instant>,
    repeat_candidate:bool,
    held:bool,
}
#[derive(Debug,Default)]
struct MotionState {
    keys:[KeyTrack;8],
    view_step_altitude:i8,
    view_step_azimuth:i8,
    geo_step_latitude:i8,
    geo_step_longitude:i8,
    geo_latest_latitude:i8,
    geo_latest_longitude:i8,
    reset_view:bool,
}
impl MotionState {
    fn add_step(&mut self,index:usize) {
        match index {
            0=>self.view_step_altitude=(self.view_step_altitude+1).clamp(-2,2),
            1=>self.view_step_altitude=(self.view_step_altitude-1).clamp(-2,2),
            2=>self.view_step_azimuth=(self.view_step_azimuth-1).clamp(-2,2),
            3=>self.view_step_azimuth=(self.view_step_azimuth+1).clamp(-2,2),
            4=>{self.geo_step_latitude=(self.geo_step_latitude+1).clamp(-2,2);self.geo_latest_latitude=1;self.geo_latest_longitude=0;},
            5=>{self.geo_step_latitude=(self.geo_step_latitude-1).clamp(-2,2);self.geo_latest_latitude=-1;self.geo_latest_longitude=0;},
            6=>{self.geo_step_longitude=(self.geo_step_longitude-1).clamp(-2,2);self.geo_latest_latitude=0;self.geo_latest_longitude=-1;},
            7=>{self.geo_step_longitude=(self.geo_step_longitude+1).clamp(-2,2);self.geo_latest_latitude=0;self.geo_latest_longitude=1;},
            _=>{},
        }
    }
    fn record(&mut self,index:usize,kind:KeyEventKind,now:Instant) {
        let mut track=self.keys[index];
        if kind==KeyEventKind::Release {
            self.keys[index]=KeyTrack::default();
            return;
        }
        if track.held {
            track.last=Some(now);
            self.keys[index]=track;
            return;
        }
        if kind==KeyEventKind::Repeat {
            track.held=true;
            track.last=Some(now);
            track.repeat_candidate=false;
            self.keys[index]=track;
            return;
        }
        let gap=track.last.map(|last|now.saturating_duration_since(last));
        let confirmed=track.repeat_candidate && gap.is_some_and(|gap|gap<=REPEAT_CONFIRM_MAX);
        if confirmed {
            track.held=true;
            track.repeat_candidate=false;
            track.last=Some(now);
            self.keys[index]=track;
            return;
        }
        self.add_step(index);
        track.last=Some(now);
        track.repeat_candidate=gap.is_some_and(|gap|gap>=REPEAT_CANDIDATE_MIN && gap<=REPEAT_CANDIDATE_MAX);
        self.keys[index]=track;
    }
    fn reset_camera(&mut self) {
        // R never relocates, but it does discard every queued/held movement state
        // so no stale camera or geographic motion resumes after the reset.
        for track in &mut self.keys {*track=KeyTrack::default();}
        self.view_step_altitude=0;self.view_step_azimuth=0;
        self.geo_step_latitude=0;self.geo_step_longitude=0;
        self.geo_latest_latitude=0;self.geo_latest_longitude=0;
        self.reset_view=true;
    }
    fn snapshot(&mut self,now:Instant)->FfMotion {
        for track in &mut self.keys {
            if track.held && track.last.is_some_and(|last|now.saturating_duration_since(last)>HOLD_STALE_AFTER) {
                *track=KeyTrack::default();
            }
        }
        let active=|index:usize|self.keys[index].held;
        let motion=FfMotion {
            view_step_altitude:self.view_step_altitude,
            view_step_azimuth:self.view_step_azimuth,
            geo_step_latitude:self.geo_step_latitude,
            geo_step_longitude:self.geo_step_longitude,
            geo_latest_latitude:self.geo_latest_latitude,
            geo_latest_longitude:self.geo_latest_longitude,
            view_hold:space_zoo::star_projection::ViewInput::new(
                active(0) as i8-active(1) as i8,
                active(3) as i8-active(2) as i8),
            geo_hold_latitude:active(4) as i8-active(5) as i8,
            geo_hold_longitude:active(7) as i8-active(6) as i8,
            reset_view:self.reset_view,
        };
        self.view_step_altitude=0;self.view_step_azimuth=0;
        self.geo_step_latitude=0;self.geo_step_longitude=0;
        self.geo_latest_latitude=0;self.geo_latest_longitude=0;
        self.reset_view=false;
        motion
    }
}

pub struct FfKeyboard {
    rx:Receiver<Message>,
    panel:Arc<Mutex<Panel>>,
    motion:Arc<Mutex<MotionState>>,
}
impl FfKeyboard {
    pub fn new()->Self {Self::with_mode(false)}
    pub fn new_modern()->Self {Self::with_mode(true)}
    fn with_mode(modern:bool)->Self {
        let panel=Arc::new(Mutex::new(if modern {Panel::modern()} else {Panel::default()}));
        let shared=Arc::clone(&panel);
        let motion=Arc::new(Mutex::new(MotionState::default()));
        let motion_writer=Arc::clone(&motion);
        let (tx,rx)=mpsc::sync_channel(64);
        std::thread::spawn(move || {
            let mut failures=0;
            let mut last_escape_event:Option<Instant>=None;
            loop {
                match event::read() {
                    Ok(event)=>{
                        failures=0;
                        let outgoing=match event {
                            Event::Key(key)=>{
                                let Ok(mut edit)=shared.lock() else {break;};
                                let movement=if !edit.open && !key.modifiers.contains(KeyModifiers::CONTROL) {
                                    match key.code {
                                        KeyCode::Char('w'|'W')=>Some(0),KeyCode::Char('s'|'S')=>Some(1),
                                        KeyCode::Char('a'|'A')=>Some(2),KeyCode::Char('d'|'D')=>Some(3),
                                        KeyCode::Up=>Some(4),KeyCode::Down=>Some(5),
                                        KeyCode::Left=>Some(6),KeyCode::Right=>Some(7),
                                        _=>None,
                                    }
                                } else {None};
                                if let Some(index)=movement {
                                    if let Ok(mut state)=motion_writer.lock() {state.record(index,key.kind,Instant::now());}
                                    None
                                } else if !edit.open && !key.modifiers.contains(KeyModifiers::CONTROL) &&
                                    matches!(key.code,KeyCode::Char('r'|'R')) {
                                    if key.kind!=KeyEventKind::Release {
                                        if let Ok(mut state)=motion_writer.lock(){state.reset_camera();}
                                    }
                                    None
                                } else {
                                    if modern && key.code==KeyCode::Esc {
                                        let now=Instant::now();
                                        if key.kind==KeyEventKind::Release {
                                            edit.release_escape();
                                            last_escape_event=None;
                                        } else {
                                            let fresh_gap=last_escape_event
                                                .map_or(true,|last|now.saturating_duration_since(last)>ESC_REPEAT_GAP_MAX);
                                            // On terminals without release events, infer a release only
                                            // after the autorepeat stream has stopped. Never re-arm while
                                            // the panel itself is still open.
                                            if key.kind!=KeyEventKind::Repeat && !edit.open && fresh_gap {
                                                edit.release_escape();
                                            }
                                            last_escape_event=Some(now);
                                        }
                                    }
                                    match if modern {edit.capture_modern(key)} else {edit.capture(key)} {
                                        Some(Captured::Submit(cmd))=>Some(FfEvent::Command(cmd)),
                                        Some(Captured::Quit(kind))=>Some(FfEvent::Quit(kind)),
                                        None=>None,
                                    }
                                }
                            }
                            Event::Resize(w,h)=>Some(FfEvent::Resize(w,h)),
                            _=>None,
                        };
                        if let Some(message)=outgoing {
                            if tx.send(Ok(message)).is_err(){break;}
                        }
                    }
                    Err(error) if error.kind()==io::ErrorKind::Interrupted=>continue,
                    Err(error) if matches!(error.kind(),
                        io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe | io::ErrorKind::NotConnected)=>{
                        let _=tx.send(Err(error));break;
                    }
                    Err(error)=>{
                        failures+=1;
                        if failures>=MAX_CONSECUTIVE_ERRORS {
                            let _=tx.send(Err(io::Error::new(error.kind(),
                                format!("Time Machine keyboard failed {failures} times: {error}"))));
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        });
        Self {rx,panel,motion}
    }
    pub fn try_next(&self)->io::Result<Option<FfEvent>> {
        match self.rx.try_recv() {
            Ok(Ok(event))=>Ok(Some(event)),
            Ok(Err(error))=>Err(error),
            Err(TryRecvError::Empty)=>Ok(None),
            Err(TryRecvError::Disconnected)=>Err(io::Error::new(io::ErrorKind::BrokenPipe,
                "Time Machine keyboard reader disconnected")),
        }
    }
    pub fn take_motion(&self)->FfMotion {
        self.motion.lock().map(|mut state|state.snapshot(Instant::now())).unwrap_or_default()
    }
    pub fn panel(&self)->io::Result<Panel> {
        self.panel.lock().map(|panel|panel.clone())
            .map_err(|_|io::Error::other("Time Machine editor lock poisoned"))
    }
    pub fn reply(&self,result:Result<CommandReply,String>)->io::Result<()> {
        self.panel.lock().map(|mut panel|panel.capture_reply(result))
            .map_err(|_|io::Error::other("Time Machine editor lock poisoned"))
    }
    pub fn reply_legacy(&self,result:Result<(),String>)->io::Result<()> {
        let converted=result.map(|_|CommandReply::Applied);
        self.panel.lock().map(|mut panel|panel.capture_reply(converted))
            .map_err(|_|io::Error::other("Time Machine editor lock poisoned"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rapid_taps_are_coalesced_not_backlogged() {
        let mut state=MotionState::default();
        let t=Instant::now();
        for i in 0..20 {state.record(3,KeyEventKind::Press,t+Duration::from_millis(i*20));}
        let first=state.snapshot(t+Duration::from_millis(400));
        assert!(first.view_step_azimuth<=2,"movement backlog must be bounded");
        let second=state.snapshot(t+Duration::from_millis(401));
        assert_eq!(second.view_step_azimuth,0,"consumed taps must not replay later");
    }
    #[test]
    fn latest_geographic_press_is_separate_from_coalesced_backlog() {
        let mut state=MotionState::default();
        let t=Instant::now();
        state.record(4,KeyEventKind::Press,t);
        state.record(4,KeyEventKind::Press,t+Duration::from_millis(10));
        state.record(7,KeyEventKind::Press,t+Duration::from_millis(20));
        let motion=state.snapshot(t+Duration::from_millis(25));
        assert_eq!(motion.geo_step_latitude,2);
        assert_eq!(motion.geo_step_longitude,1);
        assert_eq!((motion.geo_latest_latitude,motion.geo_latest_longitude),(0,1));
        let consumed=state.snapshot(t+Duration::from_millis(26));
        assert_eq!((consumed.geo_latest_latitude,consumed.geo_latest_longitude),(0,0));
    }
    #[test]
    fn terminal_repeat_becomes_live_hold_then_expires() {
        let mut state=MotionState::default();
        let t=Instant::now();
        state.record(0,KeyEventKind::Press,t);
        state.record(0,KeyEventKind::Press,t+Duration::from_millis(250));
        state.record(0,KeyEventKind::Press,t+Duration::from_millis(300));
        assert_eq!(state.snapshot(t+Duration::from_millis(320)).view_hold.altitude_axis,1);
        assert_eq!(state.snapshot(t+Duration::from_millis(410)).view_hold.altitude_axis,0);
    }
    #[test]
    fn explicit_release_stops_hold_immediately() {
        let mut state=MotionState::default();
        let t=Instant::now();
        state.record(2,KeyEventKind::Repeat,t);
        assert_eq!(state.snapshot(t).view_hold.azimuth_axis,-1);
        state.record(2,KeyEventKind::Release,t+Duration::from_millis(1));
        assert_eq!(state.snapshot(t+Duration::from_millis(1)).view_hold.azimuth_axis,0);
    }
}
