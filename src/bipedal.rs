//! v0.55.20: generalized foreground cryptid engine with standardized QUADRUPED support.
//!
//! The shared actor/render layer owns world roots, scaling, mirroring, dirty bounds,
//! routing and saved-file config reload. BIPED preserves its accepted front/back turn
//! routine; QUADRUPED uses six walk poses plus idle down/up and a mirrored turnaround
//! without front/rear pose requirements. Creature-specific geometry, scale and timing
//! terminate in each actor's independent config.
use std::{
    collections::HashSet,
    fs,
    path::{Path,PathBuf},
    sync::{Arc,Mutex,OnceLock},
};

use crate::pixel::{Canvas,HEIGHT,WIDTH};
use crate::world::FPS;

const LABEL_VERTICAL_GAP_PX:i32=6;
const ROTATION_CONFIG:&str=include_str!("../config/cryptids/rotation.config");
const ROTATION_RELATIVE_PATH:&str="config/cryptids/rotation.config";
const MAX_CONFIG_BYTES:u64=32*1024;
const CONFIG_POLL_TICKS:u64=FPS/2; // 15 ticks at 30 FPS = 500 ms.
const MAX_FRAMES:usize=16;
const MAX_WALK_POSES:usize=16;
const MAX_ROTATION:usize=64;

#[derive(Clone,Copy)]
struct FrameAsset {
    name:&'static str,
    file:&'static str,
    w:i32,
    h:i32,
    data:&'static [u8],
}

const DOGMAN_FRAMES:[FrameAsset;8]=[
    FrameAsset{name:"WALK_00",file:"assets/dogman/runtime_frames/0_walk_00.rgba",w:353,h:468,data:include_bytes!("../assets/dogman/runtime_frames/0_walk_00.rgba")},
    FrameAsset{name:"WALK_01",file:"assets/dogman/runtime_frames/1_walk_01.rgba",w:237,h:473,data:include_bytes!("../assets/dogman/runtime_frames/1_walk_01.rgba")},
    FrameAsset{name:"WALK_02",file:"assets/dogman/runtime_frames/2_walk_02.rgba",w:357,h:464,data:include_bytes!("../assets/dogman/runtime_frames/2_walk_02.rgba")},
    FrameAsset{name:"WALK_03",file:"assets/dogman/runtime_frames/3_walk_03.rgba",w:223,h:474,data:include_bytes!("../assets/dogman/runtime_frames/3_walk_03.rgba")},
    FrameAsset{name:"NEUTRAL_FRONT",file:"assets/dogman/runtime_frames/4_neutral_front.rgba",w:276,h:462,data:include_bytes!("../assets/dogman/runtime_frames/4_neutral_front.rgba")},
    FrameAsset{name:"IDLE_UP",file:"assets/dogman/runtime_frames/5_idle_up.rgba",w:251,h:461,data:include_bytes!("../assets/dogman/runtime_frames/5_idle_up.rgba")},
    FrameAsset{name:"IDLE_DOWN",file:"assets/dogman/runtime_frames/6_idle_down.rgba",w:325,h:434,data:include_bytes!("../assets/dogman/runtime_frames/6_idle_down.rgba")},
    FrameAsset{name:"NEUTRAL_BACK",file:"assets/dogman/runtime_frames/7_neutral_back.rgba",w:255,h:460,data:include_bytes!("../assets/dogman/runtime_frames/7_neutral_back.rgba")},
];
const LONDON_WEREWOLF_FRAMES:[FrameAsset;8]=[
    FrameAsset{name:"WALK_00",file:"assets/london_werewolf/runtime_frames/0_walk_00.rgba",w:388,h:473,data:include_bytes!("../assets/london_werewolf/runtime_frames/0_walk_00.rgba")},
    FrameAsset{name:"WALK_01",file:"assets/london_werewolf/runtime_frames/1_walk_01.rgba",w:297,h:485,data:include_bytes!("../assets/london_werewolf/runtime_frames/1_walk_01.rgba")},
    FrameAsset{name:"WALK_02",file:"assets/london_werewolf/runtime_frames/2_walk_02.rgba",w:384,h:473,data:include_bytes!("../assets/london_werewolf/runtime_frames/2_walk_02.rgba")},
    FrameAsset{name:"WALK_03",file:"assets/london_werewolf/runtime_frames/3_walk_03.rgba",w:267,h:471,data:include_bytes!("../assets/london_werewolf/runtime_frames/3_walk_03.rgba")},
    FrameAsset{name:"NEUTRAL_FRONT",file:"assets/london_werewolf/runtime_frames/4_neutral_front.rgba",w:366,h:469,data:include_bytes!("../assets/london_werewolf/runtime_frames/4_neutral_front.rgba")},
    FrameAsset{name:"IDLE_UP",file:"assets/london_werewolf/runtime_frames/5_idle_up.rgba",w:302,h:465,data:include_bytes!("../assets/london_werewolf/runtime_frames/5_idle_up.rgba")},
    FrameAsset{name:"IDLE_DOWN",file:"assets/london_werewolf/runtime_frames/6_idle_down.rgba",w:321,h:436,data:include_bytes!("../assets/london_werewolf/runtime_frames/6_idle_down.rgba")},
    FrameAsset{name:"NEUTRAL_BACK",file:"assets/london_werewolf/runtime_frames/7_neutral_back.rgba",w:285,h:470,data:include_bytes!("../assets/london_werewolf/runtime_frames/7_neutral_back.rgba")},
];
const BAA_FRAMES:[FrameAsset;8]=[
    FrameAsset{name:"WALK_00",file:"assets/baa/runtime_frames/0_walk_00.rgba",w:465,h:299,data:include_bytes!("../assets/baa/runtime_frames/0_walk_00.rgba")},
    FrameAsset{name:"WALK_01",file:"assets/baa/runtime_frames/1_walk_01.rgba",w:456,h:301,data:include_bytes!("../assets/baa/runtime_frames/1_walk_01.rgba")},
    FrameAsset{name:"WALK_02",file:"assets/baa/runtime_frames/2_walk_02.rgba",w:469,h:301,data:include_bytes!("../assets/baa/runtime_frames/2_walk_02.rgba")},
    FrameAsset{name:"WALK_03",file:"assets/baa/runtime_frames/3_walk_03.rgba",w:477,h:298,data:include_bytes!("../assets/baa/runtime_frames/3_walk_03.rgba")},
    FrameAsset{name:"WALK_04",file:"assets/baa/runtime_frames/4_walk_04.rgba",w:465,h:299,data:include_bytes!("../assets/baa/runtime_frames/4_walk_04.rgba")},
    FrameAsset{name:"WALK_05",file:"assets/baa/runtime_frames/5_walk_05.rgba",w:477,h:296,data:include_bytes!("../assets/baa/runtime_frames/5_walk_05.rgba")},
    FrameAsset{name:"IDLE_DOWN",file:"assets/baa/runtime_frames/6_idle_down.rgba",w:486,h:247,data:include_bytes!("../assets/baa/runtime_frames/6_idle_down.rgba")},
    FrameAsset{name:"IDLE_UP",file:"assets/baa/runtime_frames/7_idle_up.rgba",w:482,h:365,data:include_bytes!("../assets/baa/runtime_frames/7_idle_up.rgba")},
];

const CHUPACABRA_FRAMES:[FrameAsset;8]=[
    FrameAsset{name:"WALK_00",file:"assets/chupacabra/runtime_frames/0_walk_00.rgba",w:495,h:384,data:include_bytes!("../assets/chupacabra/runtime_frames/0_walk_00.rgba")},
    FrameAsset{name:"WALK_01",file:"assets/chupacabra/runtime_frames/1_walk_01.rgba",w:497,h:370,data:include_bytes!("../assets/chupacabra/runtime_frames/1_walk_01.rgba")},
    FrameAsset{name:"WALK_02",file:"assets/chupacabra/runtime_frames/2_walk_02.rgba",w:469,h:365,data:include_bytes!("../assets/chupacabra/runtime_frames/2_walk_02.rgba")},
    FrameAsset{name:"WALK_03",file:"assets/chupacabra/runtime_frames/3_walk_03.rgba",w:503,h:347,data:include_bytes!("../assets/chupacabra/runtime_frames/3_walk_03.rgba")},
    FrameAsset{name:"WALK_04",file:"assets/chupacabra/runtime_frames/4_walk_04.rgba",w:464,h:347,data:include_bytes!("../assets/chupacabra/runtime_frames/4_walk_04.rgba")},
    FrameAsset{name:"WALK_05",file:"assets/chupacabra/runtime_frames/5_walk_05.rgba",w:485,h:349,data:include_bytes!("../assets/chupacabra/runtime_frames/5_walk_05.rgba")},
    FrameAsset{name:"IDLE_DOWN",file:"assets/chupacabra/runtime_frames/6_idle_down.rgba",w:553,h:292,data:include_bytes!("../assets/chupacabra/runtime_frames/6_idle_down.rgba")},
    FrameAsset{name:"IDLE_UP",file:"assets/chupacabra/runtime_frames/7_idle_up.rgba",w:558,h:358,data:include_bytes!("../assets/chupacabra/runtime_frames/7_idle_up.rgba")},
];

#[derive(Clone,Copy)]
struct EmbeddedActor {
    id:&'static str,
    label:&'static str,
    config_text:&'static str,
    config_path:&'static str,
    frames:&'static [FrameAsset],
}
const ACTORS:[EmbeddedActor;4]=[
    EmbeddedActor{id:"dogman",label:"DOGMAN",config_text:include_str!("../config/cryptids/dogman.config"),config_path:"config/cryptids/dogman.config",frames:&DOGMAN_FRAMES},
    EmbeddedActor{id:"london_werewolf",label:"LONDON WEREWOLF",config_text:include_str!("../config/cryptids/london_werewolf.config"),config_path:"config/cryptids/london_werewolf.config",frames:&LONDON_WEREWOLF_FRAMES},
    EmbeddedActor{id:"baa",label:"B'AA",config_text:include_str!("../config/cryptids/baa.config"),config_path:"config/cryptids/baa.config",frames:&BAA_FRAMES},
    EmbeddedActor{id:"chupacabra",label:"CHUPACABRA",config_text:include_str!("../config/cryptids/chupacabra.config"),config_path:"config/cryptids/chupacabra.config",frames:&CHUPACABRA_FRAMES},
];

// v0.55.38 Rusty RAM: the embedded RGBA blobs are cold backing storage only.
// Runtime rendering owns one contiguous current-actor copy, so rotation can
// release the previous actor instead of accumulating every touched sprite page.
struct ActiveActor {
    actor_idx:usize,
    bytes:Vec<u8>,
    offsets:[usize;MAX_FRAMES+1],
}
impl ActiveActor {
    fn load(actor_idx:usize)->Self {
        let frames=ACTORS[actor_idx].frames;
        let total=frames.iter().map(|frame|frame.data.len()).sum();
        let mut bytes=Vec::with_capacity(total);
        let mut offsets=[0usize;MAX_FRAMES+1];
        for (i,frame) in frames.iter().enumerate() {
            offsets[i]=bytes.len();
            bytes.extend_from_slice(frame.data);
        }
        offsets[frames.len()]=bytes.len();
        for frame in frames {discard_embedded_residency(frame.data);}
        Self{actor_idx,bytes,offsets}
    }
    fn frame(&self,frame:usize)->&[u8] {
        debug_assert!(frame<ACTORS[self.actor_idx].frames.len());
        &self.bytes[self.offsets[frame]..self.offsets[frame+1]]
    }
}

#[cfg(target_os="linux")]
fn discard_embedded_residency(data:&'static [u8]) {
    if data.is_empty(){return;}
    // include_bytes! keeps single-binary fallback semantics, but these pages are
    // file-backed by the executable. Once copied into ActiveActor, tell Linux the
    // cold backing pages are immediately reclaimable. They can be faulted back in
    // later if this actor rotates into service again.
    unsafe {
        let page=libc::sysconf(libc::_SC_PAGESIZE);
        if page<=0{return;}
        let page=page as usize;
        let start=data.as_ptr() as usize;
        let end=start.saturating_add(data.len());
        let aligned_start=start/page*page;
        let aligned_end=end.saturating_add(page-1)/page*page;
        if aligned_end>aligned_start {
            let _=libc::madvise(aligned_start as *mut libc::c_void,aligned_end-aligned_start,libc::MADV_DONTNEED);
        }
    }
}
#[cfg(not(target_os="linux"))]
fn discard_embedded_residency(_data:&'static [u8]) {}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Facing {Left,Right}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum RouteState {
    Enter,WalkTo2,IdleAt2,WalkTo3,IdleAt3,WalkTo4,PoseAt4,
    TurnFront,TurnBack,TurnFaceSwitch,ReturnWalk,Exit,OffscreenHold,
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum LocomotionFamily {Biped,Quadruped}

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
struct FrameRegistration {anchor_x:i32,anchor_y:i32}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum WalkSequenceDirection {Forward,Reverse}

#[derive(Clone,Copy,Debug)]
// Shared walk/idle timing layout; BIPED additionally consumes the front/back turn fields.
struct BipedRoutine {
    walk_speed:f64,
    walk_frames:[u8;MAX_WALK_POSES],
    walk_pose_ms:[f64;MAX_WALK_POSES],
    walk_count:usize,
    walk_sequence_direction:WalkSequenceDirection,
    idle_up_frame:u8,
    idle_down_frame:u8,
    idle_up_ms:u64,
    idle_down_ms:u64,
    linger_bounces_2:u64,
    linger_bounces_3:u64,
    anchor_4_intro_frame:u8,
    anchor_4_intro_ms:u64,
    anchor_4_bounces:u64,
    turn_front_before_frame:u8,
    turn_front_before_ms:u64,
    turn_back_frame:u8,
    turn_back_ms:u64,
    turn_front_after_frame:u8,
    turn_front_after_ms:u64,
    inter_actor_delay_ms:u64,
}

#[derive(Clone,Copy,Debug)]
struct ActorConfig {
    locomotion:LocomotionFamily,
    scale:f64,
    frame_count:usize,
    registrations:[FrameRegistration;MAX_FRAMES],
    biped:BipedRoutine,
}

fn config_value<'a>(text:&'a str,key:&str)->Result<&'a str,String> {
    for raw in text.lines() {
        let line=raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {continue;}
        if let Some((k,v))=line.split_once('=') {
            if k.trim()==key {return Ok(v.trim());}
        }
    }
    Err(format!("missing cryptid config key {key}"))
}
fn config_number(text:&str,key:&str)->Result<f64,String> {
    config_value(text,key)?.parse().map_err(|_|format!("invalid cryptid config number for {key}"))
}
fn config_i32(text:&str,key:&str)->Result<i32,String> {
    config_value(text,key)?.parse().map_err(|_|format!("invalid cryptid config integer for {key}"))
}
fn config_u64(text:&str,key:&str)->Result<u64,String> {
    config_value(text,key)?.parse().map_err(|_|format!("invalid cryptid config integer for {key}"))
}
fn config_usize(text:&str,key:&str)->Result<usize,String> {
    config_value(text,key)?.parse().map_err(|_|format!("invalid cryptid config integer for {key}"))
}
fn validate_unique_keys(text:&str)->Result<(),String> {
    let mut seen=HashSet::new();
    for raw in text.lines() {
        let line=raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {continue;}
        let Some((k,_))=line.split_once('=') else {return Err(format!("invalid cryptid config line: {line}"));};
        let key=k.trim();
        if key.is_empty() {return Err("empty cryptid config key".to_string());}
        if !seen.insert(key) {return Err(format!("duplicate cryptid config key {key}"));}
    }
    Ok(())
}
fn parse_walk_direction(value:&str)->Result<WalkSequenceDirection,String> {
    match value.to_ascii_lowercase().as_str() {
        "forward"|"fwd"=>Ok(WalkSequenceDirection::Forward),
        "reverse"|"backward"|"backwd"|"back"=>Ok(WalkSequenceDirection::Reverse),
        v=>Err(format!("invalid walk direction {v}; use forward or reverse")),
    }
}
fn parse_locomotion(value:&str)->Result<LocomotionFamily,String> {
    match value.to_ascii_uppercase().as_str() {
        "BIPED"=>Ok(LocomotionFamily::Biped),
        "QUADRUPED"=>Ok(LocomotionFamily::Quadruped),
        v=>Err(format!("unknown locomotion type {v}")),
    }
}
fn actor_index_by_id(id:&str)->Option<usize> {ACTORS.iter().position(|a|a.id.eq_ignore_ascii_case(id))}
fn frame_index_by_name(actor:EmbeddedActor,name:&str)->Option<u8> {
    actor.frames.iter().position(|f|f.name.eq_ignore_ascii_case(name)).map(|i|i as u8)
}
fn config_frame(text:&str,actor:EmbeddedActor,key:&str)->Result<u8,String> {
    let name=config_value(text,key)?;
    frame_index_by_name(actor,name).ok_or_else(||format!("invalid sequence frame reference {name} for {}",actor.id))
}

fn parse_actor_config(actor_idx:usize,text:&str)->Result<ActorConfig,String> {
    validate_unique_keys(text)?;
    let actor=ACTORS[actor_idx];
    if config_value(text,"actor_id")?!=actor.id {return Err(format!("actor_id must be {}",actor.id));}
    if config_value(text,"label")?!=actor.label {return Err(format!("label must be {}",actor.label));}
    let locomotion=parse_locomotion(config_value(text,"locomotion")?)?;
    let scale=config_number(text,"scale")?;
    if !scale.is_finite()||scale<=0.0||scale>16.0 {return Err("cryptid scale must be positive, finite, and <= 16".to_string());}
    let frame_count=config_usize(text,"frame_count")?;
    if frame_count==0||frame_count>MAX_FRAMES||frame_count!=actor.frames.len() {
        return Err(format!("frame_count must equal embedded actor frame count {}",actor.frames.len()));
    }
    let mut registrations=[FrameRegistration::default();MAX_FRAMES];
    for (i,asset) in actor.frames.iter().enumerate() {
        let name=config_value(text,&format!("frame.{i}.name"))?;
        let file=config_value(text,&format!("frame.{i}.file"))?;
        let w=config_i32(text,&format!("frame.{i}.width"))?;
        let h=config_i32(text,&format!("frame.{i}.height"))?;
        let anchor_x=config_i32(text,&format!("frame.{i}.anchor_x"))?;
        let anchor_y=config_i32(text,&format!("frame.{i}.anchor_y"))?;
        if name!=asset.name {return Err(format!("frame.{i}.name must be {}",asset.name));}
        if file!=asset.file {return Err(format!("invalid frame path for {}: {file}",asset.name));}
        if w!=asset.w||h!=asset.h {return Err(format!("frame dimensions disagree with embedded asset {}",asset.name));}
        if anchor_x<0||anchor_x>=w||anchor_y<0||anchor_y>=h {
            return Err(format!("invalid anchor for {}: {anchor_x},{anchor_y}",asset.name));
        }
        registrations[i]=FrameRegistration{anchor_x,anchor_y};
    }

    let walk_speed=config_number(text,"walk_speed_px_per_second")?;
    if !walk_speed.is_finite()||walk_speed<=0.0 {return Err("movement speed must be positive and finite".to_string());}
    let walk_count=config_usize(text,"walk_pose_count")?;
    if walk_count==0||walk_count>MAX_WALK_POSES {return Err(format!("walk_pose_count must be 1..={MAX_WALK_POSES}"));}
    let mut walk_frames=[0u8;MAX_WALK_POSES];
    let mut walk_pose_ms=[1.0f64;MAX_WALK_POSES];
    for i in 0..walk_count {
        let pose_key=format!("walk_pose_{i:02}");
        let time_key=format!("walk_pose_{i:02}_ms");
        walk_frames[i]=config_frame(text,actor,&pose_key)?;
        let ms=config_number(text,&time_key)?;
        if !ms.is_finite()||ms<=0.0 {return Err(format!("invalid walk timing for {time_key}"));}
        walk_pose_ms[i]=ms;
    }
    let idle_up_ms=config_u64(text,"idle_up_ms")?;
    let idle_down_ms=config_u64(text,"idle_down_ms")?;
    if idle_up_ms==0||idle_down_ms==0 {return Err("idle timing must be positive".to_string());}
    let idle_up_frame=config_frame(text,actor,"idle_up_pose")?;
    let idle_down_frame=config_frame(text,actor,"idle_down_pose")?;
    let anchor_4_intro_frame=config_frame(text,actor,"anchor_4_intro_pose")?;
    let (turn_front_before_frame,turn_front_before_ms,turn_back_frame,turn_back_ms,turn_front_after_frame,turn_front_after_ms)=match locomotion {
        LocomotionFamily::Biped=>(
            config_frame(text,actor,"turn_front_before_pose")?,config_u64(text,"turn_front_before_ms")?,
            config_frame(text,actor,"turn_back_pose")?,config_u64(text,"turn_back_ms")?,
            config_frame(text,actor,"turn_front_after_pose")?,config_u64(text,"turn_front_after_ms")?,
        ),
        LocomotionFamily::Quadruped=>{
            if frame_count!=8 {return Err("QUADRUPED frame_count must be 8 (six walk + idle down/up)".to_string());}
            if walk_count!=6 {return Err("QUADRUPED walk_pose_count must be 6".to_string());}
            if anchor_4_intro_frame!=idle_up_frame {return Err("QUADRUPED anchor_4_intro_pose must equal idle_up_pose".to_string());}
            let turn_hold_ms=config_u64(text,"turn_hold_ms")?;
            if turn_hold_ms==0 {return Err("QUADRUPED turn_hold_ms must be positive".to_string());}
            (idle_up_frame,turn_hold_ms,idle_up_frame,0,idle_up_frame,0)
        }
    };
    let biped=BipedRoutine{
        walk_speed,walk_frames,walk_pose_ms,walk_count,
        walk_sequence_direction:parse_walk_direction(config_value(text,"walk_sequence_direction")?)?,
        idle_up_frame,idle_down_frame,idle_up_ms,idle_down_ms,
        linger_bounces_2:config_u64(text,"linger_bounces_anchor_2")?,
        linger_bounces_3:config_u64(text,"linger_bounces_anchor_3")?,
        anchor_4_intro_frame,
        anchor_4_intro_ms:config_u64(text,"anchor_4_intro_ms")?,
        anchor_4_bounces:config_u64(text,"anchor_4_idle_bounces")?,
        turn_front_before_frame,turn_front_before_ms,turn_back_frame,turn_back_ms,
        turn_front_after_frame,turn_front_after_ms,
        inter_actor_delay_ms:config_u64(text,"inter_actor_delay_ms")?,
    };
    Ok(ActorConfig{locomotion,scale,frame_count,registrations,biped})
}
fn embedded_actor_config(actor_idx:usize)->ActorConfig {
    parse_actor_config(actor_idx,ACTORS[actor_idx].config_text).expect("embedded cryptid config must be valid")
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
struct RotationConfig {
    actors:[usize;MAX_ROTATION],
    len:usize,
    looped:bool,
}
fn parse_rotation_config(text:&str)->Result<RotationConfig,String> {
    let mut actors_value:Option<&str>=None;
    let mut loop_value:Option<&str>=None;
    let mut seen=HashSet::new();
    for raw in text.lines() {
        let line=raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {continue;}
        if line.starts_with('[')&&line.ends_with(']') {
            if !line.eq_ignore_ascii_case("[rotation]") {return Err(format!("unknown rotation section {line}"));}
            continue;
        }
        let Some((k,v))=line.split_once('=') else {return Err(format!("invalid rotation config line: {line}"));};
        let key=k.trim();
        if !seen.insert(key.to_ascii_lowercase()) {return Err(format!("duplicate rotation config key {key}"));}
        match key.to_ascii_lowercase().as_str() {
            "actors"=>actors_value=Some(v.trim()),
            "loop"=>loop_value=Some(v.trim()),
            _=>return Err(format!("unknown rotation config key {key}")),
        }
    }
    let actor_list=actors_value.ok_or_else(||"missing rotation config key actors".to_string())?;
    let mut actors=[0usize;MAX_ROTATION];
    let mut len=0usize;
    for id in actor_list.split(',').map(str::trim) {
        if id.is_empty() {return Err("rotation actors list contains an empty entry".to_string());}
        if len>=MAX_ROTATION {return Err(format!("rotation supports at most {MAX_ROTATION} entries"));}
        actors[len]=actor_index_by_id(id).ok_or_else(||format!("unknown rotation actor {id}"))?;
        len+=1;
    }
    if len==0 {return Err("rotation actors list must not be empty".to_string());}
    let looped=match loop_value.ok_or_else(||"missing rotation config key loop".to_string())?.to_ascii_lowercase().as_str() {
        "true"=>true,
        "false"=>false,
        v=>return Err(format!("invalid rotation loop value {v}; use true or false")),
    };
    Ok(RotationConfig{actors,len,looped})
}
fn next_rotation_position(rotation:&RotationConfig,current:usize)->Option<usize> {
    if current+1<rotation.len {Some(current+1)}
    else if rotation.looped {Some(0)}
    else {None}
}

#[derive(Clone,Copy,Debug,PartialEq)]
struct Root {x:f64,y:f64}
impl Root {
    fn new(x:i32,y:i32)->Self {Self{x:x as f64,y:y as f64}}
    fn rounded(self)->(i32,i32) {(self.x.round() as i32,self.y.round() as i32)}
}

// Accepted native 960x600 Null Island route. Actor art never edits route geometry.
const A1:Root=Root{x:944.0,y:546.0};
const A2:Root=Root{x:840.0,y:561.0};
const A3:Root=Root{x:766.0,y:542.0};
const A4:Root=Root{x:617.0,y:552.0};

fn distance(a:Root,b:Root)->f64 {((b.x-a.x).powi(2)+(b.y-a.y).powi(2)).sqrt()}
fn lerp(a:Root,b:Root,t:f64)->Root {Root{x:a.x+(b.x-a.x)*t,y:a.y+(b.y-a.y)*t}}
fn ticks_for_ms(ms:u64)->u64 {ms.saturating_mul(FPS).saturating_add(999)/1000}
fn walk_ticks(r:&BipedRoutine,a:Root,b:Root)->u64 {(distance(a,b)/r.walk_speed*FPS as f64).ceil().max(1.0) as u64}
fn scaled_dimension(native:i32,scale:f64)->i32 {((native as f64*scale).round() as i32).max(1)}
fn scaled_coordinate(native:i32,scale:f64)->i32 {(native as f64*scale).round() as i32}

fn render_metrics(actor_idx:usize,config:&ActorConfig,frame:u8,facing:Facing)->(i32,i32,i32,i32) {
    let asset=ACTORS[actor_idx].frames[frame as usize];
    let reg=config.registrations[frame as usize];
    let w=scaled_dimension(asset.w,config.scale);
    let h=scaled_dimension(asset.h,config.scale);
    let left_anchor=scaled_coordinate(reg.anchor_x,config.scale).clamp(0,w-1);
    let anchor_x=match facing {Facing::Left=>left_anchor,Facing::Right=>w-1-left_anchor};
    let anchor_y=scaled_coordinate(reg.anchor_y,config.scale).clamp(0,h-1);
    (w,h,anchor_x,anchor_y)
}
fn offscreen_root(actor_idx:usize,config:&ActorConfig,facing:Facing)->Root {
    let max_anchor=(0..config.frame_count).map(|i|render_metrics(actor_idx,config,i as u8,facing).2).max().unwrap_or(0);
    Root::new(WIDTH as i32+max_anchor+1,A1.y as i32)
}

#[derive(Clone,Copy,Debug)]
struct Layout {
    enter:u64,walk_12:u64,idle_2:u64,walk_23:u64,idle_3:u64,walk_34:u64,
    pose_4:u64,turn_front:u64,turn_back:u64,turn_face_switch:u64,
    return_walk:u64,exit:u64,hold:u64,total:u64,
}
fn layout_for(actor_idx:usize,config:&ActorConfig)->Layout {
    let r=&config.biped;
    let enter_root=offscreen_root(actor_idx,config,Facing::Left);
    let exit_root=offscreen_root(actor_idx,config,Facing::Right);
    let enter=walk_ticks(r,enter_root,A1);
    let walk_12=walk_ticks(r,A1,A2);
    let idle_cycle_ms=r.idle_up_ms.saturating_add(r.idle_down_ms);
    let idle_2=ticks_for_ms(idle_cycle_ms.saturating_mul(r.linger_bounces_2));
    let walk_23=walk_ticks(r,A2,A3);
    let idle_3=ticks_for_ms(idle_cycle_ms.saturating_mul(r.linger_bounces_3));
    let walk_34=walk_ticks(r,A3,A4);
    let pose_4=ticks_for_ms(r.anchor_4_intro_ms).saturating_add(ticks_for_ms(idle_cycle_ms.saturating_mul(r.anchor_4_bounces)));
    let turn_front=ticks_for_ms(r.turn_front_before_ms);
    let turn_back=ticks_for_ms(r.turn_back_ms);
    let turn_face_switch=ticks_for_ms(r.turn_front_after_ms);
    let return_walk=walk_ticks(r,A4,A1);
    let exit=walk_ticks(r,A1,exit_root);
    let hold=ticks_for_ms(r.inter_actor_delay_ms);
    let total=[enter,walk_12,idle_2,walk_23,idle_3,walk_34,pose_4,turn_front,turn_back,turn_face_switch,return_walk,exit,hold].into_iter().fold(0u64,u64::saturating_add);
    Layout{enter,walk_12,idle_2,walk_23,idle_3,walk_34,pose_4,turn_front,turn_back,turn_face_switch,return_walk,exit,hold,total}
}
fn layout_segments(l:&Layout)->[(RouteState,u64);13] {[
    (RouteState::Enter,l.enter),(RouteState::WalkTo2,l.walk_12),(RouteState::IdleAt2,l.idle_2),
    (RouteState::WalkTo3,l.walk_23),(RouteState::IdleAt3,l.idle_3),(RouteState::WalkTo4,l.walk_34),
    (RouteState::PoseAt4,l.pose_4),(RouteState::TurnFront,l.turn_front),(RouteState::TurnBack,l.turn_back),
    (RouteState::TurnFaceSwitch,l.turn_face_switch),(RouteState::ReturnWalk,l.return_walk),
    (RouteState::Exit,l.exit),(RouteState::OffscreenHold,l.hold),
]}
fn state_start_duration(l:&Layout,target:RouteState)->(u64,u64) {
    let mut start=0;
    for (state,duration) in layout_segments(l) {
        if state==target {return (start,duration);}
        start+=duration;
    }
    unreachable!("all actor route states are represented")
}
fn locate_cycle_tick(l:&Layout,mut tick:u64)->(RouteState,u64,u64) {
    for (state,duration) in layout_segments(l) {
        if duration==0 {continue;}
        if tick<duration {return (state,tick,duration);}
        tick-=duration;
    }
    (RouteState::OffscreenHold,0,l.hold.max(1))
}
fn remap_cycle_tick(old:&Layout,new:&Layout,old_tick:u64)->u64 {
    let (state,local,old_duration)=locate_cycle_tick(old,old_tick);
    let (new_start,new_duration)=state_start_duration(new,state);
    if new_duration==0 {return new_start%new.total.max(1);}
    let fraction=local as f64/old_duration.max(1) as f64;
    let mapped=(fraction*new_duration as f64).floor() as u64;
    new_start+mapped.min(new_duration-1)
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
struct ContentSignature {len:u64,hash:u64}
fn content_signature(text:&str)->ContentSignature {
    // FNV-1a is sufficient here: this is only a cheap saved-file change detector,
    // not a trust/security boundary. Reading the bounded config content every
    // 500 ms avoids missing same-length edits when filesystem timestamp behavior
    // is coarse or an editor preserves metadata.
    let mut hash=0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash^=*byte as u64;
        hash=hash.wrapping_mul(0x100000001b3);
    }
    ContentSignature{len:text.len() as u64,hash}
}
fn resolve_runtime_path(relative:&str)->PathBuf {
    let launch=std::env::current_dir().unwrap_or_else(|_|PathBuf::from("."));
    let launch_path=launch.join(relative);
    if launch_path.exists() {return launch_path;}
    if let Ok(executable)=std::env::current_exe() {
        if let Some(dir)=executable.parent() {
            let beside=dir.join(relative);
            if beside.exists() {return beside;}
        }
    }
    launch_path
}
fn load_text_bounded(path:&Path)->Result<String,String> {
    let metadata=fs::metadata(path).map_err(|e|format!("{}: {e}",path.display()))?;
    if !metadata.is_file() {return Err(format!("{}: must be a regular file",path.display()));}
    if metadata.len()>MAX_CONFIG_BYTES {return Err(format!("{}: config exceeds 32 KiB",path.display()));}
    let text=fs::read_to_string(path).map_err(|e|format!("{}: {e}",path.display()))?;
    if text.len() as u64>MAX_CONFIG_BYTES {return Err(format!("{}: config exceeds 32 KiB",path.display()));}
    Ok(text)
}
fn load_text_signed(path:&Path)->Result<(String,ContentSignature),String> {
    let text=load_text_bounded(path)?;
    let signature=content_signature(&text);
    Ok((text,signature))
}
fn load_rotation_file(path:&Path)->Result<(RotationConfig,ContentSignature),String> {
    let (text,signature)=load_text_signed(path)?;
    Ok((parse_rotation_config(&text)?,signature))
}
fn load_actor_file(path:&Path,actor_idx:usize)->Result<(ActorConfig,ContentSignature),String> {
    let (text,signature)=load_text_signed(path)?;
    Ok((parse_actor_config(actor_idx,&text)?,signature))
}
fn actor_runtime_data(actor_idx:usize)->(PathBuf,ActorConfig,Layout,Option<ContentSignature>) {
    let path=resolve_runtime_path(ACTORS[actor_idx].config_path);
    let (config,signature)=load_actor_file(&path,actor_idx)
        .map(|(config,sig)|(config,Some(sig)))
        .unwrap_or_else(|_|(embedded_actor_config(actor_idx),None));
    let layout=layout_for(actor_idx,&config);
    (path,config,layout,signature)
}

#[derive(Clone,Copy)]
struct RuntimeSnapshot {
    actor_idx:usize,
    config:ActorConfig,
    layout:Layout,
    cycle_tick:u64,
    finished:bool,
}
struct ActorRuntime {
    rotation:RotationConfig,
    pending_rotation:Option<RotationConfig>,
    rotation_position:usize,
    rotation_path:PathBuf,
    rotation_signature:Option<ContentSignature>,
    actor_idx:usize,
    active:Arc<ActiveActor>,
    config:ActorConfig,
    layout:Layout,
    actor_path:PathBuf,
    actor_signature:Option<ContentSignature>,
    last_poll_tick:Option<u64>,
    actor_start_tick:Option<u64>,
    actor_cycle_base:u64,
    finished:bool,
}
fn runtime()->&'static Mutex<ActorRuntime> {
    static RUNTIME:OnceLock<Mutex<ActorRuntime>>=OnceLock::new();
    RUNTIME.get_or_init(||{
        let rotation_path=resolve_runtime_path(ROTATION_RELATIVE_PATH);
        let (rotation,rotation_signature)=load_rotation_file(&rotation_path)
            .map(|(rotation,sig)|(rotation,Some(sig)))
            .unwrap_or_else(|_|(parse_rotation_config(ROTATION_CONFIG).expect("embedded rotation config must be valid"),None));
        let rotation_position=0usize;
        let actor_idx=rotation.actors[rotation_position];
        let (actor_path,config,layout,actor_signature)=actor_runtime_data(actor_idx);
        let active=Arc::new(ActiveActor::load(actor_idx));
        Mutex::new(ActorRuntime{
            rotation,pending_rotation:None,rotation_position,rotation_path,rotation_signature,
            actor_idx,active,config,layout,actor_path,actor_signature,last_poll_tick:None,
            actor_start_tick:None,actor_cycle_base:0,finished:false,
        })
    })
}
fn current_cycle_tick(state:&ActorRuntime,tick:u64)->u64 {
    if state.finished {return state.layout.total.saturating_sub(1);}
    let start=state.actor_start_tick.unwrap_or(tick);
    tick.saturating_sub(start).saturating_add(state.actor_cycle_base)
}
fn install_actor_data(state:&mut ActorRuntime,actor_idx:usize,tick:u64,cycle_base:u64,
    actor_path:PathBuf,config:ActorConfig,layout:Layout,actor_signature:Option<ContentSignature>) {
    // RGBA ownership changes only once the final resume target is known. Skipped
    // actors are evaluated from config/layout metadata and are never materialized.
    let active=Arc::new(ActiveActor::load(actor_idx));
    state.actor_idx=actor_idx;
    state.active=active;
    state.config=config;
    state.layout=layout;
    state.actor_path=actor_path;
    state.actor_signature=actor_signature;
    state.actor_start_tick=Some(tick);
    state.actor_cycle_base=cycle_base;
    state.finished=false;
}
fn next_actor_after_boundary(state:&mut ActorRuntime)->Option<usize> {
    if let Some(next_rotation)=state.pending_rotation.take() {
        // A saved rotation edit takes effect only after the current actor has
        // fully exited and completed its own inter-actor delay.
        state.rotation=next_rotation;
        state.rotation_position=0;
        Some(state.rotation.actors[0])
    } else if let Some(next_position)=next_rotation_position(&state.rotation,state.rotation_position) {
        state.rotation_position=next_position;
        Some(state.rotation.actors[state.rotation_position])
    } else {None}
}
fn advance_completed_actors(state:&mut ActorRuntime,tick:u64) {
    if state.finished {return;}
    let current_total=state.layout.total.max(1);
    let cycle=current_cycle_tick(state,tick);
    if cycle<current_total {return;}
    let mut overshoot=cycle-current_total;
    let Some(mut actor_idx)=next_actor_after_boundary(state) else {
        state.finished=true;state.actor_start_tick=Some(tick);
        state.actor_cycle_base=state.layout.total.saturating_sub(1);return;
    };

    // A long hidden interval can span many rotations. Walk at most one complete
    // loop using metadata, reduce any additional whole loops arithmetically, then
    // load RGBA exactly once for the actor that is current at `tick`.
    let cycle_start=state.rotation_position;
    let cycle_len=state.rotation.len.max(1);
    let mut cycle_spent=0u64;let mut actors_seen=0usize;
    loop {
        let (actor_path,config,layout,actor_signature)=actor_runtime_data(actor_idx);
        let total=layout.total.max(1);
        if overshoot<total {
            install_actor_data(state,actor_idx,tick,overshoot,actor_path,config,layout,actor_signature);
            return;
        }
        overshoot-=total;cycle_spent=cycle_spent.saturating_add(total);actors_seen+=1;
        let Some(next_position)=next_rotation_position(&state.rotation,state.rotation_position) else {
            install_actor_data(state,actor_idx,tick,total.saturating_sub(1),actor_path,config,layout,actor_signature);
            state.finished=true;return;
        };
        state.rotation_position=next_position;
        actor_idx=state.rotation.actors[state.rotation_position];
        if state.rotation.looped&&actors_seen==cycle_len&&state.rotation_position==cycle_start&&cycle_spent>0 {
            overshoot%=cycle_spent;
            // After modulo, the target is guaranteed to lie within this one loop.
            cycle_spent=0;actors_seen=0;
        }
    }
}
fn snapshot_state(tick:u64)->(RuntimeSnapshot,Arc<ActiveActor>) {
    let mut state=runtime().lock().unwrap_or_else(|p|p.into_inner());
    if state.actor_start_tick.is_none() {state.actor_start_tick=Some(tick);}
    advance_completed_actors(&mut state,tick);

    let poll=state.last_poll_tick.map_or(true,|last|tick.saturating_sub(last)>=CONFIG_POLL_TICKS);
    if poll {
        state.last_poll_tick=Some(tick);
        if let Ok((next_rotation,rotation_sig))=load_rotation_file(&state.rotation_path) {
            if Some(rotation_sig)!=state.rotation_signature {
                // Rotation order is a queue contract. Do not replace an actor in
                // mid-routine; stage the valid queue and apply it at EXIT_COMPLETE
                // after this actor's own configured delay.
                state.pending_rotation=Some(next_rotation);
                state.rotation_signature=Some(rotation_sig);
            }
        }
        if !state.finished {
            if let Ok((next_config,actor_sig))=load_actor_file(&state.actor_path,state.actor_idx) {
                if Some(actor_sig)!=state.actor_signature {
                    let next_layout=layout_for(state.actor_idx,&next_config);
                    let old_cycle=current_cycle_tick(&state,tick).min(state.layout.total.saturating_sub(1));
                    let new_cycle=remap_cycle_tick(&state.layout,&next_layout,old_cycle);
                    state.actor_start_tick=Some(tick);
                    state.actor_cycle_base=new_cycle;
                    state.config=next_config;
                    state.layout=next_layout;
                    // Only acknowledge the saved content after it parsed and validated.
                    state.actor_signature=Some(actor_sig);
                }
            }
        }
    }
    advance_completed_actors(&mut state,tick);
    let cycle_tick=current_cycle_tick(&state,tick).min(state.layout.total.saturating_sub(1));
    let snapshot=RuntimeSnapshot{
        actor_idx:state.actor_idx,config:state.config,layout:state.layout,
        cycle_tick,finished:state.finished,
    };
    (snapshot,Arc::clone(&state.active))
}
fn snapshot(tick:u64)->RuntimeSnapshot {snapshot_state(tick).0}

#[derive(Clone,Copy,Debug)]
pub struct ActorVisual {
    pub state:RouteState,
    pub root_x:i32,
    pub root_y:i32,
    frame:u8,
    pub facing:Facing,
    actor_idx:usize,
}

fn walk_frame_for_elapsed_ms(r:&BipedRoutine,elapsed_ms:f64)->u8 {
    let cycle_ms=r.walk_pose_ms[..r.walk_count].iter().sum::<f64>();
    let phase=elapsed_ms.rem_euclid(cycle_ms);
    let mut edge=0.0;
    match r.walk_sequence_direction {
        WalkSequenceDirection::Forward=>{
            for i in 0..r.walk_count {edge+=r.walk_pose_ms[i];if phase<edge{return r.walk_frames[i];}}
            r.walk_frames[r.walk_count-1]
        }
        WalkSequenceDirection::Reverse=>{
            for i in (0..r.walk_count).rev() {edge+=r.walk_pose_ms[i];if phase<edge{return r.walk_frames[i];}}
            r.walk_frames[0]
        }
    }
}
fn walk_frame(r:&BipedRoutine,elapsed_walk_ticks:u64)->u8 {
    walk_frame_for_elapsed_ms(r,elapsed_walk_ticks as f64*1000.0/FPS as f64)
}
fn idle_frame(r:&BipedRoutine,local_tick:u64)->u8 {
    let cycle_ms=r.idle_up_ms.saturating_add(r.idle_down_ms);
    let elapsed_ms=(local_tick*1000/FPS)%cycle_ms;
    if elapsed_ms<r.idle_up_ms {r.idle_up_frame}else{r.idle_down_frame}
}
fn walk_visual(actor_idx:usize,r:&BipedRoutine,state:RouteState,a:Root,b:Root,local:u64,duration:u64,walk_tick_offset:u64,facing:Facing)->ActorVisual {
    let t=local as f64/duration.max(1) as f64;
    let (root_x,root_y)=lerp(a,b,t).rounded();
    ActorVisual{state,root_x,root_y,frame:walk_frame(r,walk_tick_offset+local),facing,actor_idx}
}
fn visual_at_cycle_tick(mut t:u64,s:&RuntimeSnapshot)->ActorVisual {
    let r=&s.config.biped;
    let l=&s.layout;
    let enter_root=offscreen_root(s.actor_idx,&s.config,Facing::Left);
    let exit_root=offscreen_root(s.actor_idx,&s.config,Facing::Right);
    if s.finished {
        let (root_x,root_y)=exit_root.rounded();
        return ActorVisual{state:RouteState::OffscreenHold,root_x,root_y,frame:r.walk_frames[0],facing:Facing::Right,actor_idx:s.actor_idx};
    }
    macro_rules! state {($duration:expr,$body:expr)=>{{let duration=$duration;if t<duration{return $body;}t-=duration;}};}
    state!(l.enter,walk_visual(s.actor_idx,r,RouteState::Enter,enter_root,A1,t,l.enter,0,Facing::Left));
    state!(l.walk_12,walk_visual(s.actor_idx,r,RouteState::WalkTo2,A1,A2,t,l.walk_12,l.enter,Facing::Left));
    state!(l.idle_2,ActorVisual{state:RouteState::IdleAt2,root_x:A2.x as i32,root_y:A2.y as i32,frame:idle_frame(r,t),facing:Facing::Left,actor_idx:s.actor_idx});
    state!(l.walk_23,walk_visual(s.actor_idx,r,RouteState::WalkTo3,A2,A3,t,l.walk_23,l.enter+l.walk_12,Facing::Left));
    state!(l.idle_3,ActorVisual{state:RouteState::IdleAt3,root_x:A3.x as i32,root_y:A3.y as i32,frame:idle_frame(r,t),facing:Facing::Left,actor_idx:s.actor_idx});
    state!(l.walk_34,walk_visual(s.actor_idx,r,RouteState::WalkTo4,A3,A4,t,l.walk_34,l.enter+l.walk_12+l.walk_23,Facing::Left));
    let intro_ticks=ticks_for_ms(r.anchor_4_intro_ms);
    state!(l.pose_4,ActorVisual{state:RouteState::PoseAt4,root_x:A4.x as i32,root_y:A4.y as i32,frame:if t<intro_ticks{r.anchor_4_intro_frame}else{idle_frame(r,t-intro_ticks)},facing:Facing::Left,actor_idx:s.actor_idx});
    state!(l.turn_front,ActorVisual{state:RouteState::TurnFront,root_x:A4.x as i32,root_y:A4.y as i32,frame:r.turn_front_before_frame,facing:Facing::Left,actor_idx:s.actor_idx});
    state!(l.turn_back,ActorVisual{state:RouteState::TurnBack,root_x:A4.x as i32,root_y:A4.y as i32,frame:r.turn_back_frame,facing:Facing::Left,actor_idx:s.actor_idx});
    state!(l.turn_face_switch,ActorVisual{state:RouteState::TurnFaceSwitch,root_x:A4.x as i32,root_y:A4.y as i32,frame:r.turn_front_after_frame,facing:Facing::Right,actor_idx:s.actor_idx});
    state!(l.return_walk,walk_visual(s.actor_idx,r,RouteState::ReturnWalk,A4,A1,t,l.return_walk,l.enter+l.walk_12+l.walk_23+l.walk_34,Facing::Right));
    state!(l.exit,walk_visual(s.actor_idx,r,RouteState::Exit,A1,exit_root,t,l.exit,l.enter+l.walk_12+l.walk_23+l.walk_34+l.return_walk,Facing::Right));
    let (root_x,root_y)=exit_root.rounded();
    ActorVisual{state:RouteState::OffscreenHold,root_x,root_y,frame:r.walk_frames[0],facing:Facing::Right,actor_idx:s.actor_idx}
}

fn sprite_bounds(config:&ActorConfig,v:ActorVisual)->(i32,i32,i32,i32) {
    let (w,h,anchor_x,anchor_y)=render_metrics(v.actor_idx,config,v.frame,v.facing);
    (v.root_x-anchor_x,v.root_y-anchor_y,w,h)
}
fn overlaps_screen(r:(i32,i32,i32,i32))->bool {r.0<WIDTH as i32&&r.1<HEIGHT as i32&&r.0+r.2>0&&r.1+r.3>0}
fn label_bounds(config:&ActorConfig,v:ActorVisual)->Option<(i32,i32,i32,i32)> {
    let sprite=sprite_bounds(config,v);
    if !overlaps_screen(sprite) {return None;}
    let label=ACTORS[v.actor_idx].label;
    let width=(label.chars().count() as i32*6-1).max(5);
    Some((sprite.0+sprite.2/2-width/2,sprite.1-LABEL_VERTICAL_GAP_PX-7,width,7))
}
fn union_rect(a:(i32,i32,i32,i32),b:(i32,i32,i32,i32))->(i32,i32,i32,i32) {
    let x=a.0.min(b.0);let y=a.1.min(b.1);
    let right=(a.0+a.2).max(b.0+b.2);let bottom=(a.1+a.3).max(b.1+b.3);
    (x,y,right-x,bottom-y)
}
fn clip_rect(r:(i32,i32,i32,i32))->Option<(i32,i32,i32,i32)> {
    let x0=r.0.max(0);let y0=r.1.max(0);
    let x1=(r.0+r.2).min(WIDTH as i32);let y1=(r.1+r.3).min(HEIGHT as i32);
    (x1>x0&&y1>y0).then_some((x0,y0,x1-x0,y1-y0))
}
fn visual_damage(config:&ActorConfig,v:ActorVisual)->Option<(i32,i32,i32,i32)> {
    let sprite=sprite_bounds(config,v);
    if !overlaps_screen(sprite) {return None;}
    clip_rect(label_bounds(config,v).map_or(sprite,|label|union_rect(sprite,label)))
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct VisualKey {damage:Option<(i32,i32,i32,i32)>}
pub fn visual_key(tick:u64)->VisualKey {
    let s=snapshot(tick);
    let v=visual_at_cycle_tick(s.cycle_tick,&s);
    VisualKey{damage:visual_damage(&s.config,v)}
}
pub fn damage_bounds(key:VisualKey)->Option<(i32,i32,i32,i32)> {key.damage}
pub fn damage_delta(old:Option<VisualKey>,new:VisualKey)->[Option<(i32,i32,i32,i32)>;2] {[old.and_then(damage_bounds),damage_bounds(new)]}

fn composite_rgba(c:&mut Canvas,x:i32,y:i32,rgba:[u8;4]) {
    if x<0||y<0||x>=WIDTH as i32||y>=HEIGHT as i32{return;}
    if !c.touched(x as usize,y as usize){return;}
    let alpha=rgba[3] as u32;if alpha==0{return;}
    let stride=c.w*c.scale;let at=y as usize*stride+x as usize;
    let rgb=((rgba[0] as u32)<<16)|((rgba[1] as u32)<<8)|rgba[2] as u32;
    if alpha==255{c.cells[at]=rgb;return;}
    let base=c.cells[at];
    let channel=|shift:u32|->u32{let src=(rgb>>shift)&255;let dst=(base>>shift)&255;(src*alpha+dst*(255-alpha)+127)/255};
    c.cells[at]=(channel(16)<<16)|(channel(8)<<8)|channel(0);
}

pub fn draw(c:&mut Canvas,tick:u64) {
    let (s,active)=snapshot_state(tick);
    let v=visual_at_cycle_tick(s.cycle_tick,&s);
    let asset=ACTORS[v.actor_idx].frames[v.frame as usize];
    debug_assert_eq!(active.actor_idx,v.actor_idx);
    let data=active.frame(v.frame as usize);
    let bounds=sprite_bounds(&s.config,v);
    if !overlaps_screen(bounds)||!c.overlaps(bounds.0,bounds.1,bounds.2,bounds.3){return;}
    debug_assert_eq!(data.len(),(asset.w*asset.h*4) as usize);
    for dy in 0..bounds.3 {
        let y=bounds.1+dy;if y<0||y>=HEIGHT as i32{continue;}
        let sy=(dy*asset.h/bounds.3).min(asset.h-1);
        for dx in 0..bounds.2 {
            let x=bounds.0+dx;if x<0||x>=WIDTH as i32||!c.touched(x as usize,y as usize){continue;}
            let native_x=(dx*asset.w/bounds.2).min(asset.w-1);
            let sx=match v.facing{Facing::Left=>native_x,Facing::Right=>asset.w-1-native_x};
            let at=((sy*asset.w+sx)*4) as usize;
            composite_rgba(c,x,y,[data[at],data[at+1],data[at+2],data[at+3]]);
        }
    }
}
pub fn draw_label(c:&mut Canvas,tick:u64) {
    let s=snapshot(tick);
    let v=visual_at_cycle_tick(s.cycle_tick,&s);
    if let Some((x,y,_,_))=label_bounds(&s.config,v) {
        crate::billboard::overlay_text(c,x,y,ACTORS[v.actor_idx].label,1,crate::constellation_render::NAME_RGB);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start_ticks(l:&Layout)->Vec<(RouteState,u64)> {
        let mut t=0;let mut out=Vec::new();
        for (state,d) in layout_segments(l){out.push((state,t));t+=d;}
        out
    }
    fn actor(id:&str)->(usize,ActorConfig) {
        let idx=actor_index_by_id(id).unwrap();(idx,embedded_actor_config(idx))
    }

    #[test]
    fn long_resume_seeks_rotation_without_resetting_to_cycle_zero() {
        let rotation=parse_rotation_config(ROTATION_CONFIG).unwrap();
        let rotation_position=0usize;let actor_idx=rotation.actors[rotation_position];
        let (actor_path,config,layout,actor_signature)=actor_runtime_data(actor_idx);
        let active=Arc::new(ActiveActor::load(actor_idx));
        let rotation_path=resolve_runtime_path(ROTATION_RELATIVE_PATH);
        let mut state=ActorRuntime{rotation,pending_rotation:None,rotation_position,rotation_path,rotation_signature:None,actor_idx,active,config,layout,actor_path,actor_signature,last_poll_tick:None,actor_start_tick:Some(0),actor_cycle_base:0,finished:false};
        let cycle_total=(0..state.rotation.len).map(|pos|{let idx=state.rotation.actors[pos];let (_,cfg,_,_)=actor_runtime_data(idx);layout_for(idx,&cfg).total.max(1)}).sum::<u64>();
        let jump=state.layout.total.max(1).saturating_add(cycle_total.saturating_mul(1000)).saturating_add(37);
        advance_completed_actors(&mut state,jump);
        assert_eq!(state.rotation_position,1);
        assert_eq!(state.actor_idx,state.rotation.actors[1]);
        assert_eq!(state.actor_cycle_base,37);
        assert!(!state.finished);
    }

    #[test]
    fn independent_actor_configs_and_ordered_rotation_parse() {
        let dog=embedded_actor_config(0);let london=embedded_actor_config(1);let baa=embedded_actor_config(2);let chupa=embedded_actor_config(3);
        let rotation=parse_rotation_config(ROTATION_CONFIG).unwrap();
        assert_eq!(rotation.len,4);
        assert_eq!(&rotation.actors[..rotation.len],&[0,1,2,3]);
        assert!(rotation.looped);
        assert_eq!(dog.scale,0.5);assert_eq!(london.scale,0.5);assert_eq!(baa.scale,0.5);assert_eq!(chupa.scale,0.5);
        assert_eq!(dog.biped.inter_actor_delay_ms,3000);
        assert_eq!(london.biped.inter_actor_delay_ms,3000);
        assert_eq!(baa.biped.inter_actor_delay_ms,3000);assert_eq!(chupa.biped.inter_actor_delay_ms,3000);
        assert_eq!(dog.frame_count,8);assert_eq!(london.frame_count,8);assert_eq!(baa.frame_count,8);assert_eq!(chupa.frame_count,8);
        assert_eq!(baa.locomotion,LocomotionFamily::Quadruped);
        assert_eq!(baa.biped.walk_count,6);
        assert_eq!(dog.registrations[0],FrameRegistration{anchor_x:182,anchor_y:464});
        assert_eq!(london.registrations[0],FrameRegistration{anchor_x:200,anchor_y:469});
        assert_eq!(london.registrations[7],FrameRegistration{anchor_x:142,anchor_y:465});
        assert_eq!(baa.registrations[0],FrameRegistration{anchor_x:216,anchor_y:294});
        assert_eq!(baa.registrations[7],FrameRegistration{anchor_x:191,anchor_y:361});
        assert_eq!(chupa.registrations[0],FrameRegistration{anchor_x:230,anchor_y:378});
        assert_eq!(chupa.registrations[7],FrameRegistration{anchor_x:221,anchor_y:354});
        assert_eq!(CONFIG_POLL_TICKS,15);
        assert_eq!([(A1.x as i32,A1.y as i32),(A2.x as i32,A2.y as i32),(A3.x as i32,A3.y as i32),(A4.x as i32,A4.y as i32)],[(944,546),(840,561),(766,542),(617,552)]);
    }

    #[test]
    fn rotation_order_is_literal_and_duplicate_entries_are_weighted() {
        let rotation=parse_rotation_config("[rotation]\nactors=dogman,dogman,dogman,london_werewolf\nloop=true\n").unwrap();
        assert_eq!(rotation.len,4);
        assert_eq!(&rotation.actors[..rotation.len],&[0,0,0,1]);
        assert!(rotation.looped);
        assert_eq!(next_rotation_position(&rotation,0),Some(1));
        assert_eq!(next_rotation_position(&rotation,1),Some(2));
        assert_eq!(next_rotation_position(&rotation,2),Some(3));
        assert_eq!(next_rotation_position(&rotation,3),Some(0));
        let once=parse_rotation_config("[rotation]\nactors=dogman,london_werewolf\nloop=false\n").unwrap();
        assert_eq!(next_rotation_position(&once,1),None);
        assert!(parse_rotation_config("[rotation]\nactors=dogman,nope\nloop=true\n").is_err());
        assert!(parse_rotation_config("[rotation]\nactors=dogman,london_werewolf\nloop=maybe\n").is_err());
    }

    #[test]
    fn inter_actor_delay_belongs_to_actor_layout() {
        let (dog_idx,dog)=actor("dogman");
        let (london_idx,london)=actor("london_werewolf");
        let (baa_idx,baa)=actor("baa");let (chupa_idx,chupa)=actor("chupacabra");
        assert_eq!(layout_for(dog_idx,&dog).hold,ticks_for_ms(dog.biped.inter_actor_delay_ms));
        assert_eq!(layout_for(london_idx,&london).hold,ticks_for_ms(london.biped.inter_actor_delay_ms));
        assert_eq!(layout_for(baa_idx,&baa).hold,ticks_for_ms(baa.biped.inter_actor_delay_ms));assert_eq!(layout_for(chupa_idx,&chupa).hold,ticks_for_ms(chupa.biped.inter_actor_delay_ms));
    }

    #[test]
    fn embedded_frame_lengths_match_registration_dimensions() {
        for actor in ACTORS {
            for frame in actor.frames {assert_eq!(frame.data.len(),(frame.w*frame.h*4) as usize);}
        }
    }

    #[test]
    fn active_actor_owns_only_one_actor_payload_with_exact_frame_slices() {
        for idx in 0..ACTORS.len() {
            let active=ActiveActor::load(idx);
            let expected=ACTORS[idx].frames.iter().map(|frame|frame.data.len()).sum::<usize>();
            assert_eq!(active.actor_idx,idx);
            assert_eq!(active.bytes.len(),expected);
            for (frame_idx,embedded) in ACTORS[idx].frames.iter().enumerate() {
                assert_eq!(active.frame(frame_idx).len(),embedded.data.len());
                assert_eq!(active.frame(frame_idx),embedded.data);
            }
        }
    }

    #[test]
    fn walk_sequence_is_data_driven_not_fixed_to_four() {
        let (idx,_)=actor("dogman");
        let three=ACTORS[idx].config_text.replace("walk_pose_count=4","walk_pose_count=3");
        let cfg=parse_actor_config(idx,&three).unwrap();
        assert_eq!(cfg.biped.walk_count,3);
        assert_eq!(walk_frame_for_elapsed_ms(&cfg.biped,0.0),0);
        assert_eq!(walk_frame_for_elapsed_ms(&cfg.biped,500.0),1);
        assert_eq!(walk_frame_for_elapsed_ms(&cfg.biped,1000.0),2);
        assert_eq!(walk_frame_for_elapsed_ms(&cfg.biped,1500.0),0);
    }

    #[test]
    fn scale_and_mirror_keep_world_root_invariant_for_every_actor_frame() {
        let root=(700,550);
        for idx in 0..ACTORS.len() {
            let cfg=embedded_actor_config(idx);
            for frame in 0..cfg.frame_count {
                for facing in [Facing::Left,Facing::Right] {
                    let v=ActorVisual{state:RouteState::PoseAt4,root_x:root.0,root_y:root.1,frame:frame as u8,facing,actor_idx:idx};
                    let b=sprite_bounds(&cfg,v);let (_,_,ax,ay)=render_metrics(idx,&cfg,frame as u8,facing);
                    assert_eq!((b.0+ax,b.1+ay),root);
                }
            }
        }
    }

    #[test]
    fn accepted_route_starts_land_on_native_anchors_for_all_actors() {
        for id in ["dogman","london_werewolf","baa","chupacabra"] {
            let (idx,cfg)=actor(id);let l=layout_for(idx,&cfg);let starts=start_ticks(&l);
            let s=RuntimeSnapshot{actor_idx:idx,config:cfg,layout:l,cycle_tick:0,finished:false};
            let at=|state|starts.iter().find(|p|p.0==state).unwrap().1;
            let v=visual_at_cycle_tick(at(RouteState::WalkTo2),&s);assert_eq!((v.root_x,v.root_y),(944,546));
            let v=visual_at_cycle_tick(at(RouteState::IdleAt2),&s);assert_eq!((v.root_x,v.root_y),(840,561));
            let v=visual_at_cycle_tick(at(RouteState::IdleAt3),&s);assert_eq!((v.root_x,v.root_y),(766,542));
            let v=visual_at_cycle_tick(at(RouteState::PoseAt4),&s);assert_eq!((v.root_x,v.root_y),(617,552));
        }
    }

    #[test]
    fn quadruped_uses_six_walks_idle_turnaround_and_no_biped_turn_poses() {
        let (idx,cfg)=actor("baa");
        assert_eq!(cfg.locomotion,LocomotionFamily::Quadruped);
        assert_eq!(cfg.biped.walk_count,6);
        assert_eq!(&cfg.biped.walk_frames[..6],&[0,1,2,3,4,5]);
        assert_eq!(cfg.biped.idle_down_frame,6);
        assert_eq!(cfg.biped.idle_up_frame,7);
        assert_eq!(cfg.biped.anchor_4_intro_frame,7);
        let layout=layout_for(idx,&cfg);
        assert_eq!(layout.turn_front,ticks_for_ms(500));
        assert_eq!(layout.turn_back,0);
        assert_eq!(layout.turn_face_switch,0);
        let starts=start_ticks(&layout);
        let at=|state|starts.iter().find(|p|p.0==state).unwrap().1;
        let snap=RuntimeSnapshot{actor_idx:idx,config:cfg,layout,cycle_tick:0,finished:false};
        let hold=visual_at_cycle_tick(at(RouteState::TurnFront),&snap);
        assert_eq!(hold.frame,7);assert_eq!(hold.facing,Facing::Left);
        let returning=visual_at_cycle_tick(at(RouteState::ReturnWalk),&snap);
        assert_eq!(returning.facing,Facing::Right);
    }

    #[test]
    fn second_quadruped_chupacabra_uses_same_standardized_contract() {
        let (idx,cfg)=actor("chupacabra");
        assert_eq!(cfg.locomotion,LocomotionFamily::Quadruped);
        assert_eq!(cfg.biped.walk_count,6);
        assert_eq!(&cfg.biped.walk_frames[..6],&[0,1,2,3,4,5]);
        assert_eq!(cfg.biped.idle_down_frame,6);
        assert_eq!(cfg.biped.idle_up_frame,7);
        assert_eq!(cfg.biped.anchor_4_intro_frame,7);
        let layout=layout_for(idx,&cfg);
        assert_eq!(layout.turn_front,ticks_for_ms(500));
        assert_eq!(layout.turn_back,0);
        assert_eq!(layout.turn_face_switch,0);
    }

    #[test]
    fn malformed_quadruped_config_is_rejected() {
        let (idx,_)=actor("baa");
        assert!(parse_actor_config(idx,&ACTORS[idx].config_text.replace("walk_pose_count=6","walk_pose_count=5")).is_err());
        let (cidx,_)=actor("chupacabra");
        assert!(parse_actor_config(cidx,&ACTORS[cidx].config_text.replace("anchor_4_intro_pose=IDLE_UP","anchor_4_intro_pose=IDLE_DOWN")).is_err());
        assert!(parse_actor_config(idx,&ACTORS[idx].config_text.replace("anchor_4_intro_pose=IDLE_UP","anchor_4_intro_pose=IDLE_DOWN")).is_err());
        assert!(parse_actor_config(idx,&ACTORS[idx].config_text.replace("turn_hold_ms=500","turn_hold_ms=nope")).is_err());
    }

    #[test]
    fn scale_changes_sprite_metrics_not_route_root() {
        let (idx,mut cfg)=actor("london_werewolf");
        let v=ActorVisual{state:RouteState::PoseAt4,root_x:617,root_y:552,frame:0,facing:Facing::Left,actor_idx:idx};
        let a=sprite_bounds(&cfg,v);cfg.scale=0.31;let b=sprite_bounds(&cfg,v);
        assert!(b.2<a.2&&b.3<a.3);
        let (_,_,ax,ay)=render_metrics(idx,&cfg,0,Facing::Left);
        assert_eq!((b.0+ax,b.1+ay),(617,552));
    }

    #[test]
    fn same_length_scale_edits_have_distinct_content_signatures() {
        let a="actor_id=london_werewolf\nscale=0.500\n";
        let b="actor_id=london_werewolf\nscale=0.800\n";
        assert_eq!(a.len(),b.len());
        assert_ne!(content_signature(a),content_signature(b));
    }

    #[test]
    fn malformed_config_is_rejected_without_panicking() {
        let (idx,_)=actor("london_werewolf");
        assert!(parse_actor_config(idx,&ACTORS[idx].config_text.replace("scale=0.500","scale=0")).is_err());
        assert!(parse_actor_config(idx,&ACTORS[idx].config_text.replace("frame.7.anchor_y=465","frame.7.anchor_y=9999")).is_err());
        assert!(parse_actor_config(idx,&ACTORS[idx].config_text.replace("walk_speed_px_per_second=48.0","walk_speed_px_per_second=-1")).is_err());
        assert!(parse_actor_config(idx,&ACTORS[idx].config_text.replace("locomotion=BIPED","locomotion=QUADRUPED")).is_err());
        assert!(parse_actor_config(idx,&ACTORS[idx].config_text.replace("walk_pose_00=WALK_00","walk_pose_00=NOPE")).is_err());
    }

    #[test]
    fn live_reload_phase_mapping_preserves_current_route_state() {
        let (idx,original)=actor("dogman");
        let changed=parse_actor_config(idx,&ACTORS[idx].config_text.replace("walk_speed_px_per_second=48.0","walk_speed_px_per_second=12.0")).unwrap();
        let old_layout=layout_for(idx,&original);let new_layout=layout_for(idx,&changed);
        let old_start=state_start_duration(&old_layout,RouteState::WalkTo3).0;
        let old_tick=old_start+old_layout.walk_23/2;
        let mapped=remap_cycle_tick(&old_layout,&new_layout,old_tick);
        let (state,local,duration)=locate_cycle_tick(&new_layout,mapped);
        assert_eq!(state,RouteState::WalkTo3);
        assert!((local as f64/duration as f64-0.5).abs()<0.05);
    }
}
