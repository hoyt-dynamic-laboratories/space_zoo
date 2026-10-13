//! Rev .45 — pre-baked motion geometry for the .4425 Gilded Sun.
//!
//! The supplied illustration is baked as TWO native-resolution layers:
//! a completely immutable golden face/Greek-key bezel, and a separate
//! transparently matted radial corona. ONLY the latter rotates, curls,
//! glints and breathes. This is deliberate: the expression is stoic, never
//! pulsing, morphing, spinning, or otherwise animated.
//!
//! The corona is refreshed at ~3.75 Hz and held between refreshes. Both
//! native framebuffer and desktop window blit the same buffer. No decoding,
//! heap churn, random flicker, GPU, or runtime trigonometry is required.
//! .45 moves EVERY pixel's radius, polar angle, and stationary optical profile,
//! plus its trig curves, into offline assets. Runtime samples a sine LUT and
//! performs only the filtering/blending needed for the unchanged appearance.
use crate::pixel::{Canvas, ART_SCALE};
use crate::world::FPS;


const TEXTURE_SIDE:usize=256;
const TEXTURE_LEN:usize=TEXTURE_SIDE*TEXTURE_SIDE*4;
const FACE:&[u8;TEXTURE_LEN]=include_bytes!("../assets/solar_face_256.rgba");
const CORONA:&[u8;TEXTURE_LEN]=include_bytes!("../assets/solar_corona_256.rgba");
const PAD:i32=12;
const EXTENT:i32=TEXTURE_SIDE as i32/2+PAD;
const SIDE:usize=(2*EXTENT+1) as usize;
const ANIMATION_TICKS:u64=8; // 30 Hz sim / 8: subpixel motion ~3.75 Hz
const MOTION_GEOMETRY:&[u8]=include_bytes!("../assets/solar_motion_geometry.v1");
const WAVE_DATA:&[u8;131072]=include_bytes!("../assets/solar_motion_sine.v1");
const MOTION_RECORD_BYTES:usize=22;
const WAVE_LEN:usize=65536;
const WAVE_MASK:usize=WAVE_LEN-1;
const QUARTER_TURN:usize=WAVE_LEN/4;
const ANGULAR_UNITS_PER_RAD:f64=WAVE_LEN as f64/std::f64::consts::TAU;

// All geometry comes from the OFFLINE asset baker, never recomputed per frame.
struct RayGeometry {
    pixel:usize,
    x:f32,
    y:f32,
    bend_phase:usize,
    breathe_phase:usize,
    shimmer_phase:usize,
    outer:f32,
    halo:f32,
}

#[inline]
fn animation_phase(seconds:f64,radians_per_second:f64)->usize {
    ((seconds*radians_per_second*ANGULAR_UNITS_PER_RAD).round() as i64 as usize)&WAVE_MASK
}

fn load_geometry()->Vec<RayGeometry> {
    assert_eq!(MOTION_GEOMETRY.len()%MOTION_RECORD_BYTES,0);
    MOTION_GEOMETRY.chunks_exact(MOTION_RECORD_BYTES).map(|v| RayGeometry {
        pixel:u32::from_le_bytes(v[0..4].try_into().unwrap()) as usize,
        x:i16::from_le_bytes(v[4..6].try_into().unwrap()) as f32,
        y:i16::from_le_bytes(v[6..8].try_into().unwrap()) as f32,
        bend_phase:u16::from_le_bytes(v[8..10].try_into().unwrap()) as usize,
        breathe_phase:u16::from_le_bytes(v[10..12].try_into().unwrap()) as usize,
        shimmer_phase:u16::from_le_bytes(v[12..14].try_into().unwrap()) as usize,
        outer:f32::from_le_bytes(v[14..18].try_into().unwrap()),
        halo:f32::from_le_bytes(v[18..22].try_into().unwrap()),
    }).collect()
}

fn load_wave()->Vec<f32> {
    WAVE_DATA.chunks_exact(2).map(|s| {
        i16::from_le_bytes([s[0],s[1]]) as f32 / 32767.0
    }).collect()
}


/// Repaint exactly this portion of the foreground occlusion mask.
pub const OCCLUSION_RADIUS:i32=(EXTENT+ART_SCALE as i32-1)/ART_SCALE as i32;

#[inline]
fn blend(bg:u32,fg:u32,alpha:u32)->u32 {
    let inv=255-alpha;
    let r=(((bg>>16)&255)*inv+((fg>>16)&255)*alpha+127)/255;
    let g=(((bg>>8)&255)*inv+((fg>>8)&255)*alpha+127)/255;
    let b=((bg&255)*inv+(fg&255)*alpha+127)/255;
    (r<<16)|(g<<8)|b
}

// Alpha-aware bilinear resampling keeps the reference's hairline gold rays
// sharp when turning, without creating black fringing on the blue sky.
#[inline]
fn sample_ray(sx:f32,sy:f32)->(u32,f32) {
    if sx<0.0 || sy<0.0 || sx>255.0 || sy>255.0 {return (0,0.0);}
    let ix=sx as usize;
    let iy=sy as usize;
    let fx=sx-ix as f32;
    let fy=sy-iy as f32;
    let coords=[(ix,iy,(1.0-fx)*(1.0-fy)),
        ((ix+1).min(255),iy,fx*(1.0-fy)),
        (ix,(iy+1).min(255),(1.0-fx)*fy),
        ((ix+1).min(255),(iy+1).min(255),fx*fy)];
    let (mut a,mut r,mut g,mut b)=(0.0,0.0,0.0,0.0);
    for (x,y,w) in coords {
        let p=4*(y*TEXTURE_SIDE+x);
        let wa=w*CORONA[p+3] as f32/255.0;
        a+=wa;
        r+=wa*CORONA[p] as f32;
        g+=wa*CORONA[p+1] as f32;
        b+=wa*CORONA[p+2] as f32;
    }
    if a<=0.001 {return(0,0.0);}
    let r=(r/a).round().clamp(0.0,255.0) as u32;
    let g=(g/a).round().clamp(0.0,255.0) as u32;
    let b=(b/a).round().clamp(0.0,255.0) as u32;
    ((r<<16)|(g<<8)|b,a)
}

pub struct SunSprite {
    ray_rgb:Vec<u32>,
    ray_alpha:Vec<u8>,
    cached_step:Option<u64>,
    geometry:Vec<RayGeometry>,
    wave:Vec<f32>,
}
impl SunSprite {
    pub fn new()->Self {
        Self {
            ray_rgb:vec![0;SIDE*SIDE],ray_alpha:vec![0;SIDE*SIDE],
            cached_step:None,geometry:load_geometry(),wave:load_wave(),
        }
    }

    fn prepare(&mut self,tick:u64){
        let step=tick/ANIMATION_TICKS;
        if self.cached_step==Some(step){return;}
        self.cached_step=Some(step);
        let secs=step as f64*ANIMATION_TICKS as f64/FPS as f64;
        // Same .4425 kinetics: 0.012 rad/s rotation (~nine minutes),
        // traveling curl at -0.62, breathing at 0.83, shimmer at -1.1.
        // Only FOUR phase counters per corona refresh; no trig, hypot,
        // atan2, sqrt or radial profile calculation in the frame loop.
        let drift_phase=animation_phase(secs,0.012);
        let bend_phase=animation_phase(secs,-0.62);
        let breathe_phase=animation_phase(secs,0.83);
        let shimmer_phase=animation_phase(secs,-1.1);
        let wave=&self.wave;
        let drift_sin=wave[drift_phase];
        let drift_cos=wave[(drift_phase+QUARTER_TURN)&WAVE_MASK];
        for ray in &self.geometry {
            // sin(drift+bend) ~ sin(drift)+bend*cos(drift), with a
            // worst-case <0.007px displacement at the outer rim. Keep
            // bilinear alpha sampling, so the hairline rays stay crisp.
            let bend=ray.outer*0.010*wave[(ray.bend_phase+bend_phase)&WAVE_MASK];
            let breathe=1.0+ray.outer*0.009*wave[(ray.breathe_phase+breathe_phase)&WAVE_MASK];
            let sin=drift_sin+bend*drift_cos;
            let cos=drift_cos-bend*drift_sin;
            let xx=(ray.x*cos+ray.y*sin)/breathe;
            let yy=(-ray.x*sin+ray.y*cos)/breathe;
            let (color,a)=sample_ray(xx+127.5,yy+127.5);
            let combined=a+ray.halo*(1.0-a);
            if combined<0.002 {
                self.ray_alpha[ray.pixel]=0;
                continue;
            }
            let shimmer=0.965+0.035*wave[(ray.shimmer_phase+shimmer_phase+QUARTER_TURN)&WAVE_MASK];
            let shimmer_rgb=[
                (((color>>16)&255) as f32*shimmer).min(255.0),
                (((color>>8)&255) as f32*shimmer).min(255.0),
                ((color&255) as f32*shimmer).min(255.0),
            ];
            let base=a/combined;
            let glow=ray.halo*(1.0-a)/combined;
            let r_out=(shimmer_rgb[0]*base+255.0*glow).round() as u32;
            let g_out=(shimmer_rgb[1]*base+202.0*glow).round() as u32;
            let b_out=(shimmer_rgb[2]*base+102.0*glow).round() as u32;
            self.ray_rgb[ray.pixel]=(r_out.min(255)<<16)|(g_out.min(255)<<8)|b_out.min(255);
            self.ray_alpha[ray.pixel]=(combined*255.0).round() as u8;
        }
    }

    pub fn draw(&mut self,canvas:&mut Canvas,x:i32,y:i32,tick:u64){
        let stride=canvas.w*canvas.scale;
        let height=canvas.h*canvas.scale;
        let left=x*canvas.scale as i32-EXTENT;
        let top=y*canvas.scale as i32-EXTENT;
        let x0=(-left).max(0) as usize;
        let x1=(stride as i32-left).min(SIDE as i32).max(0) as usize;
        let y0=(-top).max(0) as usize;
        let y1=(height as i32-top).min(SIDE as i32).max(0) as usize;
        if x0>=x1||y0>=y1 || !canvas.overlaps(left,top,SIDE as i32,SIDE as i32) {return;}
        self.prepare(tick);
        // FIRST the entirely independent moving corona and faint gold optics.
        for py in y0..y1 {
            let dst_row=(top+py as i32) as usize*stride;
            for px in x0..x1 {
                let i=py*SIDE+px;
                let a=self.ray_alpha[i] as u32;
                if a==0 {continue;}
                if !canvas.touched((left+px as i32) as usize,(top+py as i32) as usize) {continue;}
                let di=dst_row+(left+px as i32) as usize;
                let fg=self.ray_rgb[i];
                canvas.cells[di]=if a==255 {fg}else{blend(canvas.cells[di],fg,a)};
            }
        }
        // THEN the exactly unmodified, immobile portrait and engraved bezel.
        // This copy must never depend on `tick`, `step`, or animation phase.
        let face_x0=x0.max(PAD as usize);
        let face_x1=x1.min(PAD as usize+TEXTURE_SIDE);
        let face_y0=y0.max(PAD as usize);
        let face_y1=y1.min(PAD as usize+TEXTURE_SIDE);
        for py in face_y0..face_y1 {
            let dst_row=(top+py as i32) as usize*stride;
            let src_row=(py-PAD as usize)*TEXTURE_SIDE;
            for px in face_x0..face_x1 {
                let p=(src_row+px-PAD as usize)*4;
                let a=FACE[p+3] as u32;
                if a==0 {continue;}
                let fg=((FACE[p] as u32)<<16)|((FACE[p+1] as u32)<<8)|FACE[p+2] as u32;
                if !canvas.touched((left+px as i32) as usize,(top+py as i32) as usize) {continue;}
                let di=dst_row+(left+px as i32) as usize;
                canvas.cells[di]=if a==255 {fg}else{blend(canvas.cells[di],fg,a)};
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn baked_motion_covers_only_the_original_ring(){
        assert_eq!(WAVE_DATA.len(),WAVE_LEN*2);
        assert_eq!(MOTION_GEOMETRY.len()%MOTION_RECORD_BYTES,0);
        let sun=SunSprite::new();
        assert!(sun.geometry.len()>50_000);
        assert_eq!(sun.wave.len(),WAVE_LEN);
        assert!(sun.geometry.iter().all(|p|p.pixel<SIDE*SIDE));
        assert!(sun.geometry.iter().all(|p| {
            let r2=p.x*p.x+p.y*p.y;
            (50.0*50.0..=140.0*140.0).contains(&r2)
        }));
        // A complete rotation must remain a slow movement, never a short GIF loop.
        assert_ne!(animation_phase(10.0,0.012),animation_phase(200.0,0.012));
    }
    #[test]
    fn artwork_baked_with_independent_alpha(){
        assert_eq!(FACE.len(),262144);
        assert_eq!(CORONA.len(),262144);
        let at=|x:usize,y:usize| (y*TEXTURE_SIDE+x)*4+3;
        assert_eq!(FACE[at(128,128)],255,"opaque face covers the starfield");
        assert_eq!(CORONA[at(128,128)],0,"corona never covers face center");
        assert_eq!(FACE[at(0,0)],0);
        assert_eq!(CORONA[at(0,0)],0);
    }
    #[test]
    fn corona_is_animated_but_portrait_never_changes(){
        let mut sun=SunSprite::new();
        sun.prepare(0);
        let a=sun.ray_alpha.clone();
        let rgb=sun.ray_rgb.clone();
        let unchanged=FACE[12000];
        sun.prepare(FPS*20);
        assert!(a!=sun.ray_alpha||rgb!=sun.ray_rgb,"rays must move");
        assert_eq!(FACE[12000],unchanged,"portrait asset cannot animate");
    }
    #[test]
    fn native_blit_clips_offscreen_without_panics(){
        let mut sun=SunSprite::new();
        let mut canvas=Canvas::scaled(320,200,ART_SCALE,false);
        sun.draw(&mut canvas,-32,-6,0);
        sun.draw(&mut canvas,325,202,30);
        sun.draw(&mut canvas,160,45,60);
        assert_eq!(canvas.cells.len(),960*600);
    }
    #[test]
    fn corona_is_cached_between_animation_steps(){
        let mut sun=SunSprite::new();
        sun.prepare(8);
        assert_eq!(sun.cached_step,Some(1));
        sun.prepare(15);
        assert_eq!(sun.cached_step,Some(1));
        sun.prepare(16);
        assert_eq!(sun.cached_step,Some(2));
    }
}
