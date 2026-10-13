//! Rev .4424: photographic, native-pixel Moon with true spherical terminator.
//!
//! The 256x256 lunar albedo was extracted from the reference image provided
//! for this revision. It is raw, single-channel image data: no PNG decoder,
//! file IO, external dependencies, or allocation in the render loop.
//! The terminator is computed from the visible sphere's surface normals;
//! a faint earthlit night side and subtle optical bloom complete the image.
use crate::pixel::{Canvas, ART_SCALE};

const ALBEDO_SIDE:usize = 256;
const ALBEDO:&[u8;ALBEDO_SIDE*ALBEDO_SIDE] =
    include_bytes!("../assets/lunar_albedo_256.r8");
const RADIUS:i32 = 66;             // 22 logical pixels, rendered natively
const HALO:i32 = 28;               // atmosphere-free photographic lens bloom
const EXTENT:i32 = RADIUS+HALO;
const SIDE:usize = (2*EXTENT+1) as usize;
const PHASE_STEPS:u32 = 4096;      // refresh texture roughly every 10.4 minutes

/// Range for restoring foreground pixels over the Moon and its full halo.
pub const OCCLUSION_RADIUS:i32 = (EXTENT+ART_SCALE as i32-1)/ART_SCALE as i32;

/// Precomposited photographic sprite; rebuilt only when its phase step changes.
/// Both the desktop window and framebuffer consume its native RGB output.
pub struct MoonSprite {
    rgb:Vec<u32>,
    alpha:Vec<u8>,
    phase_step:Option<u16>,
}

#[inline]
fn mix_pixel(bg:u32,fg:u32,alpha:u32)->u32 {
    let a=alpha;
    let inv=255-a;
    let r=(((bg>>16)&255)*inv+((fg>>16)&255)*a+127)/255;
    let g=(((bg>>8)&255)*inv+((fg>>8)&255)*a+127)/255;
    let b=((bg&255)*inv+(fg&255)*a+127)/255;
    (r<<16)|(g<<8)|b
}

#[inline]
fn smoothstep(edge0:f32,edge1:f32,x:f32)->f32 {
    let t=((x-edge0)/(edge1-edge0)).clamp(0.0,1.0);
    t*t*(3.0-2.0*t)
}

/// Exact lit-sphere geometry: the right side waxes, the left side wanes.
/// Phase 0 is new; .25 first quarter; .5 full; .75 last quarter.
#[cfg(test)]
#[inline]
fn sun_dot(nx:f32,ny:f32,phase:f32)->f32 {
    let theta=std::f32::consts::TAU*phase;
    let nz=(1.0-nx*nx-ny*ny).max(0.0).sqrt();
    nx*theta.sin()-nz*theta.cos()
}

#[inline]
fn sample_albedo(tx:f32,ty:f32)->f32 {
    let x=tx.clamp(0.0,255.0);
    let y=ty.clamp(0.0,255.0);
    let ix=x as usize;
    let iy=y as usize;
    let fx=x-ix as f32;
    let fy=y-iy as f32;
    let x1=(ix+1).min(ALBEDO_SIDE-1);
    let y1=(iy+1).min(ALBEDO_SIDE-1);
    let a=ALBEDO[iy*ALBEDO_SIDE+ix] as f32;
    let b=ALBEDO[iy*ALBEDO_SIDE+x1] as f32;
    let c=ALBEDO[y1*ALBEDO_SIDE+ix] as f32;
    let d=ALBEDO[y1*ALBEDO_SIDE+x1] as f32;
    let upper=a+(b-a)*fx;
    let lower=c+(d-c)*fx;
    upper+(lower-upper)*fy
}

impl MoonSprite {
    pub fn new()->Self {
        Self{rgb:vec![0;SIDE*SIDE],alpha:vec![0;SIDE*SIDE],phase_step:None}
    }

    fn prepare(&mut self,phase:f32){
        let key=((phase.rem_euclid(1.0)*PHASE_STEPS as f32).round() as u32
            % PHASE_STEPS) as u16;
        if self.phase_step==Some(key){return;}
        self.phase_step=Some(key);
        let phase=key as f32/PHASE_STEPS as f32;
        let theta=std::f32::consts::TAU*phase;
        let (sun_x,sun_z)=(theta.sin(),-theta.cos());
        let lit_fraction=0.5*(1.0-theta.cos());
        // Unlike a uniformly glowing disc, near-new phases barely bloom.
        let bloom=0.22*lit_fraction.sqrt();
        // Sunlight reflected off Earth softly reveals only the dark hemisphere.
        let earthshine=0.028+0.058*(1.0-lit_fraction).powi(2);
        let radius=RADIUS as f32;
        for py in 0..SIDE {
            let dy=py as i32-EXTENT;
            for px in 0..SIDE {
                let dx=px as i32-EXTENT;
                let idx=py*SIDE+px;
                let distance=((dx*dx+dy*dy) as f32).sqrt();
                if distance>radius+HALO as f32 {
                    self.alpha[idx]=0;
                    continue;
                }
                if distance>radius+0.5 {
                    let spread=((distance-radius)/HALO as f32).clamp(0.0,1.0);
                    // Broad, very low opacity lens bloom, not a painted ring.
                    let falloff=(1.0-spread).powi(3);
                    self.alpha[idx]=(255.0*bloom*falloff).round() as u8;
                    self.rgb[idx]=0xd7e0eb;
                    continue;
                }
                let nx=dx as f32/radius;
                let ny=dy as f32/radius;
                let nz=(1.0-nx*nx-ny*ny).max(0.0).sqrt();
                let dot=nx*sun_x+nz*sun_z;
                // Approximately one physical pixel of anti-aliased terminator.
                let lit=smoothstep(-0.016,0.020,dot);
                let albedo=sample_albedo(
                    (nx*0.5+0.5)*255.0,(ny*0.5+0.5)*255.0);
                // Dark maria, bright ejecta rays, and a natural edge falloff.
                let relief=0.72+0.28*dot.max(0.0).sqrt();
                let surface=(82.0+171.0*albedo/255.0)*relief;
                let brightness=(surface*(lit+(1.0-lit)*earthshine))
                    .clamp(0.0,255.0);
                let r=brightness.round() as u32;
                let g=(brightness*1.008).round().min(255.0) as u32;
                let b=(brightness*1.027).round().min(255.0) as u32;
                self.rgb[idx]=(r<<16)|(g<<8)|b;
                // Subpixel limb coverage; the unlit hemisphere still blocks
                // background stars, as a real solid moon must.
                self.alpha[idx]=(255.0*(radius+0.5-distance).clamp(0.0,1.0))
                    .round() as u8;
            }
        }
    }

    pub fn draw(&mut self,canvas:&mut Canvas,x:i32,y:i32,phase:f32){
        let stride=canvas.w*canvas.scale;
        let height=canvas.h*canvas.scale;
        let center_x=x*canvas.scale as i32;
        let center_y=y*canvas.scale as i32;
        let left=center_x-EXTENT;
        let top=center_y-EXTENT;
        let x0=(-left).max(0) as usize;
        let x1=(stride as i32-left).min(SIDE as i32).max(0) as usize;
        let y0=(-top).max(0) as usize;
        let y1=(height as i32-top).min(SIDE as i32).max(0) as usize;
        if x0>=x1 || y0>=y1 || !canvas.overlaps(left,top,SIDE as i32,SIDE as i32) {return;}
        self.prepare(phase);
        for row in y0..y1 {
            let src_row=row*SIDE;
            let dst_row=(top+row as i32) as usize*stride;
            for col in x0..x1 {
                let src=src_row+col;
                let a=self.alpha[src] as u32;
                if a==0 {continue;}
                if !canvas.touched((left+col as i32) as usize,(top+row as i32) as usize) {continue;}
                let dst=dst_row+(left+col as i32) as usize;
                let fg=self.rgb[src];
                canvas.cells[dst]=if a==255 {fg}
                    else {mix_pixel(canvas.cells[dst],fg,a)};
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn photographic_albedo_embedded_and_nonuniform(){
        assert_eq!(ALBEDO.len(),65536);
        let (min,max)=ALBEDO.iter().copied().fold((255u8,0u8),|(lo,hi),v|
            (lo.min(v),hi.max(v)));
        assert!(max>min+100,"photographic contrast absent");
    }
    #[test]
    fn phases_illuminate_correct_hemisphere(){
        assert!(sun_dot(0.7,0.0,0.25)>0.0); // first quarter right
        assert!(sun_dot(-0.7,0.0,0.25)<0.0);
        assert!(sun_dot(-0.7,0.0,0.75)>0.0); // last quarter left
        assert!(sun_dot(0.7,0.0,0.75)<0.0);
        assert!(sun_dot(0.0,0.0,0.5)>0.0); // full center
        assert!(sun_dot(0.0,0.0,0.0)<0.0); // new center
    }
    #[test]
    fn full_moon_is_bright_and_new_moon_has_no_halo(){
        let mut sprite=MoonSprite::new();
        let center=EXTENT as usize*SIDE+EXTENT as usize;
        let outer=EXTENT as usize*SIDE+(EXTENT+RADIUS+4) as usize;
        sprite.prepare(0.0);
        let new_color=sprite.rgb[center]&255;
        assert_eq!(sprite.alpha[outer],0);
        sprite.prepare(0.5);
        assert!((sprite.rgb[center]&255)>new_color+90);
        assert!(sprite.alpha[outer]>0);
    }
    #[test]
    fn moon_clips_cleanly_at_edge_of_scene(){
        let mut sprite=MoonSprite::new();
        let mut canvas=Canvas::scaled(320,200,ART_SCALE,false);
        sprite.draw(&mut canvas,-10,5,0.4);
        sprite.draw(&mut canvas,330,205,0.6);
        sprite.draw(&mut canvas,145,62,0.5);
        assert_eq!(canvas.cells.len(),960*600);
    }
}
