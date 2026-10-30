//! v0.55.03 default-scene artwork: aspect-correct Null Island waterfront + unified mountains.
//! The legacy zoo scenery and fauna remain available unchanged through --og.
use crate::pixel::Canvas;

const MOUNTAIN_W:usize=960;
const MOUNTAIN_H:usize=83;
const WATERFRONT_W:usize=960;
const WATERFRONT_H:usize=120;
const MOUNTAIN_Y:i32=450;
const WATERFRONT_X:i32=0;
const WATERFRONT_Y:i32=480;

const MOUNTAINS:&[u8]=include_bytes!("../assets/scenery/null_island/mountains_unified.rgba");
const WATERFRONT:&[u8]=include_bytes!("../assets/scenery/null_island/waterfront_coop.rgba");

#[inline]
fn blend(base:u32,r:u8,g:u8,b:u8,a:u8)->u32 {
    if a==255 {return ((r as u32)<<16)|((g as u32)<<8)|b as u32;}
    if a==0 {return base;}
    let a=a as u32;
    let inv=255-a;
    let br=(base>>16)&0xff;
    let bg=(base>>8)&0xff;
    let bb=base&0xff;
    let rr=(r as u32*a+br*inv+127)/255;
    let gg=(g as u32*a+bg*inv+127)/255;
    let bl=(b as u32*a+bb*inv+127)/255;
    (rr<<16)|(gg<<8)|bl
}

fn blit(c:&mut Canvas,src:&[u8],w:usize,h:usize,x0:i32,y0:i32) {
    debug_assert_eq!(src.len(),w*h*4);
    let stride=c.w*c.scale;
    let max_h=c.h*c.scale;
    for sy in 0..h {
        let y=y0+sy as i32;
        if y<0 || y>=max_h as i32 {continue;}
        for sx in 0..w {
            let x=x0+sx as i32;
            if x<0 || x>=stride as i32 || !c.touched(x as usize,y as usize) {continue;}
            let at=(sy*w+sx)*4;
            let a=src[at+3];
            if a==0 {continue;}
            let dst=y as usize*stride+x as usize;
            c.cells[dst]=blend(c.cells[dst],src[at],src[at+1],src[at+2],a);
        }
    }
}

/// The v0.55.03 baked layer contains one uniformly scaled mountain sprite plus
/// mirrored repetitions of that same layer to fill leftward. Its top remains
/// locked at native y=450 and its bottom lands at y=533, approximately two
/// pixels behind the waterfront waterline. It deliberately
/// does not set the static-scenery occlusion mask; catalogue stars remain
/// governed only by the astronomical horizon/projection rules.
pub fn draw_mountains(c:&mut Canvas) {
    blit(c,MOUNTAINS,MOUNTAIN_W,MOUNTAIN_H,0,MOUNTAIN_Y);
}

/// The v0.55.03 full-width foreground layer contains the uniformly scaled,
/// bottom-right waterfront/coop plus mirrored continuation of its own water.
/// It remains confined to the accepted bottom scenery band and contains no fauna.
pub fn draw_island(c:&mut Canvas) {
    blit(c,WATERFRONT,WATERFRONT_W,WATERFRONT_H,WATERFRONT_X,WATERFRONT_Y);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn baked_assets_have_expected_sizes() {
        assert_eq!(MOUNTAINS.len(),MOUNTAIN_W*MOUNTAIN_H*4);
        assert_eq!(WATERFRONT.len(),WATERFRONT_W*WATERFRONT_H*4);
    }
    #[test]
    fn placement_matches_v05503_contract() {
        assert_eq!((MOUNTAIN_Y,MOUNTAIN_W,MOUNTAIN_H),(450,960,83));
        assert_eq!((WATERFRONT_X,WATERFRONT_Y,WATERFRONT_W,WATERFRONT_H),(0,480,960,120));
        assert_eq!(MOUNTAIN_Y+MOUNTAIN_H as i32,533);
    }
}
