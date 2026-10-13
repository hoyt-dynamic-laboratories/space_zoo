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
}
