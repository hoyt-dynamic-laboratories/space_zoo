//! .443 articulated native-pixel animation. Assets are baked offline.
//! No decoding library, allocation, floating point, or file IO in the draw loop.
use crate::pixel::Canvas;
use crate::npc::{Animal,Facing};
use crate::sprite::{Species,Pose};
pub struct Clip {pub w:usize,pub h:usize,pub frames:usize,pub order:[usize;8],pub colors:&'static [u32],pub data:&'static [u8]}
#[path="living_atlas.rs"] pub mod atlas;

pub fn draw(c:&mut Canvas,clip:&Clip,x:i32,y:i32,mirror:bool,frame:usize,palette:u8) {
    let frame=frame%clip.frames;
    let s=c.scale as i32;
    let dw=clip.w as i32*s/3;let dh=clip.h as i32*s/3;
    let left=x*s;let top=y*s;
    if left+dw<=0 || top+dh<=0 || left>= (c.w*c.scale) as i32 || top>= (c.h*c.scale) as i32 ||
        !c.overlaps(left,top,dw,dh) {return;}
    let start=clip.order[frame]*clip.w*clip.h;
    for dy in 0..dh {
        let sy=(dy*3/s) as usize;
        for dx in 0..dw {
            let sx=(dx*3/s) as usize;
            let sx=if mirror {clip.w-1-sx} else {sx};
            let i=start+sy*clip.w+sx;
            let mut rgb=clip.colors[clip.data[i] as usize];
            if rgb==0 {continue;}
            // Retain each hen's existing stable feather color identity.
            if palette<7 {
                const FEATHERS:[[u32;3];7]=[
                    [0xe6d5ae,0xfff2d5,0xac8e67],[0xd6a65b,0xf6d493,0x956c3d],
                    [0xb56849,0xe5a87a,0x774735],[0x626675,0xa2a6af,0x3f4350],
                    [0xb6b6aa,0xeeeadd,0x747b74],[0xc18a54,0xf0c788,0x855c39],
                    [0x8e6250,0xc99877,0x604339]];
                rgb=match rgb {0xe6d5ae=>FEATHERS[palette as usize][0],
                    0xfff2d5=>FEATHERS[palette as usize][1],0xac8e67=>FEATHERS[palette as usize][2],_=>rgb};
            }
            c.native_put(left+dx,top+dy,rgb);
        }
    }
}

fn chosen_clip(a:&Animal,tick:u64)->&'static Clip {
    use atlas::*;
    let pose=a.pose(tick);
    let walk=matches!(pose,Pose::WalkA|Pose::WalkB|Pose::Hop);
    let mut clip: &'static Clip=match a.species {
        Species::Shepherd=>if walk {&DOG_WALK} else {match pose {
            Pose::Rest=>&DOG_REST,Pose::Look=>&DOG_LOOK,_=>&DOG_SIT}},
        Species::Cat=>if walk {&CAT_WALK} else {match pose {
            Pose::Forage=>&CAT_FORAGE,Pose::Scratch=>&CAT_SCRATCH,Pose::Rest=>&CAT_REST,_=>&CAT_IDLE}},
        Species::Chicken=>if walk {&CHICKEN_WALK} else {match pose {
            Pose::Look=>&CHICKEN_LOOK,Pose::Forage=>&CHICKEN_FORAGE,Pose::DustBathe=>&CHICKEN_DUST,
            Pose::Rest|Pose::Perch=>&CHICKEN_REST,_=>&CHICKEN_IDLE}},
        Species::GuineaFowl=>if walk {&GUINEA_WALK} else {match pose {
            Pose::Look=>&GUINEA_LOOK,Pose::Forage=>&GUINEA_FORAGE,Pose::Rest|Pose::Perch=>&GUINEA_REST,_=>&GUINEA_IDLE}},
        Species::GuineaChick=>if walk {&KEET_WALK} else {match pose {
            Pose::Forage=>&KEET_FORAGE,Pose::Rest|Pose::Huddle=>&KEET_REST,_=>&KEET_IDLE}},
    };
    if a.antic.is_some_and(|v|v.active(tick) && v.kind==crate::animations::AnticKind::Flutter) {
        clip=match a.species {Species::Chicken=>&CHICKEN_FLUTTER,
            Species::GuineaFowl=>&GUINEA_FLUTTER,Species::GuineaChick=>&KEET_FLUTTER,_=>clip};
    }
    clip
}
pub fn animal_bounds(a:&Animal,tick:u64)->Option<(i32,i32,i32,i32)> {
    if !a.visible(){return None;}
    let clip=chosen_clip(a,tick);
    let p=a.visual_pos(tick);
    Some((p.x*3,p.y*3,clip.w as i32,clip.h as i32))
}
pub fn animal(c:&mut Canvas,a:&Animal,tick:u64) {
    let clip=chosen_clip(a,tick);
    let pose=a.pose(tick);
    let walk=matches!(pose,Pose::WalkA|Pose::WalkB|Pose::Hop);
    let pos=a.visual_pos(tick);
    let ticks=if walk {match a.species {Species::Shepherd=>8,Species::Cat=>3,
        Species::Chicken=>3,_=>2}} else {6};
    let palette=if a.species==Species::Chicken {a.palette%7} else {255};
    draw(c,clip,pos.x,pos.y,a.facing==Facing::Left,(tick/ticks+a.id as u64*3) as usize,palette);
}


// v0.55.42: the retained living-animal atlas also supplies a deliberately small
// background-fauna layer for the modern Null Island scene. This is NOT the old
// --og zoo simulation: positions are fixed native-pixel anchors chosen for the
// waterfront, sprites are scaled down, and the generalized cryptid actor still
// paints afterward in the foreground.
const NULL_BG_NUM:i32=2;
const NULL_BG_DEN:i32=3;

#[inline]
fn background_feather(rgb:u32,palette:u8)->u32 {
    if palette>=7 {return rgb;}
    const FEATHERS:[[u32;3];7]=[
        [0xe6d5ae,0xfff2d5,0xac8e67],[0xd6a65b,0xf6d493,0x956c3d],
        [0xb56849,0xe5a87a,0x774735],[0x626675,0xa2a6af,0x3f4350],
        [0xb6b6aa,0xeeeadd,0x747b74],[0xc18a54,0xf0c788,0x855c39],
        [0x8e6250,0xc99877,0x604339]];
    match rgb {
        0xe6d5ae=>FEATHERS[palette as usize][0],
        0xfff2d5=>FEATHERS[palette as usize][1],
        0xac8e67=>FEATHERS[palette as usize][2],
        _=>rgb,
    }
}

#[inline]
fn background_bounds_for(clip:&Clip,foot_x:i32,foot_y:i32)->(i32,i32,i32,i32) {
    let w=(clip.w as i32*NULL_BG_NUM/NULL_BG_DEN).max(1);
    let h=(clip.h as i32*NULL_BG_NUM/NULL_BG_DEN).max(1);
    (foot_x-w/2,foot_y-h,w,h)
}

fn draw_background_clip(c:&mut Canvas,clip:&Clip,foot_x:i32,foot_y:i32,
    mirror:bool,frame:usize,palette:u8)
{
    let frame=frame%clip.frames;
    let (left,top,dw,dh)=background_bounds_for(clip,foot_x,foot_y);
    let stride=(c.w*c.scale) as i32;
    let height=(c.h*c.scale) as i32;
    if left+dw<=0 || top+dh<=0 || left>=stride || top>=height || !c.overlaps(left,top,dw,dh) {return;}
    let start=clip.order[frame]*clip.w*clip.h;
    for dy in 0..dh {
        let sy=((dy*NULL_BG_DEN/NULL_BG_NUM) as usize).min(clip.h-1);
        for dx in 0..dw {
            let mut sx=((dx*NULL_BG_DEN/NULL_BG_NUM) as usize).min(clip.w-1);
            if mirror {sx=clip.w-1-sx;}
            let mut rgb=clip.colors[clip.data[start+sy*clip.w+sx] as usize];
            if rgb==0 {continue;}
            rgb=background_feather(rgb,palette);
            c.native_put(left+dx,top+dy,rgb);
        }
    }
}

// Native-pixel foot anchors. They intentionally live on the accepted v0.55.03
// waterfront rather than the retired old-zoo habitat geometry. Keep these
// centralized: this candidate is expected to get a visual anchor-tuning pass.
const DAY_CHICKENS:[(i32,i32);8]=[
    (616,535),(655,544),(695,534),(734,548),(772,538),(807,552),(840,544),(879,557),
];
const DAY_GUINEAS:[(i32,i32);5]=[(632,562),(680,555),(726,565),(777,562),(826,568)];
const DAY_CHICKS:[(i32,i32);3]=[(857,570),(892,574),(925,570)];
const NIGHT_DEER:[(i32,i32);4]=[(585,551),(663,548),(718,559),(770,553)];

#[inline]
fn bat_anchor(tick:u64,index:usize)->(i32,i32) {
    // Two slow opposing triangular flights above the waterfront. Integer-only,
    // deterministic, and intentionally much calmer than the foreground actor.
    let phase=((tick/2+index as u64*47)%160) as i32;
    let wave=if phase<80 {phase}else{159-phase};
    let x=690+index as i32*105+wave;
    let y=468-(wave.min(40)/5)+index as i32*5;
    (x,y)
}

pub fn draw_null_island_background(c:&mut Canvas,phase:crate::circadian::Phase,tick:u64) {
    use atlas::*;
    if matches!(phase,crate::circadian::Phase::Dusk|crate::circadian::Phase::Night) {
        let deer=[&STAG_FORAGE,&DOE_IDLE,&FAWN_IDLE,&DOE_FORAGE];
        for (i,(&(x,y),clip)) in NIGHT_DEER.iter().zip(deer).enumerate() {
            draw_background_clip(c,clip,x,y,i%2==1,(tick/10+i as u64*2) as usize,255);
        }
        draw_background_clip(c,&CHUPACABRA_IDLE,829,563,true,(tick/9) as usize,255);
        draw_background_clip(c,&SASQUATCH_IDLE,905,560,false,(tick/11) as usize,255);
        for i in 0..2 {
            let (x,y)=bat_anchor(tick,i);
            draw_background_clip(c,&BAT_FLY,x,y,i==1,(tick/3+i as u64*3) as usize,255);
        }
    } else {
        draw_background_clip(c,&DOG_IDLE,570,556,false,(tick/11) as usize,255);
        for (i,&(x,y)) in DAY_CHICKENS.iter().enumerate() {
            let clip=if (tick/120+i as u64)%3==0 {&CHICKEN_WALK}else{&CHICKEN_FORAGE};
            draw_background_clip(c,clip,x,y,i%2==1,(tick/7+i as u64*2) as usize,(i%7) as u8);
        }
        for (i,&(x,y)) in DAY_GUINEAS.iter().enumerate() {
            let clip=if (tick/150+i as u64)%4==0 {&GUINEA_WALK}else{&GUINEA_FORAGE};
            draw_background_clip(c,clip,x,y,i%2==0,(tick/8+i as u64*3) as usize,255);
        }
        for (i,&(x,y)) in DAY_CHICKS.iter().enumerate() {
            let clip=if (tick/100+i as u64)%3==0 {&KEET_WALK}else{&KEET_FORAGE};
            draw_background_clip(c,clip,x,y,i==1,(tick/6+i as u64*2) as usize,255);
        }
    }
}

pub fn for_each_null_island_background_bound<F:FnMut(i32,i32,i32,i32)>(
    phase:crate::circadian::Phase,tick:u64,mut f:F)
{
    use atlas::*;
    if matches!(phase,crate::circadian::Phase::Dusk|crate::circadian::Phase::Night) {
        let deer=[&STAG_FORAGE,&DOE_IDLE,&FAWN_IDLE,&DOE_FORAGE];
        for (&(x,y),clip) in NIGHT_DEER.iter().zip(deer) {
            let (x,y,w,h)=background_bounds_for(clip,x,y);f(x,y,w,h);
        }
        for (clip,x,y) in [(&CHUPACABRA_IDLE,829,563),(&SASQUATCH_IDLE,905,560)] {
            let (x,y,w,h)=background_bounds_for(clip,x,y);f(x,y,w,h);
        }
        for i in 0..2 {
            let (x,y)=bat_anchor(tick,i);
            let (x,y,w,h)=background_bounds_for(&BAT_FLY,x,y);f(x,y,w,h);
        }
    } else {
        let (x,y,w,h)=background_bounds_for(&DOG_IDLE,570,556);f(x,y,w,h);
        for &(x,y) in &DAY_CHICKENS {let (x,y,w,h)=background_bounds_for(&CHICKEN_WALK,x,y);f(x,y,w,h);}
        for &(x,y) in &DAY_GUINEAS {let (x,y,w,h)=background_bounds_for(&GUINEA_WALK,x,y);f(x,y,w,h);}
        for &(x,y) in &DAY_CHICKS {let (x,y,w,h)=background_bounds_for(&KEET_WALK,x,y);f(x,y,w,h);}
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn articulated_frames_change_and_mirror_exactly() {
        for clip in [&atlas::DOG_WALK,&atlas::STAG_WALK,&atlas::SASQUATCH_WALK,
            &atlas::CHUPACABRA_WALK,&atlas::CHICKEN_WALK,&atlas::GUINEA_WALK,
            &atlas::CAT_WALK,&atlas::CARDINAL_FLY,&atlas::BAT_FLY] {
            assert_eq!(clip.data.len()%(clip.w*clip.h),0);
            assert!(clip.order.iter().all(|f| (f+1)*clip.w*clip.h<=clip.data.len()));
            let mut a=Canvas::scaled(clip.w/3,clip.h/3,3,false);
            let mut b=Canvas::scaled(clip.w/3,clip.h/3,3,false);
            draw(&mut a,clip,0,0,false,0,255);
            draw(&mut b,clip,0,0,false,2,255);
            assert_ne!(a.cells,b.cells);
            b.clear(0);draw(&mut b,clip,0,0,true,0,255);
            for y in 0..clip.h {for x in 0..clip.w {
                assert_eq!(a.cells[y*clip.w+x],b.cells[y*clip.w+clip.w-1-x]);
            }}
        }
    }
    #[test] fn clips_offscreen_without_panicking() {
        let mut c=Canvas::scaled(10,10,3,false);
        draw(&mut c,&atlas::DOG_WALK,-100,-100,false,7,255);
        assert!(c.cells.iter().all(|p|*p==0));
        draw(&mut c,&atlas::DOG_WALK,-50,-25,true,7,255);
    }
    #[test] fn null_island_background_has_requested_day_and_night_rosters() {
        use crate::circadian::Phase;
        let mut day=0;
        for_each_null_island_background_bound(Phase::Day,0,|_,_,_,_|day+=1);
        let mut dawn=0;
        for_each_null_island_background_bound(Phase::Dawn,0,|_,_,_,_|dawn+=1);
        let mut dusk=0;
        for_each_null_island_background_bound(Phase::Dusk,0,|_,_,_,_|dusk+=1);
        let mut night=0;
        for_each_null_island_background_bound(Phase::Night,0,|_,_,_,_|night+=1);
        assert_eq!(day,17); // dog + 8 hens + 5 guineas + 3 chicks
        assert_eq!(dawn,17);
        assert_eq!(dusk,8);
        assert_eq!(night,8); // 4 deer + chupacabra + sasquatch + 2 bats
    }
}
