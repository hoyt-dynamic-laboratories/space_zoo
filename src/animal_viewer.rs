//! Isolated visual review using the exact runtime atlases, no animal scheduling.
use crate::living_animals::{self,atlas::*,Clip};
use crate::pixel::Canvas;
use minifb::{Key,KeyRepeat,Window,WindowOptions};
use std::time::{Duration,Instant};

pub fn run()->Result<(),Box<dyn std::error::Error>> {
    let roster:[(&str,[&Clip;4]);14]=[
        ("German shepherd",[&DOG_WALK,&DOG_SIT,&DOG_REST,&DOG_LOOK]),
        ("Cat",[&CAT_WALK,&CAT_IDLE,&CAT_FORAGE,&CAT_SCRATCH]),
        ("Chicken",[&CHICKEN_WALK,&CHICKEN_IDLE,&CHICKEN_FORAGE,&CHICKEN_DUST]),
        ("Guinea fowl",[&GUINEA_WALK,&GUINEA_LOOK,&GUINEA_FORAGE,&GUINEA_REST]),
        ("Keet",[&KEET_WALK,&KEET_IDLE,&KEET_FORAGE,&KEET_REST]),
        ("Stag",[&STAG_WALK,&STAG_LOOK,&STAG_FORAGE,&STAG_REST]),
        ("Doe",[&DOE_WALK,&DOE_LOOK,&DOE_FORAGE,&DOE_REST]),
        ("Fawn",[&FAWN_WALK,&FAWN_LOOK,&FAWN_FORAGE,&FAWN_REST]),
        ("Sasquatch",[&SASQUATCH_WALK,&SASQUATCH_IDLE,&SASQUATCH_WALK,&SASQUATCH_IDLE]),
        ("Chupacabra",[&CHUPACABRA_WALK,&CHUPACABRA_IDLE,&CHUPACABRA_WALK,&CHUPACABRA_IDLE]),
        ("Cardinal",[&CARDINAL_FLY,&CARDINAL_IDLE,&CARDINAL_FLY,&CARDINAL_IDLE]),
        ("Blue jay",[&BLUEJAY_FLY,&BLUEJAY_IDLE,&BLUEJAY_FLY,&BLUEJAY_IDLE]),
        ("Blue heron",[&HERON_FLY,&HERON_IDLE,&HERON_FLY,&HERON_IDLE]),
        ("Bat",[&BAT_FLY,&BAT_REST,&BAT_FLY,&BAT_REST]),
    ];
    let mut window=Window::new("Space Zoo .443 animals",960,600,WindowOptions::default())?;
    let mut c=Canvas::scaled(320,200,3,false);
    let mut detail=Canvas::scaled(60,34,6,false);
    let (mut selected,mut action,mut tick)=(0usize,0usize,0u64);
    let (mut mirror,mut paused)=(false,false);
    let mut changed=true;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let start=Instant::now();
        for key in window.get_keys_pressed(KeyRepeat::No) {
            match key {
                Key::Right=>{selected=(selected+1)%roster.len();changed=true;},
                Key::Left=>{selected=(selected+roster.len()-1)%roster.len();changed=true;},
                Key::C=>{action=(action+1)%4;tick=0;changed=true;},
                Key::F=>{mirror=!mirror;},Key::Space=>{paused=!paused;},
                _=>{},
            }
        }
        if changed {
            window.set_title(&format!(".443 {} | action {}/4 | Left/Right animal | C action | F facing | Space pause | Esc quit",roster[selected].0,action+1));
            changed=false;
        }
        c.clear(0x14212a);
        c.rect(0,121,320,1,0x526653);
        c.rect(0,122,320,78,0x1e2d28);
        let clip=roster[selected].1[action];
        let frame=(tick/3) as usize;
        // Native-size view at left. A 2x nearest-neighbor inspection at right.
        let x=55-clip.w as i32/6;let y=121-clip.h as i32/3;
        living_animals::draw(&mut c,clip,x,y,mirror,frame,255);
        detail.clear(0);
        living_animals::draw(&mut detail,clip,0,0,mirror,frame,255);
        let stride=detail.w*detail.scale;let dw=clip.w*2;let dh=clip.h*2;
        for yy in 0..dh {for xx in 0..dw {
            let rgb=detail.cells[yy*stride+xx];
            if rgb!=0 {c.native_put(690-dw as i32/2+xx as i32,363-dh as i32+yy as i32,rgb);}
        }}
        window.update_with_buffer(&c.cells,960,600)?;
        if !paused {tick+=1;}
        if let Some(wait)=Duration::from_millis(33).checked_sub(start.elapsed()) {std::thread::sleep(wait);}
    }
    Ok(())
}
