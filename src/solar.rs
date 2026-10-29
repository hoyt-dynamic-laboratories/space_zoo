//! Helios .46.3 candidate — centered coin-relief animation, offline-baked.
//!
//! One centered Helios coin beneath two counter-rotating coronas, two mane poses, and 192 clipped
//! skeleton-gear frames come directly from the approved Helios motion study.
//! All artwork, wheel gradients, the mane's fixed neck boundary, and the
//! circular wheel clip are baked into RGBA resources; the fixed wheel rim/hub and
//! the portrait never rotate. There are no shaders or runtime trigonometric
//! functions, polar maps, ray tracing, per-frame allocations, or decoding.
//!
//! Corona rotation uses pre-baked Q15 sin/cos pairs; its 8-tick cache
//! cadence is unchanged. The small skeleton wheel independently refreshes
//! at the billboard tourbillon's 5-tick cadence (6Hz), using existing
//! touched-tile compositing without resampling the coronas.
use crate::pixel::{Canvas, ART_SCALE};

const SIDE:usize=280;
const LEN:usize=SIDE*SIDE*4;
const HALF:i32=(SIDE/2) as i32;
const ANIMATION_TICKS:u64=8; // Coronas: unchanged 3.75 Hz.
pub(crate) const MANE_TICKS_PER_FRAME:u64=42; // Approved alternate mane pose every 1.4s.
pub(crate) const WHEEL_TICKS_PER_FRAME:u64=5; // Same cadence as billboard tourbillon: 6 Hz.
const SMALL:&[u8;LEN]=include_bytes!("../assets/helios/ring_small.rgba");
const BIG:&[u8;LEN]=include_bytes!("../assets/helios/ring_big.rgba");
const FACE_A:&[u8;LEN]=include_bytes!("../assets/helios/face_0.rgba");
const FACE_B:&[u8;LEN]=include_bytes!("../assets/helios/face_1.rgba");
const WHEEL_FRAMES:usize=192;
const WHEEL:&[u8;WHEEL_FRAMES*32*32*4]=include_bytes!("../assets/helios/wheel_192x32.rgba");
const ROTATION:&[u8;4096*4]=include_bytes!("../assets/helios/rotate_lut_4096.i16");
const WHEEL_X:usize=82;
const WHEEL_Y:usize=172;
const WHEEL_SIDE:usize=32;
// Exact union of pixels changed between face_0.rgba and face_1.rgba.
// Native source coordinates; 73x20, not the full 280x280 sun.
const MANE_X:i32=125;
const MANE_Y:i32=163;
const MANE_W:i32=73;
const MANE_H:i32=20;

pub(crate) fn mane_damage_rect(x:i32,y:i32)->(i32,i32,i32,i32) {
    (x*ART_SCALE as i32-HALF+MANE_X,
     y*ART_SCALE as i32-HALF+MANE_Y,MANE_W,MANE_H)
}

// Wheel damage uses the exact baked sprite offset, not the 280px sun bounds.
// Input coordinates match SunSprite::draw: logical celestial center.
pub(crate) fn wheel_damage_rect(x:i32,y:i32)->(i32,i32,i32,i32){
    (x*ART_SCALE as i32-HALF+WHEEL_X as i32,
     y*ART_SCALE as i32-HALF+WHEEL_Y as i32,
     WHEEL_SIDE as i32,WHEEL_SIDE as i32)
}

#[inline]
fn wheel_frame_at(tick:u64)->usize {
    // Hold the selected pose for five simulation ticks. Unrelated sun/actor
    // repaints cannot accidentally advance this 6Hz animation in between.
    let sampled_tick=(tick/WHEEL_TICKS_PER_FRAME)*WHEEL_TICKS_PER_FRAME;
    ((sampled_tick.saturating_mul(244_462)/1_000_000)%WHEEL_FRAMES as u64) as usize
}

// Preserve the old sun's dirty-region / foreground occlusion footprint.
pub const OCCLUSION_RADIUS:i32=(HALF+ART_SCALE as i32-1)/ART_SCALE as i32;

#[inline]
fn color_over_background(bg:u32,fg:u32,a:u32)->u32 {
    let inv=255-a;
    let r=(((bg>>16)&255)*inv+((fg>>16)&255)*a+127)/255;
    let g=(((bg>>8)&255)*inv+((fg>>8)&255)*a+127)/255;
    let b=((bg&255)*inv+(fg&255)*a+127)/255;
    (r<<16)|(g<<8)|b
}

// The two corona source layers are PREMULTIPLIED offline. Return an ARGB
// premultiplied sample, using only fixed-point interpolation and no trig.
#[inline]
fn sample_ring(src:&[u8;LEN],sx:i32,sy:i32)->u32 {
    if sx<0 || sy<0 {return 0;}
    let x=(sx>>16) as usize;
    let y=(sy>>16) as usize;
    if x+1>=SIDE || y+1>=SIDE {return 0;}
    let fx=((sx>>8)&255) as u32;
    let fy=((sy>>8)&255) as u32;
    let weights=[(256-fx)*(256-fy),fx*(256-fy),(256-fx)*fy,fx*fy];
    let p=(y*SIDE+x)*4;
    let locations=[p,p+4,p+SIDE*4,p+SIDE*4+4];
    let (mut r,mut g,mut b,mut a)=(0u32,0u32,0u32,0u32);
    for i in 0..4 {
        let off=locations[i];let w=weights[i];
        r+=src[off] as u32*w;
        g+=src[off+1] as u32*w;
        b+=src[off+2] as u32*w;
        a+=src[off+3] as u32*w;
    }
    let (r,g,b,a)=((r+32768)>>16,(g+32768)>>16,(b+32768)>>16,(a+32768)>>16);
    (a<<24)|(r<<16)|(g<<8)|b
}

// Top layer composited over bottom, both in premultiplied color space.
#[inline]
fn premult_over(bottom:u32,top:u32)->u32 {
    let a=(top>>24)&255;
    if a==0 {return bottom;}
    if a==255 {return top;}
    let inv=255-a;
    let ch=|shift:u32| -> u32 {
        let upper=(top>>shift)&255;
        let lower=(bottom>>shift)&255;
        (upper+(lower*inv+127)/255).min(255)
    };
    (ch(24)<<24)|(ch(16)<<16)|(ch(8)<<8)|ch(0)
}

#[inline]
fn rotate_pair(phase:u32)->(i32,i32) {
    let at=(phase as usize&4095)*4;
    (i16::from_le_bytes([ROTATION[at],ROTATION[at+1]]) as i32,
     i16::from_le_bytes([ROTATION[at+2],ROTATION[at+3]]) as i32)
}

/// Resample the two rings with row-wise fixed-point additions. No per-pixel
/// trig, Cartesian products, square roots, dynamic allocations, or decoding.
fn rotate_to_cache(cache:&mut [u32],src:&[u8;LEN],cos:i32,sin:i32,under:bool){
    const MID_FP:i32=(SIDE as i32-1)*32768;
    for y in 0..SIDE {
        let dy2=y as i32*2-(SIDE as i32-1);
        let mut sx=MID_FP-(SIDE as i32-1)*cos+dy2*sin;
        let mut sy=MID_FP+(SIDE as i32-1)*sin+dy2*cos;
        let row=y*SIDE;
        for x in 0..SIDE {
            let pix=sample_ring(src,sx,sy);
            let out=&mut cache[row+x];
            *out=if under {pix} else {premult_over(*out,pix)};
            sx+=2*cos;
            sy-=2*sin;
        }
    }
}

pub struct SunSprite {
    // One ~306KB cached composite; the approved art is immutable ROM data.
    corona:Vec<u32>,
    cached_step:Option<u64>,
}
impl SunSprite {
    pub fn new()->Self { Self { corona:vec![0;SIDE*SIDE],cached_step:None } }

    fn prepare(&mut self,tick:u64){
        let step=tick/ANIMATION_TICKS;
        if self.cached_step==Some(step) {return;}
        self.cached_step=Some(step);
        // 0.012 rad/s at 30 simulation ticks/sec, modulo 4096 rotations.
        // This Q15 phase rate is quantized offline: error <0.001%.
        let phase=((step.saturating_mul(ANIMATION_TICKS)).saturating_mul(260_759)
            /1_000_000 %4096) as u32;
        let (cos,sin)=rotate_pair(phase);
        // The small spikes turn counterclockwise, the large flames clockwise.
        // The stationary coin and relief are baked into the face texture.
        rotate_to_cache(&mut self.corona,SMALL,cos,-sin,true);
        rotate_to_cache(&mut self.corona,BIG,cos,sin,false);
    }

    /// Native-pixel coverage of the already prepared Sun. Applied ONLY inside
    /// Moon/Sun overlap, so the original solar pixel (including translucent
    /// corona, face and wheel) survives byte-for-byte as if no Moon existed.
    pub fn protects_native_pixel(&self,center_x:i32,center_y:i32,
            native_x:i32,native_y:i32,tick:u64)->bool {
        let x=native_x-(center_x*ART_SCALE as i32-HALF);
        let y=native_y-(center_y*ART_SCALE as i32-HALF);
        if x<0 || y<0 || x>=SIDE as i32 || y>=SIDE as i32 {return false;}
        let (x,y)=(x as usize,y as usize);
        if self.corona[y*SIDE+x]>>24 != 0 {return true;}
        let face=if (tick/MANE_TICKS_PER_FRAME)&1==0 {FACE_A} else {FACE_B};
        if face[(y*SIDE+x)*4+3]!=0 {return true;}
        if x>=WHEEL_X && x<WHEEL_X+WHEEL_SIDE && y>=WHEEL_Y && y<WHEEL_Y+WHEEL_SIDE {
            let frame=wheel_frame_at(tick);
            let pixel=((frame*WHEEL_SIDE*WHEEL_SIDE)+
                (y-WHEEL_Y)*WHEEL_SIDE+x-WHEEL_X)*4;
            return WHEEL[pixel+3]!=0;
        }
        false
    }

    pub fn draw(&mut self,canvas:&mut Canvas,x:i32,y:i32,tick:u64){
        let stride=canvas.w*canvas.scale;
        let height=canvas.h*canvas.scale;
        let left=x*canvas.scale as i32-HALF;
        let top=y*canvas.scale as i32-HALF;
        let x0=(-left).max(0) as usize;
        let x1=(stride as i32-left).clamp(0,SIDE as i32) as usize;
        let y0=(-top).max(0) as usize;
        let y1=(height as i32-top).clamp(0,SIDE as i32) as usize;
        if x0>=x1 || y0>=y1 || !canvas.overlaps(left,top,SIDE as i32,SIDE as i32){return;}
        self.prepare(tick);
        // The original motion study alternates the mane every 1.4 seconds.
        let face=if (tick/MANE_TICKS_PER_FRAME)&1==0 {FACE_A} else {FACE_B};
        // The asymmetric plate is a true 360-degree cycle, not the old
        // spoke wheel's repeating 30-degree segment. 20x original speed:
        // 0.240 rad/s, at 30 simulation ticks/sec with 192 baked poses.
        // 192 * 0.240 / (30 * 2pi) ~= 0.244462 frames per tick.
        let wheel_frame=wheel_frame_at(tick);
        let wheel=&WHEEL[wheel_frame*WHEEL_SIDE*WHEEL_SIDE*4..
            (wheel_frame+1)*WHEEL_SIDE*WHEEL_SIDE*4];
        for py in y0..y1 {
            let base=(top+py as i32) as usize*stride;
            for px in x0..x1 {
                let dest_x=(left+px as i32) as usize;
                if !canvas.touched(dest_x,(top+py as i32) as usize){continue;}
                let di=base+dest_x;
                let mut color=canvas.cells[di];
                let ring=self.corona[py*SIDE+px];
                let a=ring>>24;
                if a!=0 {
                    let inv=255-a;
                    let r=(((color>>16)&255)*inv+127)/255+((ring>>16)&255);
                    let g=(((color>>8)&255)*inv+127)/255+((ring>>8)&255);
                    let b=((color&255)*inv+127)/255+(ring&255);
                    color=(r.min(255)<<16)|(g.min(255)<<8)|b.min(255);
                }
                let fp=(py*SIDE+px)*4;
                let fa=face[fp+3] as u32;
                if fa!=0 {
                    let fg=((face[fp] as u32)<<16)|((face[fp+1] as u32)<<8)|face[fp+2] as u32;
                    color=if fa==255 {fg}else{color_over_background(color,fg,fa)};
                }
                if px>=WHEEL_X && px<WHEEL_X+WHEEL_SIDE && py>=WHEEL_Y && py<WHEEL_Y+WHEEL_SIDE {
                    let wp=((py-WHEEL_Y)*WHEEL_SIDE+(px-WHEEL_X))*4;
                    let wa=wheel[wp+3] as u32;
                    if wa!=0 {
                        let fg=((wheel[wp] as u32)<<16)|((wheel[wp+1] as u32)<<8)|wheel[wp+2] as u32;
                        color=if wa==255 {fg}else{color_over_background(color,fg,wa)};
                    }
                }
                canvas.cells[di]=color;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_assets_match_the_approved_280px_canvas(){
        assert_eq!(SMALL.len(),LEN);
        assert_eq!(BIG.len(),LEN);
        assert_eq!(FACE_A.len(),LEN);
        assert_eq!(FACE_B.len(),LEN);
        assert_eq!(WHEEL.len(),WHEEL_FRAMES*32*32*4);
        assert_eq!(ROTATION.len(),4096*4);
        let at=|x:usize,y:usize|(y*SIDE+x)*4+3;
        assert_eq!(SMALL[at(140,140)],0); // no second disk beneath coin
        assert_eq!(BIG[at(140,140)],0);
        assert!(FACE_A[at(140,140)]>0); // stationary coin relief
        assert_eq!(FACE_A[at(0,0)],0);
        assert_eq!(FACE_B[at(0,0)],0);
        assert_eq!(OCCLUSION_RADIUS,47);
    }
    #[test]
    fn contra_rotation_is_visible_but_cached(){
        let mut sun=SunSprite::new();
        sun.prepare(0);
        let a=sun.corona.clone();
        sun.prepare(7);
        assert_eq!(sun.corona,a);
        sun.prepare(30*20);
        assert_ne!(sun.corona,a);
    }
    #[test]
    fn wheel_refresh_is_five_ticks_and_does_not_change_corona_cache(){
        assert_eq!(WHEEL_TICKS_PER_FRAME,5);
        assert_eq!(ANIMATION_TICKS,8);
        assert_eq!(wheel_damage_rect(50,50),(92,182,32,32));
        for start in [0,10,75,150,600] {
            let frame=wheel_frame_at(start);
            for t in start..start+WHEEL_TICKS_PER_FRAME {
                assert_eq!(wheel_frame_at(t),frame);
            }
        }
        let mut sun=SunSprite::new();
        sun.prepare(0);
        let initial=sun.corona.clone();
        sun.prepare(5); // Wheel repaint does not recomposite coronas.
        assert_eq!(sun.corona,initial);
        assert_eq!(sun.cached_step,Some(0));
    }
    #[test]
    fn asymmetric_plate_requires_full_revolution(){
        let bytes_per_frame=WHEEL_SIDE*WHEEL_SIDE*4;
        assert_eq!(WHEEL_FRAMES,192);
        // At 30 degrees the old symmetric wheel repeated; not this plate.
        assert_ne!(&WHEEL[..bytes_per_frame],
                   &WHEEL[(WHEEL_FRAMES/12)*bytes_per_frame..(WHEEL_FRAMES/12+1)*bytes_per_frame]);
        assert_ne!(&WHEEL[..bytes_per_frame],
                   &WHEEL[(WHEEL_FRAMES/2)*bytes_per_frame..(WHEEL_FRAMES/2+1)*bytes_per_frame]);
    }
    #[test]
    fn mane_damage_rect_exactly_covers_changed_baked_pixels() {
        // Regression guard: invalidation bounds must change if either of the
        // approved two pose assets changes. Neither artwork nor timing is altered.
        let mut changed=0;
        let (mut min_x,mut min_y,mut max_x,mut max_y)=(SIDE,SIDE,0,0);
        for y in 0..SIDE { for x in 0..SIDE {
            let p=(y*SIDE+x)*4;
            if FACE_A[p..p+4]!=FACE_B[p..p+4] {
                changed+=1;
                min_x=min_x.min(x); max_x=max_x.max(x);
                min_y=min_y.min(y); max_y=max_y.max(y);
            }
        }}
        assert_eq!(changed,555);
        assert_eq!((min_x,min_y,max_x,max_y),
            (MANE_X as usize,MANE_Y as usize,
             (MANE_X+MANE_W-1) as usize,(MANE_Y+MANE_H-1) as usize));
        assert_eq!(mane_damage_rect(50,50),(135,173,73,20));
        assert_eq!(MANE_TICKS_PER_FRAME,42);
    }
    #[test]
    fn mane_changes_only_in_approved_baked_poses(){
        assert_ne!(FACE_A.as_slice(),FACE_B.as_slice());
        assert_eq!(&FACE_A[12000..18000],&FACE_B[12000..18000]);
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
}
