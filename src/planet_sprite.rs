//! v0.55.391 planet artwork: transparent offline-baked frames with bounded RAM.
//!
//! The complete atlases stay embedded in the executable as clean file-backed
//! data. Runtime heap residency contains complete atlases only for planets that
//! actually project into the camera viewport. Leaving the viewport drops that
//! owned atlas immediately; copied embedded backing pages are returned to Linux
//! with MADV_DONTNEED and can fault back from the executable on re-entry.
use crate::pixel::{Canvas, ART_SCALE};
use crate::sky::{PlanetKind,PlanetVisual};
use crate::world::FPS;

pub(crate) const FRAMES: usize = 24;
const FRAME_TICKS:u64 = FPS / 2; // 500 ms per pose, 12-second loop at 30Hz.
const MERCURY_FRAMES:usize=4;
const MARS_FRAMES:usize=8;
const NEPTUNE_FRAMES:usize=8;
const PLUTO_FRAMES:usize=12;
const URANUS_FRAMES:usize=12;
const MERCURY_FRAME_TICKS:u64=FPS*6; // 6-second hold: 4 frames in 24 seconds.
const MARS_FRAME_TICKS:u64=FPS*3; // 3-second hold: 8 frames in 24 seconds.
const NEPTUNE_FRAME_TICKS:u64=FPS*3/2; // 1.5s hold: eight frames in 12s.
const PLUTO_FRAME_TICKS:u64=FPS*8/3; // 80 ticks: 12 frames in 32s.
const URANUS_FRAME_TICKS:u64=FPS*5/2; // 75 ticks: 12 frames in 30s.
const JUPITER: &[u8; FRAMES * 184 * 184 * 4] =
    include_bytes!("../assets/planets/jupiter_24x184x184.rgba");
const SATURN: &[u8; FRAMES * 184 * 76 * 4] =
    include_bytes!("../assets/planets/saturn_24x184x76.rgba");
const VENUS: &[u8; FRAMES * 93 * 93 * 4] =
    include_bytes!("../assets/planets/venus_24x93x93.rgba");
const MERCURY: &[u8; MERCURY_FRAMES * 64 * 64 * 4] =
    include_bytes!("../assets/planets/mercury_4x64x64.rgba");
const MARS: &[u8; MARS_FRAMES * 112 * 112 * 4] =
    include_bytes!("../assets/planets/mars_8x112x112.rgba");
const NEPTUNE: &[u8; NEPTUNE_FRAMES * 128 * 128 * 4] =
    include_bytes!("../assets/planets/neptune_8x128x128.rgba");
const PLUTO: &[u8; PLUTO_FRAMES * 64 * 64 * 4] =
    include_bytes!("../assets/planets/pluto_12x64x64.rgba");
const URANUS: &[u8; URANUS_FRAMES * 120 * 120 * 4] =
    include_bytes!("../assets/planets/uranus_12x120x120.rgba");

#[inline]
pub(crate) fn frame_key(tick:u64)->u64 {tick/FRAME_TICKS%FRAMES as u64}

/// Frame phase must match the damage-tracking key in pixel.rs.
#[inline]
pub(crate) fn frame_key_for(kind:PlanetKind,tick:u64)->u64 {
    match kind {
        PlanetKind::Mercury => tick/MERCURY_FRAME_TICKS%MERCURY_FRAMES as u64,
        PlanetKind::Mars => tick/MARS_FRAME_TICKS%MARS_FRAMES as u64,
        PlanetKind::Neptune => tick/NEPTUNE_FRAME_TICKS%NEPTUNE_FRAMES as u64,
        PlanetKind::Pluto => tick/PLUTO_FRAME_TICKS%PLUTO_FRAMES as u64,
        PlanetKind::Uranus => tick/URANUS_FRAME_TICKS%URANUS_FRAMES as u64,
        _ => frame_key(tick),
    }
}

pub(crate) fn dimensions(kind:PlanetKind)->Option<(usize,usize)> {
    // Earth is intentionally data-model-only: the observer body has no sky atlas.
    match kind {
        PlanetKind::Earth=>None,
        PlanetKind::Jupiter=>Some((184,184)),
        PlanetKind::Saturn=>Some((184,76)),
        PlanetKind::Venus=>Some((93,93)),
        PlanetKind::Mercury|PlanetKind::Pluto=>Some((64,64)),
        PlanetKind::Uranus=>Some((120,120)),
        PlanetKind::Mars=>Some((112,112)),
        PlanetKind::Neptune=>Some((128,128)),
    }
}

/// Native pixel bounds for damage tracking and foreground restoration.
pub(crate) fn native_bounds(kind:PlanetKind,x:i32,y:i32)->(i32,i32,i32,i32) {
    if let Some((w,h))=dimensions(kind) {
        (x*ART_SCALE as i32-w as i32/2,
         y*ART_SCALE as i32-h as i32/2,w as i32,h as i32)
    } else {
        let r=kind.radius()+6;
        ((x-r)*ART_SCALE as i32,(y-r)*ART_SCALE as i32,
         (2*r+1)*ART_SCALE as i32,(2*r+1)*ART_SCALE as i32)
    }
}

fn atlas(kind:PlanetKind)->Option<&'static [u8]> {
    Some(match kind {
        PlanetKind::Jupiter=>&JUPITER[..],PlanetKind::Saturn=>&SATURN[..],
        PlanetKind::Venus=>&VENUS[..],PlanetKind::Mercury=>&MERCURY[..],
        PlanetKind::Mars=>&MARS[..],PlanetKind::Neptune=>&NEPTUNE[..],
        PlanetKind::Pluto=>&PLUTO[..],PlanetKind::Uranus=>&URANUS[..],
        PlanetKind::Earth=>return None,
    })
}
fn atlas_source(kind:PlanetKind)->Option<(usize,usize,&'static [u8])> {
    let (w,h)=dimensions(kind)?;
    Some((w,h,atlas(kind)?))
}

struct ResidentAtlas {kind:PlanetKind,w:usize,h:usize,pixels:Vec<u8>}

/// Current-view artwork residency. Eight coordinate states may exist, but
/// full RGBA atlas memory exists only for bodies actually visible in the camera.
pub(crate) struct PlanetSpriteCache {atlases:Vec<ResidentAtlas>}
impl PlanetSpriteCache {
    pub(crate) fn new()->Self {Self{atlases:Vec::with_capacity(4)}}
    pub(crate) fn sync_visible(&mut self,planets:&[Option<PlanetVisual>;8]) {
        self.atlases.retain(|atlas|planets.iter().flatten().any(|planet|planet.kind==atlas.kind));
        for planet in planets.iter().flatten() {
            if self.atlases.iter().any(|atlas|atlas.kind==planet.kind) {continue;}
            if let Some((w,h,source))=atlas_source(planet.kind) {
                let pixels=source.to_vec();
                discard_embedded_residency(source);
                self.atlases.push(ResidentAtlas{kind:planet.kind,w,h,pixels});
            }
        }
    }
    pub(crate) fn draw(&self,c:&mut Canvas,kind:PlanetKind,x:i32,y:i32,tick:u64)->bool {
        let Some(atlas)=self.atlases.iter().find(|atlas|atlas.kind==kind) else{return false;};
        let frame_bytes=atlas.w*atlas.h*4;
        let start=frame_key_for(kind,tick) as usize*frame_bytes;
        let Some(pixels)=atlas.pixels.get(start..start+frame_bytes) else{return false;};
        draw_frame(c,kind,x,y,atlas.w,atlas.h,pixels);
        true
    }
    #[cfg(test)]
    fn resident_bytes(&self)->usize {self.atlases.iter().map(|atlas|atlas.pixels.len()).sum()}
    #[cfg(test)]
    fn resident_count(&self)->usize {self.atlases.len()}
}
impl Default for PlanetSpriteCache {fn default()->Self{Self::new()}}

#[cfg(target_os="linux")]
fn discard_embedded_residency(data:&'static [u8]) {
    if data.is_empty(){return;}
    // The visible planet's complete atlas has already been copied into owned heap RAM.
    // Its include_bytes! source is clean executable-backed data, so Linux may
    // discard those pages now and fault them back from disk on a later re-entry.
    unsafe {
        let page=libc::sysconf(libc::_SC_PAGESIZE);
        if page<=0{return;}
        let page=page as usize;
        let start=data.as_ptr() as usize;
        let end=start.saturating_add(data.len());
        let aligned_start=start/page*page;
        let aligned_end=end.saturating_add(page-1)/page*page;
        if aligned_end>aligned_start {
            let _=libc::madvise(aligned_start as *mut libc::c_void,
                aligned_end-aligned_start,libc::MADV_DONTNEED);
        }
    }
}
#[cfg(not(target_os="linux"))]
fn discard_embedded_residency(_data:&'static [u8]) {}

#[inline]
fn over(bg:u32,r:u32,g:u32,b:u32,a:u32)->u32 {
    let inv=255-a;
    let red=(((bg>>16)&255)*inv+r*a+127)/255;
    let green=(((bg>>8)&255)*inv+g*a+127)/255;
    let blue=((bg&255)*inv+b*a+127)/255;
    (red<<16)|(green<<8)|blue
}

fn draw_frame(c:&mut Canvas,kind:PlanetKind,x:i32,y:i32,w:usize,h:usize,pixels:&[u8]) {
    let (left,top,_,_)=native_bounds(kind,x,y);
    if !c.overlaps(left,top,w as i32,h as i32) {return;}
    let stride=c.w*c.scale;
    let native_h=c.h*c.scale;
    let x0=(-left).max(0) as usize;
    let y0=(-top).max(0) as usize;
    let x1=(stride as i32-left).clamp(0,w as i32) as usize;
    let y1=(native_h as i32-top).clamp(0,h as i32) as usize;
    if x0>=x1 || y0>=y1 {return;}
    for py in y0..y1 {
        let cy=(top+py as i32) as usize;
        let dest=cy*stride;
        let row=py*w*4;
        for px in x0..x1 {
            let cx=(left+px as i32) as usize;
            if !c.touched(cx,cy) {continue;}
            let off=row+px*4;
            let a=pixels[off+3] as u32;
            if a==0 {continue;}
            let pixel=((pixels[off] as u32)<<16)
                |((pixels[off+1] as u32)<<8)|pixels[off+2] as u32;
            let target=&mut c.cells[dest+cx];
            *target=if a==255 {pixel} else {
                over(*target,pixels[off] as u32,pixels[off+1] as u32,
                    pixels[off+2] as u32,a)
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Point;
    fn visual(kind:PlanetKind,x:i32,y:i32)->PlanetVisual {
        PlanetVisual{kind,center:Point{x,y},radius:kind.radius(),color:kind.color(),
            accent:kind.accent(),ring:kind.has_ring()}
    }
    #[test]
    fn exact_baked_atlas_dimensions_and_frame_timing() {
        assert_eq!(JUPITER.len(),24*184*184*4);
        assert_eq!(SATURN.len(),24*184*76*4);
        assert_eq!(VENUS.len(),24*93*93*4);
        assert_eq!(MERCURY.len(),4*64*64*4);
        assert_eq!(MARS.len(),8*112*112*4);
        assert_eq!(NEPTUNE.len(),8*128*128*4);
        assert_eq!(PLUTO.len(),12*64*64*4);
        assert_eq!(URANUS.len(),12*120*120*4);
        assert_eq!(frame_key(0),0);assert_eq!(frame_key(14),0);
        assert_eq!(frame_key(15),1);assert_eq!(frame_key(359),23);assert_eq!(frame_key(360),0);
        for (kind,hold,count) in [(PlanetKind::Mercury,180,4),(PlanetKind::Mars,90,8),
                                  (PlanetKind::Neptune,45,8),(PlanetKind::Pluto,80,12),
                                  (PlanetKind::Uranus,75,12)] {
            assert_eq!(frame_key_for(kind,0),0);assert_eq!(frame_key_for(kind,hold-1),0);
            assert_eq!(frame_key_for(kind,hold),1);assert_eq!(frame_key_for(kind,hold*count-1),count-1);
            assert_eq!(frame_key_for(kind,hold*count),0);
        }
        assert_eq!(frame_key_for(PlanetKind::Jupiter,15),1);
    }
    #[test]
    fn current_view_cache_owns_only_visible_atlases_and_drops_offscreen_atlases() {
        let mut cache=PlanetSpriteCache::new();
        let mars=visual(PlanetKind::Mars,160,70);
        cache.sync_visible(&[Some(mars),None,None,None,None,None,None,None]);
        assert_eq!(cache.resident_count(),1);
        assert_eq!(cache.resident_bytes(),MARS_FRAMES*112*112*4);
        let jupiter=visual(PlanetKind::Jupiter,170,80);
        cache.sync_visible(&[Some(mars),Some(jupiter),None,None,None,None,None,None]);
        assert_eq!(cache.resident_count(),2);
        assert_eq!(cache.resident_bytes(),MARS_FRAMES*112*112*4+FRAMES*184*184*4);
        cache.sync_visible(&[Some(jupiter),None,None,None,None,None,None,None]);
        assert_eq!(cache.resident_count(),1);
        assert_eq!(cache.resident_bytes(),FRAMES*184*184*4);
        cache.sync_visible(&[None;8]);
        assert_eq!(cache.resident_count(),0);
        assert_eq!(cache.resident_bytes(),0);
    }
    #[test]
    fn all_studies_have_distinct_motion_and_transparent_corners() {
        for (w,h,data) in [(184,184,&JUPITER[..]),(184,76,&SATURN[..]),(93,93,&VENUS[..]),
                            (64,64,&MERCURY[..]),(112,112,&MARS[..]),(128,128,&NEPTUNE[..]),
                            (64,64,&PLUTO[..]),(120,120,&URANUS[..])] {
            assert_eq!(data[3],0,"transparent sprite corner");
            let size=w*h*4;assert_ne!(&data[..size], &data[size..size*2]);
            assert!(data[..size].chunks_exact(4).any(|p|p[3]>200));
        }
    }
    #[test]
    fn earth_has_no_sprite_and_all_sky_bodies_have_native_dimensions() {
        assert_eq!(dimensions(PlanetKind::Mercury),Some((64,64)));
        assert_eq!(dimensions(PlanetKind::Mars),Some((112,112)));
        assert_eq!(dimensions(PlanetKind::Neptune),Some((128,128)));
        assert_eq!(dimensions(PlanetKind::Uranus),Some((120,120)));
        assert_eq!(dimensions(PlanetKind::Pluto),Some((64,64)));
        assert_eq!(dimensions(PlanetKind::Earth),None);
    }
    #[test]
    fn clipped_cached_planet_draw_keeps_native_canvas_dimensions() {
        let mut c=Canvas::scaled(320,200,ART_SCALE,false);
        let planets=[visual(PlanetKind::Jupiter,-3,1),visual(PlanetKind::Saturn,102,61),
            visual(PlanetKind::Venus,53,23),visual(PlanetKind::Mars,50,23)];
        let mut cache=PlanetSpriteCache::new();
        let visible=[Some(planets[0]),Some(planets[1]),Some(planets[2]),Some(planets[3]),None,None,None,None];
        cache.sync_visible(&visible);
        for p in planets {assert!(cache.draw(&mut c,p.kind,p.center.x,p.center.y,0));}
        assert_eq!(c.cells.len(),960*600);
    }
}
