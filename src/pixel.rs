//! Native 960x600 software framebuffer. World art uses 3x3 native blocks. Scene details
//! belong in pixel_scenery.rs; individual sprites in pixel_sprite.rs.
use crate::pixel_scenery;
use crate::billboard::Billboard;
use crate::telemetry::Snapshot;
use crate::pixel_sprite;
use crate::pixel_birds;
use crate::staging;
use crate::lunar::MoonSprite;
use crate::solar::SunSprite;
use crate::planet_sprite;
use crate::constellations::ConstellationOverlay;
use crate::world::World;

pub const ART_SCALE: usize=3; // Legacy coordinates painted directly as native 3x3 blocks.
pub const WIDTH: usize=960;
pub const HEIGHT: usize=600;
// Small fixed tiles turn rectangular damage into fast row spans. Exact pixel
// output is maintained: only covered cells may change, and all painter layers
// retain their original order. No allocations in the frame loop.
pub(crate) const TILE: usize=16;
pub(crate) const TILES_X:usize=WIDTH.div_ceil(TILE);
const TILES_Y:usize=HEIGHT.div_ceil(TILE);
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
struct Rect {x:i32,y:i32,w:i32,h:i32}
impl Rect {
    fn new(x:i32,y:i32,w:i32,h:i32)->Self {Self{x,y,w,h}}
    fn logical(x:i32,y:i32,w:i32,h:i32)->Self {
        Self::new(x*ART_SCALE as i32,y*ART_SCALE as i32,
            w*ART_SCALE as i32,h*ART_SCALE as i32)
    }
}

// Native-pixel floating readout: preserves every live sysmon meter/process row.
// Maximum dirty footprint for the upper-right simulated-time HUD. The visible
// box is content-sized and right-anchored so SPACE TIME MACHINE does not
// force a permanently oversized panel. Keep this larger region for clean
// repaint when the live speed text grows or contracts.
const FF_STATUS_DAMAGE_RECT:(i32,i32,i32,i32)=(WIDTH as i32-300,0,300,68);
// View and geolocation are different control planes; keep discovery text in
// the existing right-hand HUD instead of introducing another permanent box.
const FF_CONTROL_HINTS:[&str;2]=[
    "WASD: VIEW  ARROWS: LOCATION",
    "R: RESET VIEW  ESC: COMMANDS",
];
const FF_STATUS_VISIBLE_HEIGHT:i32=64;
// Transient navigation feedback is independent of the persistent HUD toggle so
// a cancellation acknowledgement is always visible.
const GIMBAL_MESSAGE_DAMAGE_RECT:(i32,i32,i32,i32)=(WIDTH as i32/2-72,8,144,20);
// v0.55.04 ADMIN-1 overlay: a separate fixed dirty footprint directly below
// the live coordinate HUD. Source polygon complexity never reaches the frame
// loop; the current region is reduced to a tiny bounded wireframe bitmap.
const ADMIN1_DAMAGE_RECT:(i32,i32,i32,i32)=(WIDTH as i32-420,FF_STATUS_VISIBLE_HEIGHT,420,68);
const ADMIN1_WIREFRAME_W:u16=96;
const ADMIN1_WIREFRAME_H:u16=38;
// v0.51.03b keeps the compact summary panel; each visible diagnostic star now also
// carries a tiny crosshair centered on its already-projected catalogue-star pixel.
// The camera panel deliberately avoids the existing Time Machine HUD on the right.
// Maximum damage footprint; visible panel width is calculated from its live text.
const STAR_DIAGNOSTIC_PANEL_RECT:(i32,i32,i32,i32)=(0,0,420,28);
const STAR_RETICLE_RECT:(i32,i32,i32,i32)=(WIDTH as i32/2-8,HEIGHT as i32/2-8,130,17);
// Modern Command Center uses the agreed 640x480 shell at 960x600.
pub(crate) const COMMAND_CENTER_W:usize=640;
pub(crate) const COMMAND_CENTER_H:usize=480;
const TIME_PANEL_RECT:(i32,i32,i32,i32)=(160,60,COMMAND_CENTER_W as i32,COMMAND_CENTER_H as i32);
const TIME_PANEL_TITLE_Y:i32=10;
const TIME_PANEL_CONTENT_TOP:i32=36;
const TIME_PANEL_LINE_HEIGHT:i32=12;
const TIME_PANEL_FOOTER_Y:i32=374;
const TIME_PANEL_MESSAGE_Y:i32=408;
const TIME_PANEL_INPUT_BOX_Y:i32=438;
const TIME_PANEL_INPUT_TEXT_Y:i32=445;
const DIAGNOSTIC_CROSSHAIR_RADIUS:i32=3;
const DIAGNOSTIC_CROSSHAIR_RGB:u32=0xf3d9a3;
const HORIZON_RGB:u32=0x71858b;
const HORIZON_LABEL:&str="HORIZON";

pub const LOGICAL_W: usize=WIDTH/ART_SCALE;
pub const LOGICAL_H: usize=HEIGHT/ART_SCALE;

// A native framebuffer with optional logical-coordinate drawing. For the zoo,
// 320x200 game coordinates paint directly into 960x600 physical pixels.
// There is no intermediate 320x200 full-scene image or upscale pass.
// Ordinary small offscreen canvases (sprite tests etc.) keep scale=1.
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<u32>,
    pub scale: usize,
    pub(crate) occluded: Option<Vec<bool>>,
    // Enabled only on the main scene, not the static scenery or sprites.
    pub(crate) damage: Option<Vec<bool>>,
}
impl Canvas {
    pub fn new(w: usize, h: usize) -> Self { Self::scaled(w,h,1,false) }
    fn native_scene() -> Self { Self::scaled(LOGICAL_W,LOGICAL_H,ART_SCALE,false) }
    fn native_scenery() -> Self { Self::scaled(LOGICAL_W,LOGICAL_H,ART_SCALE,true) }
    pub fn scaled(w:usize,h:usize,scale:usize,mask:bool) -> Self {
        assert!(scale >= 1);
        Self { w, h, scale, cells: vec![0; w*h*scale*scale],
            occluded: if mask {Some(vec![false;w*h])} else {None}, damage:None }
    }
    fn enable_damage(&mut self) {
        if self.damage.is_none() { self.damage=Some(vec![false;TILES_X*TILES_Y]); }
        self.damage.as_mut().unwrap().fill(false);
    }
    fn mark(&mut self,r:Rect) {
        let x0=r.x.max(0) as usize/TILE;
        let y0=r.y.max(0) as usize/TILE;
        let x1=(r.x.saturating_add(r.w)).clamp(0,WIDTH as i32) as usize;
        let y1=(r.y.saturating_add(r.h)).clamp(0,HEIGHT as i32) as usize;
        if x1==0 || y1==0 || x0*TILE>=x1 || y0*TILE>=y1 {return;}
        let right=(x1-1)/TILE+1;
        let bottom=(y1-1)/TILE+1;
        let damage=self.damage.as_mut().expect("enabled damage map");
        for y in y0..bottom {
            damage[y*TILES_X+x0..y*TILES_X+right].fill(true);
        }
    }
    fn mark_all(&mut self) {
        self.damage.as_mut().expect("enabled damage map").fill(true);
    }
    // Mark only pixels traversed by a thin line, not its often-large rectangle.
    fn mark_line(&mut self,line:&crate::constellation_render::Line) {
        let (mut x,mut y)=(line.x0,line.y0);
        let dx=(line.x1-line.x0).abs();let dy=-(line.y1-line.y0).abs();
        let sx=if x<line.x1 {1}else{-1};let sy=if y<line.y1 {1}else{-1};
        let mut err=dx+dy;
        loop {
            let tile=(y as usize/TILE)*TILES_X+x as usize/TILE;
            self.damage.as_mut().expect("enabled damage map")[tile]=true;
            if x==line.x1 && y==line.y1 {break;}
            let twice=2*err;
            if twice>=dy {err+=dy;x+=sx;}
            if twice<=dx {err+=dx;y+=sy;}
        }
    }
    pub(crate) fn touched(&self,x:usize,y:usize)->bool {
        self.damage.as_ref().map_or(true,|map|map[(y/TILE)*TILES_X+x/TILE])
    }
    pub(crate) fn overlaps(&self,x:i32,y:i32,w:i32,h:i32)->bool {
        let Some(map)=self.damage.as_ref() else {return true;};
        let x0=x.max(0) as usize/TILE;
        let y0=y.max(0) as usize/TILE;
        let x1=(x.saturating_add(w)).clamp(0,WIDTH as i32) as usize;
        let y1=(y.saturating_add(h)).clamp(0,HEIGHT as i32) as usize;
        if x1==0 || y1==0 || x0*TILE>=x1 || y0*TILE>=y1{return false;}
        for row in y0..(y1-1)/TILE+1 {
            if map[row*TILES_X+x0..row*TILES_X+(x1-1)/TILE+1].iter().any(|&v|v) {return true;}
        }
        false
    }
    fn restore_marked(&mut self,scenery:&Canvas) {
        let map=self.damage.as_ref().expect("enabled damage map");
        for ty in 0..TILES_Y {
            let y0=ty*TILE;
            let y1=((ty+1)*TILE).min(HEIGHT);
            let mut tx=0;
            while tx<TILES_X {
                if !map[ty*TILES_X+tx] {tx+=1;continue;}
                let start=tx;
                while tx<TILES_X && map[ty*TILES_X+tx] {tx+=1;}
                let left=start*TILE;
                let right=(tx*TILE).min(WIDTH);
                for row in y0..y1 {
                    let a=row*WIDTH+left;let b=row*WIDTH+right;
                    self.cells[a..b].copy_from_slice(&scenery.cells[a..b]);
                }
            }
        }
    }
    pub(crate) fn regions(&self)->Option<&[bool]> {self.damage.as_deref()}
    pub fn clear(&mut self, rgb:u32) {
        self.cells.fill(rgb);
        if let Some(mask)=&mut self.occluded {mask.fill(false);}
    }
    // Pixel coordinate space of the underlying storage, not game coordinates.
    pub fn native_put(&mut self,x:i32,y:i32,rgb:u32) {
        if x>=0 && y>=0 && x<(self.w*self.scale) as i32 && y<(self.h*self.scale) as i32 && self.touched(x as usize,y as usize) {
            self.cells[y as usize*self.w*self.scale+x as usize]=rgb;
        }
    }
    pub fn native_rect(&mut self,x:i32,y:i32,w:i32,h:i32,rgb:u32) {
        let stride=self.w*self.scale;
        let x0=x.max(0) as usize;
        let y0=y.max(0) as usize;
        let x1=(x+w).min(stride as i32).max(0) as usize;
        let y1=(y+h).min((self.h*self.scale) as i32).max(0) as usize;
        if x0>=x1 {return;}
        if self.damage.is_none() {
            for yy in y0..y1 {self.cells[yy*stride+x0..yy*stride+x1].fill(rgb);}
        } else {
            for yy in y0..y1 {
                let mut xx=x0;
                while xx<x1 {
                    if !self.touched(xx,yy) {xx=((xx/TILE+1)*TILE).min(x1);continue;}
                    let right=((xx/TILE+1)*TILE).min(x1);
                    self.cells[yy*stride+xx..yy*stride+right].fill(rgb);
                    xx=right;
                }
            }
        }
    }
    pub fn put(&mut self,x:i32,y:i32,rgb:u32) {
        if x>=0 && y>=0 && x<self.w as i32 && y<self.h as i32 {
            let x=x as usize;
            let y=y as usize;
            let sw=self.w*self.scale;
            let left=x*self.scale;
            if self.damage.is_none() {
                for sy in y*self.scale..(y+1)*self.scale {
                    self.cells[sy*sw+left..sy*sw+left+self.scale].fill(rgb);
                }
            } else {
                for sy in y*self.scale..(y+1)*self.scale {
                    for sx in left..left+self.scale {
                        if self.touched(sx,sy) {self.cells[sy*sw+sx]=rgb;}
                    }
                }
            }
            if let Some(mask)=&mut self.occluded {mask[y*self.w+x]=true;}
        }
    }
    pub fn rect(&mut self,x:i32,y:i32,w:i32,h:i32,rgb:u32){
        let x0=x.max(0);let y0=y.max(0);
        let x1=(x+w).min(self.w as i32);
        let y1=(y+h).min(self.h as i32);
        if x1<=x0 || y1<=y0 {return;}
        self.native_rect(x0*self.scale as i32,y0*self.scale as i32,
            (x1-x0)*self.scale as i32,(y1-y0)*self.scale as i32,rgb);
        if let Some(mask)=&mut self.occluded {
            for yy in y0 as usize..y1 as usize {
                mask[yy*self.w+x0 as usize..yy*self.w+x1 as usize].fill(true);
            }
        }
    }
    pub fn disc(&mut self,x:i32,y:i32,r:i32,rgb:u32){
        for dy in -r..=r {for dx in -r..=r {
            if dx*dx+dy*dy<=r*r {self.put(x+dx,y+dy,rgb);}
        }}
    }
}

fn catalogue_star_visible(scenery:&Canvas,star:space_zoo::star_projection::StarPixel)->bool {
    if star.x<0 || star.y<0 || star.x>=WIDTH as i32 || star.y>=HEIGHT as i32 {return false;}
    let mask=scenery.occluded.as_ref().expect("scenery has occlusion mask");
    let logical_x=star.x as usize/ART_SCALE;
    let logical_y=star.y as usize/ART_SCALE;
    !mask[logical_y*LOGICAL_W+logical_x]
}

fn draw_catalogue_star_layer(c:&mut Canvas,scenery:&Canvas,stars:&[crate::star_map::RenderedStar]) {
    for &star in stars {
        if catalogue_star_visible(scenery,star.pixel) {c.native_put(star.pixel.x,star.pixel.y,star.rgb);}
    }
}

fn blend_rgb(base:u32,overlay:u32,alpha:f64)->u32 {
    let alpha=alpha.clamp(0.0,1.0);
    let channel=|shift:u32| -> u32 {
        let a=((base>>shift)&0xff) as f64;
        let b=((overlay>>shift)&0xff) as f64;
        (a+(b-a)*alpha).round().clamp(0.0,255.0) as u32
    };
    (channel(16)<<16)|(channel(8)<<8)|channel(0)
}

fn horizon_label_rect(points:&[space_zoo::star_projection::StarPixel])->Option<Rect> {
    let center_x=WIDTH as i32/2;
    let anchor=points.iter().min_by_key(|p|(p.x-center_x).abs())?;
    let width=HORIZON_LABEL.chars().count() as i32*6;
    let x=center_x-width/2;
    let y=anchor.y+5;
    (y>=0 && y+7<=HEIGHT as i32).then_some(Rect::new(x,y,width,7))
}

fn draw_horizon_layer(c:&mut Canvas,scenery:&Canvas,points:&[space_zoo::star_projection::StarPixel]) {
    for &pixel in points {
        if catalogue_star_visible(scenery,pixel) {c.native_put(pixel.x,pixel.y,HORIZON_RGB);}
    }
    // The geometric alt=0 great circle is the astronomical horizon. Keep its
    // compact designator centered in the view, directly beneath the projected
    // dotted line, rather than attaching it to an arbitrary sampled dash.
    if let Some(label)=horizon_label_rect(points) {
        crate::billboard::overlay_text(c,label.x,label.y,HORIZON_LABEL,1,HORIZON_RGB);
    }
}

fn culture_line_visible(scenery:&Canvas,line:&crate::constellation_render::Line)->bool {
    let (mut x,mut y)=(line.x0,line.y0);
    let dx=(line.x1-line.x0).abs();let dy=-(line.y1-line.y0).abs();
    let sx=if x<line.x1 {1}else{-1};let sy=if y<line.y1 {1}else{-1};
    let mut err=dx+dy;
    loop {
        if catalogue_star_visible(scenery,space_zoo::star_projection::StarPixel{x,y}) {return true;}
        if x==line.x1 && y==line.y1 {return false;}
        let twice=2*err;
        if twice>=dy {err+=dy;x+=sx;}
        if twice<=dx {err+=dx;y+=sy;}
    }
}

fn draw_culture_layer(c:&mut Canvas,scenery:&Canvas,picture:&crate::constellation_render::Picture,
    visibility:f64) {
    for line in &picture.lines {
        let (mut x,mut y)=(line.x0,line.y0);
        let dx=(line.x1-line.x0).abs();let dy=-(line.y1-line.y0).abs();
        let sx=if x<line.x1 {1}else{-1};let sy=if y<line.y1 {1}else{-1};
        let mut err=dx+dy;
        loop {
            if c.touched(x as usize,y as usize) &&
                catalogue_star_visible(scenery,space_zoo::star_projection::StarPixel{x,y}) {
                let base=c.cells[y as usize*WIDTH+x as usize];
                c.native_put(x,y,blend_rgb(base,crate::constellation_render::LINE_RGB,visibility));
            }
            if x==line.x1 && y==line.y1 {break;}
            let twice=2*err;
            if twice>=dy {err+=dy;x+=sx;}
            if twice<=dx {err+=dx;y+=sy;}
        }
    }
    // Compact metadata labels are retained behind all protected UI and moving actors.
    for name in &picture.names {
        if picture.lines[name.first_line..name.end_line].iter().any(|line|culture_line_visible(scenery,line)) &&
            catalogue_star_visible(scenery,space_zoo::star_projection::StarPixel{x:name.x,y:name.y}) {
            let sample=c.cells[name.y as usize*WIDTH+name.x as usize];
            let name_rgb=blend_rgb(sample,crate::constellation_render::NAME_RGB,visibility);
            crate::billboard::overlay_text(c,name.x,name.y,&name.text,1,name_rgb);
            // Keep glyph ink below the same logical static-occlusion mask as star dots.
            let mask=scenery.occluded.as_ref().expect("scenery has occlusion mask");
            for yy in name.y..name.y+7 {
                for xx in name.x..name.x+name.text.chars().count() as i32*6 {
                    if xx>=0 && yy>=0 && xx<WIDTH as i32 && yy<HEIGHT as i32 &&
                        mask[(yy as usize/ART_SCALE)*LOGICAL_W+xx as usize/ART_SCALE] &&
                        c.touched(xx as usize,yy as usize) {
                        let offset=yy as usize*WIDTH+xx as usize;
                        c.cells[offset]=scenery.cells[offset];
                    }
                }
            }
        }
    }
}

fn diagnostic_label_text(point:&crate::star_map::DiagnosticPoint)->String {
    format!("{} | {} | ALT {:+.2} | AZ {:06.2} | X {} | Y {}",
        point.name,point.id,point.altitude_deg,point.azimuth_deg,
        point.pixel.x,point.pixel.y)
}

fn diagnostic_label_rect_for_text(point:&crate::star_map::DiagnosticPoint,text:&str)->Rect {
    let width=(text.chars().count() as i32*6).max(6);
    let mut x=point.pixel.x+4;
    if x+width>=WIDTH as i32 {x=point.pixel.x-4-width;}
    let y=(point.pixel.y-3).clamp(0,HEIGHT as i32-7);
    Rect::new(x,y,width,7)
}

fn diagnostic_label_rect(point:&crate::star_map::DiagnosticPoint)->Rect {
    let text=diagnostic_label_text(point);
    diagnostic_label_rect_for_text(point,&text)
}

fn diagnostic_crosshair_rect(point:&crate::star_map::DiagnosticPoint)->Rect {
    let r=DIAGNOSTIC_CROSSHAIR_RADIUS;
    Rect::new(point.pixel.x-r,point.pixel.y-r,r*2+1,r*2+1)
}

fn draw_diagnostic_crosshair(c:&mut Canvas,point:&crate::star_map::DiagnosticPoint) {
    // Presentation-only marker: its center is the exact existing projected star pixel.
    // The arms never supply or modify a sky position; the catalogue dot is restored at
    // the center after the marker is painted.
    let x=point.pixel.x;
    let y=point.pixel.y;
    let r=DIAGNOSTIC_CROSSHAIR_RADIUS;
    c.native_rect(x-r,y,r*2+1,1,DIAGNOSTIC_CROSSHAIR_RGB);
    c.native_rect(x,y-r,1,r*2+1,DIAGNOSTIC_CROSSHAIR_RGB);
    c.native_put(x,y,point.rgb);
}

fn draw_star_diagnostic_overlay(
    c:&mut Canvas,
    scenery:&Canvas,
    map:&crate::star_map::StarMap,
    label_rects:&mut Vec<Rect>,
    show_hud:bool,
) {
    let (px,py,_,ph)=STAR_DIAGNOSTIC_PANEL_RECT;
    let (rx,ry,rw,rh)=STAR_RETICLE_RECT;

    // Diagnostic stars intentionally supersede dynamic scene objects (Sun, Moon,
    // planets, birds, bats, animals), while retaining the normal static-scenery
    // horizon/occlusion rule. Their screen position always comes from DiagnosticPoint.
    // The camera panel below is permanent; :debugstars controls only labels/crosshairs.
    label_rects.clear();
    if map.cultures.debugstars() {
        let mut tagged_indices=[usize::MAX;crate::star_map::DIAGNOSTIC_REGISTRY_LEN];
        let mut tagged_count=0usize;
        for (index,point) in map.diagnostics().iter().enumerate() {
            if !point.rendered || !catalogue_star_visible(scenery,point.pixel) {continue;}
            tagged_indices[tagged_count]=index;
            tagged_count+=1;
        }
        for &index in &tagged_indices[..tagged_count] {
            let point=&map.diagnostics()[index];
            let text=diagnostic_label_text(point);
            let label=diagnostic_label_rect_for_text(point,&text);
            crate::billboard::overlay_text(c,label.x,label.y,&text,1,0xdce8ee);
            label_rects.push(label);
        }
        for &index in &tagged_indices[..tagged_count] {
            let point=&map.diagnostics()[index];
            draw_diagnostic_crosshair(c,point);
        }
    }

    // Protected fixed UI: diagnostic stars/text stay BEHIND this camera debugger.
    // Size the visible panel to the live text; the larger damage footprint above
    // ensures old pixels are repainted when its width contracts.
    if show_hud {
    let vfov=space_zoo::star_projection::vertical_fov_deg(WIDTH,HEIGHT);
    let camera_line=format!(
        "CAMERA ALT {:+.2}° AZ {:06.2}° | FOV H {:.2}° V {:.2}°",
        map.camera().altitude_deg,
        map.camera().azimuth_deg.rem_euclid(360.0),
        space_zoo::star_projection::HORIZONTAL_FOV_DEG,vfov);
    let status_line=if map.cultures.debugstars() {
        let counts=map.counts();
        format!("STARS: {} ABOVE HORIZON / {} IN VIEW / {} DIAGNOSTIC",
            counts.above_horizon,counts.in_view,counts.diagnostic)
    } else {
        format!("CONSTELLATION CULTURE MAP: {}",map.cultures.selected())
    };
    // Native overlay glyphs advance 6 pixels at scale 1; 7 left + 7 right.
    let pw=(camera_line.chars().count().max(status_line.chars().count()) as i32*6+14)
        .min(STAR_DIAGNOSTIC_PANEL_RECT.2);
    if c.overlaps(px,py,pw,ph) {
        c.native_rect(px,py,pw,ph,0x071018);
        c.native_rect(px,py,pw,1,0x87aab8);
        c.native_rect(px,py+ph-1,pw,1,0x314b58);
        c.native_rect(px,py,1,ph,0x526f7c);
        c.native_rect(px+pw-1,py,1,ph,0x314b58);
        crate::billboard::overlay_text(c,px+7,py+4,&camera_line,1,0xe8f1f4);
        crate::billboard::overlay_text(c,px+7,py+16,&status_line,1,0xc5d9e1);
    }

    }

    // Optical-axis reticle remains part of :debugstars rather than permanent UI.
    if map.cultures.debugstars() && c.overlaps(rx,ry,rw,rh) {
        let cx=WIDTH as i32/2;
        let cy=HEIGHT as i32/2;
        c.native_rect(cx-5,cy,11,1,0xf3d9a3);
        c.native_rect(cx,cy-5,1,11,0xf3d9a3);
        crate::billboard::overlay_text(c,cx+8,cy-3,&format!("ALT {:+.0} / AZ {:03.0}",
            map.camera().altitude_deg,map.camera().azimuth_deg.rem_euclid(360.0)),1,0xf3d9a3);
    }
}

fn legacy_constellation_layer_enabled(fast:bool)->bool { !fast }
fn legacy_zoo_scene_enabled(fast:bool)->bool { !fast }

#[derive(Clone,Debug)]
struct Admin1Ui {
    label:String,
    pixels:Vec<(u16,u16)>,
}

fn admin1_overlay_label(found:&crate::admin1_geo::Admin1Match)->(String,bool) {
    if found.country=="__WATER__" {
        (found.admin1.clone(),false)
    } else {
        (found.display_label(),true)
    }
}

pub(crate) fn command_center_visible_rows(height:usize)->usize {
    let content_bottom=(height as i32-114).max(TIME_PANEL_CONTENT_TOP+TIME_PANEL_LINE_HEIGHT);
    ((content_bottom-TIME_PANEL_CONTENT_TOP)/TIME_PANEL_LINE_HEIGHT).max(1) as usize
}

pub(crate) fn paint_command_center(c:&mut Canvas,panel:&crate::time_machine::Panel,
    star_multiplier:f64,cursor_on:bool,x:i32,y:i32) {
    paint_command_center_viewport(c,panel,star_multiplier,cursor_on,x,y,COMMAND_CENTER_W,COMMAND_CENTER_H);
}

pub(crate) fn paint_command_center_viewport(c:&mut Canvas,panel:&crate::time_machine::Panel,
    star_multiplier:f64,cursor_on:bool,x:i32,y:i32,width:usize,height:usize) {
    if !panel.open {return;}
    let native_w=c.w.saturating_mul(c.scale);let native_h=c.h.saturating_mul(c.scale);
    let w=width.min(native_w.saturating_sub(x.max(0) as usize)) as i32;
    let h=height.min(native_h.saturating_sub(y.max(0) as usize)) as i32;
    if w<120||h<150{return;}
    let footer_sep=(h-114).max(TIME_PANEL_CONTENT_TOP+TIME_PANEL_LINE_HEIGHT);
    let footer_y=h-106;let input_sep=h-84;let message_y=h-72;let input_box_y=h-42;let input_text_y=h-35;
    c.native_rect(x,y,w,h,0x071721);
    c.native_rect(x,y,w,1,0x92b8c9);
    c.native_rect(x,y+h-1,w,1,0x455b6c);
    c.native_rect(x,y,1,h,0x45616e);
    c.native_rect(x+w-1,y,1,h,0x45616e);
    let title=format!("SPACE ZOO / {}",panel.view.title());
    crate::billboard::overlay_text(c,x+14,y+TIME_PANEL_TITLE_Y,&title,1,0xe0e9ef);
    let stars=format!("STARS {}X",crate::display_settings::format_multiplier(star_multiplier));
    let stars_w=stars.chars().count() as i32*6;
    if w>14+title.chars().count() as i32*6+18+stars_w {
        crate::billboard::overlay_text(c,x+w-14-stars_w,y+TIME_PANEL_TITLE_Y,&stars,1,0xaec1cb);
    }
    c.native_rect(x+1,y+28,w-2,1,0x45616e);
    for (row,line) in panel.visible_lines().iter().enumerate() {
        let py=y+TIME_PANEL_CONTENT_TOP+row as i32*TIME_PANEL_LINE_HEIGHT;
        if py+7>=y+footer_sep {break;}
        let color=if line.heading {0x9ac9d9}else if line.muted{0xaec1cb}else{0xe0e9ef};
        crate::billboard::overlay_text(c,x+14,py,&line.text,1,color);
    }
    c.native_rect(x+1,y+footer_sep,w-2,1,0x45616e);
    let scroll=panel.scroll_indicator();
    let footer=if panel.lines.len()>panel.visible_lines().len() {
        if w>=560 {format!("UP/DOWN SCROLL | PGUP/PGDN PAGE     {scroll}")}
        else if w>=360 {format!("UP/DOWN SCROLL  {scroll}")}
        else {format!("SCROLL {scroll}")}
    } else {scroll};
    crate::billboard::overlay_text(c,x+14,y+footer_y,&footer,1,0xaec1cb);
    if w>=520 {crate::billboard::overlay_text(c,x+w-68,y+footer_y,"ESC CLOSE",1,0xaec1cb);}
    c.native_rect(x+1,y+input_sep,w-2,1,0x45616e);
    let status=if !panel.error.is_empty(){&panel.error}else{&panel.message};
    if !status.is_empty() {
        let max_chars=((w-28).max(6)/6) as usize;
        let mut shown=status.chars().collect::<Vec<_>>();
        if shown.len()>max_chars {shown.truncate(max_chars);}
        let text:String=shown.into_iter().collect();
        let color=if panel.error.is_empty(){0xe6d6b2}else{0xffa5a5};
        crate::billboard::overlay_text(c,x+14,y+message_y,&text,1,color);
    }
    c.native_rect(x+14,y+input_box_y,(w-28).max(1),22.min(h-input_box_y-1),0x020c12);
    let max_chars=((w-44).max(6)/6) as usize;
    let prompt=if panel.pending {": APPLYING COMMAND...".to_string()} else {
        let chars:Vec<char>=panel.input.chars().collect();
        let tail=if chars.len()>max_chars {chars[chars.len()-max_chars..].iter().collect::<String>()}else{panel.input.clone()};
        format!(":{tail}")
    };
    crate::billboard::overlay_text(c,x+22,y+input_text_y,&prompt,1,0xb9e8bc);
    if !panel.pending && cursor_on {
        let cursor_x=(x+22+prompt.chars().count() as i32*6).min(x+w-7);
        c.native_rect(cursor_x,y+input_text_y,5,7,0xb9e8bc);
    }
}

pub struct PixelRenderer {
    canvas: Canvas,        // Exact native 960x600 scene, no second enlarged copy.
    scenery: Canvas,       // Cached native scenery, rebuilt on hour change.
    cached_hour: Option<u8>,
    cached_minute: Option<u8>,
    cached_sky_minute: Option<u64>,
    cached_fast_daylight: Option<f32>,
    cached_dimensions: (u16, u16),
    depth_order: Vec<usize>,
    billboard: Option<Billboard>,
    moon:MoonSprite,
    sun:SunSprite,
    constellations:ConstellationOverlay,
    coop_art: Canvas,           // 62x41 logical, 3x native; no full-screen layer
    cached_coop: Option<(crate::coop::Coop,bool)>,
    last_stars:Vec<crate::world::Point>,
    star_map:Option<crate::star_map::StarMap>,
    star_map_changed:bool,
    last_sky_light_key:Option<(bool,u8)>,
    visual_change_pending:bool,
    diagnostic_label_rects:Vec<Rect>,
    last_actors:Vec<Rect>,
    current_actors:Vec<Rect>,
    last_orb:Option<(crate::sky::Orb,i32,i32,u64)>,
    // Moon has a separate damage footprint when Sun and Moon share the sky.
    last_concurrent_moon:Option<(crate::sky::Orb,i32,i32,u64)>,
    last_sun_wheel_step:Option<u64>,
    last_sun_mane_step:Option<u64>,
    planet_ephemeris:crate::planet_ephemeris::PlanetEphemeris,
    planet_sprites:planet_sprite::PlanetSpriteCache,
    last_planets:[Option<(crate::sky::PlanetKind,i32,i32,u64)>;8],
    last_constellation_key:Option<i64>,
    last_constellation_daylight:Option<f32>,
    last_billboard_gyro:Option<u64>,
    last_billboard_lighting:Option<bool>,
    billboard_updated:bool,
    last_billboard_date:Option<crate::circadian::ClockDate>,
    last_arc_jewel_day:Option<bool>,
    time_panel:Option<crate::time_machine::Panel>,
    previous_time_panel:Option<crate::time_machine::Panel>,
    last_command_cursor_on:Option<bool>,
    time_hud_changed:bool,
    simulation_hud:Option<crate::billboard::TimeHud>,
    hud_message:Option<(String,std::time::Instant)>,
    hud_message_changed:bool,
    admin1_lookup:Option<crate::admin1_geo::Admin1Lookup>,
    admin1_display:Option<Admin1Ui>,
    admin1_last_location:Option<(u64,u64)>,
    admin1_region_id:Option<u16>,
    admin1_changed:bool,
    last_bipedal_key:Option<crate::bipedal::VisualKey>,
    cryptid_mode:crate::time_machine::CryptidMode,
    planet_appearance:crate::time_machine::PlanetAppearance,
    jumbotron_visible:bool,
    hud_visible:bool,
    background_animals_visible:bool,
}
impl PixelRenderer {
    pub fn new() -> Self {
        let admin1_lookup=match crate::admin1_geo::Admin1Lookup::open(
            "data/admin1/admin1_null_island_1.2.csv") {
            Ok(lookup)=>Some(lookup),
            Err(error)=>{
                crate::logging::error("ADMIN1_ERROR","admin1.open",&error.to_string());
                None
            }
        };
        Self {
            canvas: Canvas::native_scene(),
            scenery: Canvas::native_scenery(),
            cached_hour: None,
            cached_minute: None,
            cached_sky_minute: None,
            cached_fast_daylight: None,
            cached_dimensions: (0, 0),
            depth_order: Vec::with_capacity(28),
            billboard: None,
            moon:MoonSprite::new(),
            sun:SunSprite::new(),
            constellations:ConstellationOverlay::new(),
            coop_art: Canvas::scaled(62,41,ART_SCALE,false),
            cached_coop: None,
            last_stars:Vec::with_capacity(20),
            star_map:None,star_map_changed:false,last_sky_light_key:None,visual_change_pending:false,diagnostic_label_rects:Vec::with_capacity(100),
            last_actors:Vec::with_capacity(40),current_actors:Vec::with_capacity(40),
            last_orb:None,last_concurrent_moon:None,
            last_sun_wheel_step:None,last_sun_mane_step:None,
            planet_ephemeris:crate::planet_ephemeris::PlanetEphemeris::new(),
            planet_sprites:planet_sprite::PlanetSpriteCache::new(),last_planets:[None;8],
            last_constellation_key:None,last_constellation_daylight:None,
            last_billboard_gyro:None,last_billboard_lighting:None,
            billboard_updated:false,last_billboard_date:None,
            time_panel:None,previous_time_panel:None,last_command_cursor_on:None,time_hud_changed:false,
            simulation_hud:None,hud_message:None,hud_message_changed:false,
            admin1_lookup,
            admin1_display:None,admin1_last_location:None,admin1_region_id:None,
            admin1_changed:false,
            last_arc_jewel_day:None,
            last_bipedal_key:None,
            cryptid_mode:crate::time_machine::CryptidMode::Cycle,
            planet_appearance:crate::time_machine::PlanetAppearance::Graphic,
            jumbotron_visible:true,
            hud_visible:true,
            background_animals_visible:false,
        }
    }

    /// Load the accepted production catalogue for the --ff pixel-only star map.
    /// Normal mode never calls this and retains the original decorative stars.
    pub fn enable_star_map(&mut self)->Result<(),crate::star_map::StarMapError> {
        match crate::star_map::StarMap::load() {
            Ok(map)=>{self.star_map=Some(map);crate::logging::recovered("data.star_map");Ok(())},
            Err(error)=>{crate::logging::error("RUNTIME_FAILURE","data.star_map",&error.to_string());Err(error)}
        }
    }

    pub fn set_view_input(&mut self,input:space_zoo::star_projection::ViewInput)->bool {
        self.star_map.as_mut().is_some_and(|map|map.set_view_input(input))
    }

    pub fn view_control(&mut self,control:space_zoo::star_projection::ViewControl)->bool {
        self.star_map.as_mut().is_some_and(|map|map.apply_view_control(control))
    }

    pub fn release_view_control(&mut self,control:space_zoo::star_projection::ViewControl)->bool {
        self.star_map.as_mut().is_some_and(|map|map.release_view_control(control))
    }

    /// Session-only Command Center changes do not alter persistent config.
    pub fn visual_command(&mut self,cmd:&crate::time_machine::Command,world:&mut World,gimbal:&mut crate::GimbalController)
        ->Result<crate::time_machine::CommandReply,String> {
        use crate::time_machine::{Command,CommandReply,MenuView};
        match cmd {
            Command::Help=>Ok(CommandReply::View(MenuView::Help,Vec::new())),
            Command::Culture(id)=>{
                let map=self.star_map.as_mut().ok_or("culture rendering requires pixel --ff")?;
                if map.set_culture(id)? {self.visual_change_pending=true;}
                Ok(CommandReply::Applied)
            }
            Command::CultureQuery | Command::Cultures=>{
                let map=self.star_map.as_ref().ok_or("culture rendering requires pixel --ff")?;
                let active=map.cultures.selected().to_string();
                let mut ids=map.cultures.available();
                ids.sort_unstable();
                let mut lines=Vec::with_capacity(ids.len()/2+3);
                lines.push(format!("CULTURES / {} / ACTIVE: {}",ids.len(),active.to_ascii_uppercase()));
                let split=(ids.len()+1)/2;
                for i in 0..split {
                    let left=ids[i];
                    let left=if left==active {format!("{left} [ACTIVE]")} else {left.to_string()};
                    let right=ids.get(i+split).map(|id|if *id==active {format!("{id} [ACTIVE]")}else{(*id).to_string()}).unwrap_or_default();
                    lines.push(format!("{left:<48}{right}"));
                }
                lines.push(String::new());
                lines.push("Type an exact ID, e.g. :chinese".into());
                lines.push(":culture off hides lines + names".into());
                Ok(CommandReply::View(MenuView::Cultures,lines))
            }
            Command::DebugStars(value)=>{
                let map=self.star_map.as_mut().ok_or("culture rendering requires pixel --ff")?;
                if map.cultures.set_debugstars(*value) {self.visual_change_pending=true;}
                Ok(CommandReply::Applied)
            }
            Command::DebugStarsQuery=>{
                let map=self.star_map.as_ref().ok_or("culture rendering requires pixel --ff")?;
                Ok(CommandReply::Info(format!("DEBUGSTARS: {}",if map.cultures.debugstars(){"on"}else{"off"})))
            }
            Command::ListPlanets=>{
                let place=world.sky.location().ok_or("planet list requires observer location")?;
                self.planet_ephemeris.refresh(place,world.sky.now_secs());
                let kinds=crate::sky::PlanetKind::ALL;
                let split=(kinds.len()+1)/2;
                let mut cells=Vec::with_capacity(kinds.len());
                for kind in kinds {
                    let cell=if kind==crate::sky::PlanetKind::Earth {
                        format!("{:<9} {:>8}",kind.name().to_ascii_uppercase(),"OBSERVER")
                    } else if let Some(state)=self.planet_ephemeris.state(kind) {
                        format!("{:<9} {:+6.1} {:6.1}",kind.name().to_ascii_uppercase(),state.altitude_deg,state.azimuth_deg)
                    } else {
                        format!("{:<9} {:>14}",kind.name().to_ascii_uppercase(),"NO EPHEMERIS")
                    };
                    cells.push(cell);
                }
                let mut lines=Vec::with_capacity(split+5);
                lines.push(format!("SNAPSHOT UTC {}",crate::time_machine::format_utc(world.sky.now_secs())));
                lines.push(format!("{:<32}{}","NAME        ALT     AZ","NAME        ALT     AZ"));
                for i in 0..split {
                    let right=cells.get(i+split).map(String::as_str).unwrap_or("");
                    lines.push(format!("{:<32}{}",cells[i],right));
                }
                lines.push(String::new());
                lines.push("Type a name, e.g. :venus or :planets venus".into());
                lines.push(":planets graphic / point  Choose planet appearance".into());
                Ok(CommandReply::View(MenuView::Planets,lines))
            }
            Command::Planet(name)=>{
                let kind=crate::sky::PlanetKind::parse(name)
                    .ok_or_else(||format!("unknown planet {name:?}; use :planets"))?;
                if kind==crate::sky::PlanetKind::Earth {
                    return Ok(CommandReply::Info("EARTH IS THE OBSERVER BODY".into()));
                }
                let _=self.star_map.as_ref().ok_or("planet navigation requires pixel --ff")?;
                gimbal.begin_planet(world,kind)?;
                self.visual_change_pending=true;
                Ok(CommandReply::Applied)
            }
            Command::StarsQuery=>Ok(CommandReply::Info(format!(
                "STARS: current {}x · default {}x · allowed >0 to {}x",
                crate::display_settings::format_multiplier(world.sky.star_multiplier()),
                crate::display_settings::format_multiplier(crate::display_settings::DEFAULT_STAR_MULTIPLIER),
                crate::display_settings::MAX_STAR_MULTIPLIER))),
            Command::Cryptids(mode)=>{
                if self.cryptid_mode!=*mode {self.cryptid_mode=*mode;self.visual_change_pending=true;}
                Ok(CommandReply::Applied)
            }
            Command::PlanetAppearance(mode)=>{
                if self.planet_appearance!=*mode {self.planet_appearance=*mode;self.visual_change_pending=true;}
                Ok(CommandReply::Applied)
            }
            Command::Jumbotron(value)=>{
                if self.jumbotron_visible!=*value {self.jumbotron_visible=*value;self.visual_change_pending=true;}
                Ok(CommandReply::Applied)
            }
            Command::Hud(value)=>{
                if self.hud_visible!=*value {self.hud_visible=*value;self.visual_change_pending=true;}
                Ok(CommandReply::Applied)
            }
            Command::Chickens(value)=>{
                if self.background_animals_visible!=*value {
                    self.background_animals_visible=*value;
                    self.visual_change_pending=true;
                }
                Ok(CommandReply::Applied)
            }
            Command::CelestialPlaceholder=>Ok(CommandReply::Info("Celestial lookup not implemented yet".into())),
            Command::Named(name)=>{
                let planet=crate::sky::PlanetKind::parse(name);
                let culture=self.star_map.as_ref().is_some_and(|map|map.cultures.available().iter().any(|id|id.eq_ignore_ascii_case(name)));
                match (planet,culture) {
                    (Some(_),true)=>Err(format!("{name:?} is ambiguous; use :planets {name} or :culture {name}")),
                    (Some(_),false)=>self.visual_command(&Command::Planet(name.clone()),world,gimbal),
                    (None,true)=>self.visual_command(&Command::Culture(name.clone()),world,gimbal),
                    (None,false)=>Err(format!("unknown command or registered name {name:?}; use :help")),
                }
            }
            _=>Err("not a visual command".into()),
        }
    }

    fn cryptids_visible(&self,world:&World)->bool {
        use crate::time_machine::CryptidMode;
        match self.cryptid_mode {
            CryptidMode::Cycle=>true,
            CryptidMode::Night=>world.sky.solar_phase()==crate::circadian::Phase::Night,
            CryptidMode::Off=>false,
        }
    }

    fn projected_planets(&mut self,world:&World)->[Option<crate::sky::PlanetVisual>;8] {
        let mut visuals=[None;8];
        if !world.sky.fast() {return visuals;}
        let camera=match self.star_map.as_ref(){Some(map)=>map.camera(),None=>return visuals};
        let place=match world.sky.location(){Some(place)=>place,None=>return visuals};
        self.planet_ephemeris.refresh(place,world.sky.now_secs());
        for state in self.planet_ephemeris.states() {
            if !state.above_horizon {continue;}
            let Some(slot)=crate::sky::PlanetKind::SKY_ORDER.iter().position(|kind|*kind==state.kind) else{continue;};
            let Some(pixel)=space_zoo::star_projection::project_alt_az_with_camera(
                state.altitude_deg.to_radians(),state.azimuth_deg.to_radians(),WIDTH,HEIGHT,camera) else{continue;};
            let scale=ART_SCALE as i32;
            let center=crate::world::Point{x:(pixel.x+scale/2)/scale,y:(pixel.y+scale/2)/scale};
            let kind=state.kind;
            visuals[slot]=Some(crate::sky::PlanetVisual{kind,center,radius:kind.radius(),
                color:kind.color(),accent:kind.accent(),ring:kind.has_ring()});
        }
        visuals
    }

    /// Choose the optional approved 1902-inspired Moon without changing sky timing.
    pub fn set_melies_moon(&mut self, on:bool) {
        self.moon.set_melies(on);
        self.last_orb=None;
        self.last_concurrent_moon=None;
    }

    // Telemetry arrives only once per 2 seconds. Never read /proc on a frame.
    pub fn update_billboard(&mut self, snapshot: Snapshot) {
        if let Some(b) = &mut self.billboard { b.update(snapshot); }
        else { self.billboard = Some(Billboard::new(snapshot)); }
        self.billboard_updated=true;
    }

    /// Nonblocking transient scene feedback. The deadline is wall-clock based so
    /// simulation pause/rate changes cannot stretch or suppress the message.
    pub fn show_hud_message(&mut self,text:&str,duration:std::time::Duration) {
        self.hud_message=Some((text.to_string(),std::time::Instant::now()+duration));
        self.hud_message_changed=true;
    }
    fn expire_hud_message(&mut self) {
        if self.hud_message.as_ref().is_some_and(|(_,deadline)|std::time::Instant::now()>=*deadline) {
            self.hud_message=None;
            self.hud_message_changed=true;
        }
    }
    fn draw_hud_message(&mut self) {
        let Some((text,_))=&self.hud_message else{return;};
        let text=text.clone();
        let (x,y,w,h)=GIMBAL_MESSAGE_DAMAGE_RECT;
        self.canvas.native_rect(x,y,w,h,0x08121b);
        self.canvas.native_rect(x,y,w,1,0x83a9b7);
        self.canvas.native_rect(x,y+h-1,w,1,0x334d5c);
        let text_w=text.chars().count() as i32*6;
        crate::billboard::overlay_text(&mut self.canvas,x+(w-text_w)/2,y+6,&text,1,0xeef6f4);
    }

    /// Set only in --ff. Normal renderers never construct a Time Machine panel.
    pub fn set_time_panel(&mut self,panel:&crate::time_machine::Panel) {
        self.time_panel=Some(panel.clone());
    }
    fn draw_time_panel(&mut self,world:&World) {
        let Some(panel)=&self.time_panel else{return;};
        let (x,y,_,_)=TIME_PANEL_RECT;
        paint_command_center(&mut self.canvas,panel,world.sky.star_multiplier(),(world.tick/8)%2==0,x,y);
    }
    // An independent simulated-clock display: the existing billboard remains
    // completely dedicated to LIVE /proc telemetry, even at 3600x or paused.
    fn draw_simulation_hud(&mut self) {
        let Some(hud)=&self.simulation_hud else {return;};
        let title=format!("SPACE TIME MACHINE | {} {}",hud.direction,hud.speed);
        let utc=format!("UTC {}",hud.utc);
        let mut coordinates=hud.location.split(',');
        let lat=coordinates.next().unwrap_or("").trim();
        let lon=coordinates.next().unwrap_or("").trim();
        let location=format!("LAT {}  LON {}",lat,lon);
        // Five seven-pixel text rows at 12px spacing, with the two new
        // control reminders beneath the existing three live status rows.
        // Visible width remains driven by content, not the damage bounds.
        let longest=title.chars().count().max(utc.chars().count())
            .max(location.chars().count())
            .max(FF_CONTROL_HINTS[0].chars().count())
            .max(FF_CONTROL_HINTS[1].chars().count());
        let w=(longest as i32*6+14).min(FF_STATUS_DAMAGE_RECT.2);
        let h=FF_STATUS_VISIBLE_HEIGHT;
        let x=WIDTH as i32-w;
        let y=0;
        let c=&mut self.canvas;
        c.native_rect(x,y,w,h,0x08121b);
        c.native_rect(x,y,w,1,0x83a9b7);
        c.native_rect(x,y+h-1,w,1,0x334d5c);
        c.native_rect(x,y,1,h,0x526f7c);
        c.native_rect(x+w-1,y,1,h,0x334d5c);
        crate::billboard::overlay_text(c,x+7,y+4,&title,1,0x9ebfcd);
        crate::billboard::overlay_text(c,x+7,y+16,&utc,1,0xeef6f4);
        crate::billboard::overlay_text(c,x+7,y+28,&location,1,0xeef6f4);
        crate::billboard::overlay_text(c,x+7,y+40,FF_CONTROL_HINTS[0],1,0xc5d9e1);
        crate::billboard::overlay_text(c,x+7,y+52,FF_CONTROL_HINTS[1],1,0xc5d9e1);
    }
    fn refresh_admin1(&mut self,world:&World) {
        if !world.sky.fast() {
            self.admin1_last_location=None;
            if self.admin1_display.take().is_some() || self.admin1_region_id.take().is_some() {
                self.admin1_changed=true;
            }
            return;
        }
        let Some(place)=world.sky.location() else {
            self.admin1_last_location=None;
            if self.admin1_display.take().is_some() || self.admin1_region_id.take().is_some() {
                self.admin1_changed=true;
            }
            return;
        };
        let location_key=(place.latitude.to_bits(),place.longitude.to_bits());
        if self.admin1_last_location==Some(location_key) {return;}
        self.admin1_last_location=Some(location_key);
        let Some(lookup)=self.admin1_lookup.as_mut() else {return;};
        let result=lookup.resolve(place.latitude,place.longitude);
        let mut disable_lookup=false;
        match result {
            Ok(Some(found))=>{
                let region_id=lookup.cached_region_id();
                if self.admin1_region_id!=region_id || self.admin1_display.is_none() {
                    let (label,draw_wireframe)=admin1_overlay_label(&found);
                    let pixels=if draw_wireframe {
                        lookup.wireframe_pixels(ADMIN1_WIREFRAME_W,ADMIN1_WIREFRAME_H)
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    self.admin1_display=Some(Admin1Ui{label,pixels});
                    self.admin1_region_id=region_id;
                    self.admin1_changed=true;
                }
            }
            Ok(None)=>{
                if self.admin1_display.take().is_some() || self.admin1_region_id.take().is_some() {
                    self.admin1_changed=true;
                }
            }
            Err(error)=>{
                crate::logging::error("ADMIN1_ERROR","admin1.lookup",&error.to_string());
                if self.admin1_display.take().is_some() || self.admin1_region_id.take().is_some() {
                    self.admin1_changed=true;
                }
                disable_lookup=true;
            }
        }
        if disable_lookup {self.admin1_lookup=None;}
    }
    fn draw_admin1_overlay(&mut self) {
        let Some(view)=&self.admin1_display else{return;};
        let label_width=view.label.chars().count() as i32*6+14;
        let w=label_width.max(116).min(ADMIN1_DAMAGE_RECT.2);
        let x=WIDTH as i32-w;
        let y=FF_STATUS_VISIBLE_HEIGHT+4;
        let c=&mut self.canvas;
        // Transparent ADMIN-1 overlay: only text and the region outline paint.
        // The star field/scenery beneath the fixed damage area remain visible.
        crate::billboard::overlay_text(c,x+7,y+4,&view.label,1,0xc5d9e1);
        let ox=x+(w-ADMIN1_WIREFRAME_W as i32)/2;
        let oy=y+18;
        for &(px,py) in &view.pixels {
            // A 2x2 native stroke is deliberately heavier than the original
            // single-pixel wireframe while retaining a fully transparent interior.
            c.native_rect(ox+px as i32,oy+py as i32,2,2,0x71858b);
        }
    }
    fn refresh_coop(&mut self, world:&World) {
        let coop=world.habitat.coop;
        let day=world.clock.is_day();
        if self.cached_coop==Some((coop,day)) {return;}
        self.coop_art=Canvas::scaled(coop.width as usize,coop.height as usize,ART_SCALE,false);
        self.coop_art.clear(0);
        let mut local=coop;
        local.door.x-=coop.x;
        local.door.y-=coop.y;
        local.x=0;
        local.y=0;
        pixel_scenery::paint_coop(&mut self.coop_art,local,day);
        self.cached_coop=Some((coop,day));
    }
    fn composite_coop(&mut self, world:&World) {
        let coop=world.habitat.coop;
        let src_width=self.coop_art.w*ART_SCALE;
        let ox=coop.x*ART_SCALE as i32;
        let oy=coop.y*ART_SCALE as i32;
        if !self.canvas.overlaps(ox,oy,src_width as i32,(self.coop_art.h*ART_SCALE) as i32){return;}
        let stride=self.canvas.w*ART_SCALE;
        for row in 0..self.coop_art.h*ART_SCALE {
            let y=oy+row as i32;
            if y<0 || y>=HEIGHT as i32 {continue;}
            for col in 0..src_width {
                let x=ox+col as i32;
                if x<0 || x>=WIDTH as i32 {continue;}
                let pixel=self.coop_art.cells[row*src_width+col];
                if pixel!=0 && self.canvas.touched(x as usize,y as usize) {
                    self.canvas.cells[y as usize*stride+x as usize]=pixel;
                }
            }
        }
    }
    fn actor_damage(&mut self,world:&World) {
        self.current_actors.clear();
        for a in &world.animals {
            if let Some(r)=crate::living_animals::animal_bounds(a,world.tick) {
                self.current_actors.push(Rect::new(r.0,r.1,r.2,r.3));
            }
        }
        for d in &world.deer.residents {
            if d.visible {
                let p=d.top_left();let (w,h)=d.size();
                self.current_actors.push(Rect::logical(p.x,p.y,w,h));
            }
        }
        for m in &world.mythic.visitors {
            if m.visible() {
                let p=m.top_left();let (w,h)=m.size();
                self.current_actors.push(Rect::logical(p.x,p.y,w,h));
            }
        }
        for b in &world.bats.pair {
            if b.visible {self.current_actors.push(Rect::new((b.pos.x-7)*3,(b.pos.y-4)*3,42,24));}
        }
        for b in world.birds.visual(world.tick,world.width) {
            let (w,h)=match b.kind {
                crate::birds::BirdKind::BlueHeron => (120,54),
                _ => (48,30)
            };
            self.current_actors.push(Rect::new(b.pos.x*3,b.pos.y*3,w,h));
        }
        if let Some(v)=world.events.visual(world.tick,world.width) {
            if world.events.visible_kind().is_some() {
                self.current_actors.push(Rect::logical(v.pos.x,v.pos.y,26,10));
            }
        }
    }
    fn mark_animation(&mut self,world:&World,reset:bool,planets:&[Option<crate::sky::PlanetVisual>;8]) {
        self.canvas.enable_damage();
        if reset {self.canvas.mark_all();}
        if self.time_hud_changed {
            let (x,y,w,h)=FF_STATUS_DAMAGE_RECT;
            self.canvas.mark(Rect::new(x,y,w,h));
            self.time_hud_changed=false;
        }
        if self.admin1_changed {
            let (x,y,w,h)=ADMIN1_DAMAGE_RECT;
            self.canvas.mark(Rect::new(x,y,w,h));
            self.admin1_changed=false;
        }
        if self.hud_message_changed {
            let (x,y,w,h)=GIMBAL_MESSAGE_DAMAGE_RECT;
            self.canvas.mark(Rect::new(x,y,w,h));
            self.hud_message_changed=false;
        }
        if self.time_panel!=self.previous_time_panel {
            if self.time_panel.as_ref().is_some_and(|p|p.open) ||
                self.previous_time_panel.as_ref().is_some_and(|p|p.open) {
                let (x,y,w,h)=TIME_PANEL_RECT;
                self.canvas.mark(Rect::new(x,y,w,h));
            }
            self.previous_time_panel=self.time_panel.clone();
        }
        let command_cursor_on=self.time_panel.as_ref()
            .filter(|p|p.open && p.editing && !p.pending)
            .map(|_|(world.tick/8)%2==0);
        if command_cursor_on!=self.last_command_cursor_on {
            if command_cursor_on.is_some() || self.last_command_cursor_on.is_some() {
                let (x,y,w,_)=TIME_PANEL_RECT;
                self.canvas.mark(Rect::new(x+12,y+TIME_PANEL_INPUT_BOX_Y,w-24,11));
            }
            self.last_command_cursor_on=command_cursor_on;
        }
        // On ordinary frames the sky gradient and static hills aren't touched.
        // Normal mode keeps the original 3x3 decorative stars. In --ff, only
        // old/new native catalogue pixels are invalidated when a bounded
        // reprojection actually replaces the star map.
        if world.sky.fast() {
            if self.star_map_changed {
                if let Some(map)=&self.star_map {
                    for p in map.previous_pixels().iter().chain(map.pixels()) {
                        self.canvas.mark(Rect::new(p.pixel.x,p.pixel.y,1,1));
                    }
                    for p in map.previous_horizon_pixels().iter().chain(map.horizon_pixels()) {
                        self.canvas.mark(Rect::new(p.x,p.y,1,1));
                    }
                    for points in [map.previous_horizon_pixels(),map.horizon_pixels()] {
                        if let Some(rect)=horizon_label_rect(points) {self.canvas.mark(rect);}
                    }
                    for picture in [&map.cultures.previous,&map.cultures.current] {
                        for line in &picture.lines {
                            self.canvas.mark_line(line);
                        }
                        for name in &picture.names {
                            self.canvas.mark(Rect::new(name.x,name.y,(name.text.chars().count()*6) as i32,7));
                        }
                    }
                    // Labels and crosshairs are deterministic derivatives of these same
                    // points. Mark old/new footprints plus fixed UI to prevent trails,
                    // including marker arms that may cross a 16x16 damage-tile edge.
                    for p in map.previous_diagnostics().iter().chain(map.diagnostics()).filter(|_|map.cultures.debugstars()) {
                        self.canvas.mark(diagnostic_label_rect(p));
                        self.canvas.mark(diagnostic_crosshair_rect(p));
                    }
                    let (x,y,w,h)=STAR_DIAGNOSTIC_PANEL_RECT;
                    self.canvas.mark(Rect::new(x,y,w,h));
                    let (x,y,w,h)=STAR_RETICLE_RECT;
                    self.canvas.mark(Rect::new(x,y,w,h));
                }
            }
        } else {
            for p in self.last_stars.iter().chain(world.background.stars()) {
                self.canvas.mark(Rect::logical(p.x,p.y,1,1));
            }
            self.last_stars.clear();
            self.last_stars.extend_from_slice(world.background.stars());
        }
        let daylight=world.sky.solar_daylight(world.clock.seconds_today());
        if legacy_constellation_layer_enabled(world.sky.fast()) {
            // Preserve the existing constellation overlay's four-second projection key.
            // Between those projections only its native ~5px star halos twinkle.
            let unix=world.clock.observing_unix(world.sky.now_secs());
            let constellation_key=unix/4;
            let active=daylight<0.985;
            // Only redraw the full constellation layer when star projection or
            // solar opacity actually changes, not on every twilight render frame.
            if active {
                if self.last_constellation_key!=Some(constellation_key) ||
                        self.last_constellation_daylight!=Some(daylight) {
                    self.canvas.mark_all();
                } else {
                    for (x,y) in self.constellations.star_centers() {
                        self.canvas.mark(Rect::new(x-3,y-3,7,7));
                    }
                }
            }
            self.last_constellation_key=Some(constellation_key);
            self.last_constellation_daylight=Some(daylight);
        } else {
            // v0.51.01: --ff is a clean catalogue diagnostic scene. Keep both the
            // legacy constellation painter and its damage/invalidation path dormant.
            self.last_constellation_key=None;
            self.last_constellation_daylight=None;
        }
        let jewel_day=daylight>0.45;
        if self.last_arc_jewel_day.is_some_and(|last|last!=jewel_day) {
            for n in 1..8 {
                let p=crate::sky::position_on_sun_arc(n as f32/8.0,
                    world.width,world.height);
                self.canvas.mark(Rect::logical(p.x,p.y,2,1));
            }
        }
        self.last_arc_jewel_day=Some(jewel_day);
        // Coronas still change only once per eight simulation ticks; avoid
        // touching their 280px area on intervening 15 FPS refreshes.
        let [sun,moon]=world.sky.celestial_pair(world.clock.seconds_today(),world.width,world.height);
        let orb=sun.or(moon);
        let phase=crate::lunar::phase_key(world.sky.moon_phase()) as u64;
        let next_orb=orb.map(|o| (o.kind,o.center.x,o.center.y,
            if o.kind==crate::sky::Orb::Sun {world.tick/8} else {phase}));
        if next_orb!=self.last_orb {
            for o in [self.last_orb,next_orb].into_iter().flatten() {
                let radius=if o.0==crate::sky::Orb::Sun {
                    crate::solar::OCCLUSION_RADIUS
                } else {crate::lunar::OCCLUSION_RADIUS};
                self.canvas.mark(Rect::logical(o.1-radius,o.2-radius,
                    radius*2+1,radius*2+1));
            }
        }
        self.last_orb=next_orb;
        // Previously only the first celestial body was damage-tracked. When
        // both are visible the Sun retains its existing cadence, while the
        // Moon independently invalidates old/new footprints and phase frames.
        let concurrent=if sun.is_some() {moon} else {None};
        let next_concurrent=concurrent.map(|o|(o.kind,o.center.x,o.center.y,phase));
        if next_concurrent!=self.last_concurrent_moon {
            for o in [self.last_concurrent_moon,next_concurrent].into_iter().flatten() {
                let radius=crate::lunar::OCCLUSION_RADIUS;
                self.canvas.mark(Rect::logical(o.1-radius,o.2-radius,
                    radius*2+1,radius*2+1));
            }
        }
        self.last_concurrent_moon=next_concurrent;
        // Independently repaint ONLY the 32x32 gear at the watch tourbillon's
        // five-tick cadence. The corona cache remains on its eight-tick clock.
        if let Some(sun)=orb.filter(|o|o.kind==crate::sky::Orb::Sun) {
            let wheel_step=world.tick/crate::solar::WHEEL_TICKS_PER_FRAME;
            if self.last_sun_wheel_step!=Some(wheel_step) {
                let (x,y,w,h)=crate::solar::wheel_damage_rect(sun.center.x,sun.center.y);
                self.canvas.mark(Rect::new(x,y,w,h));
            }
            self.last_sun_wheel_step=Some(wheel_step);
            // The face mane changes on its own 42-tick cycle, independently
            // of both corona (8) and tourbillon (5) cadence. Previous code
            // missed transitions such as tick 41 -> 42 and retained 555
            // pixels of the old pose. Invalidate only the 73x20 changed area.
            let mane_step=world.tick/crate::solar::MANE_TICKS_PER_FRAME;
            if self.last_sun_mane_step!=Some(mane_step) {
                let (x,y,w,h)=crate::solar::mane_damage_rect(sun.center.x,sun.center.y);
                self.canvas.mark(Rect::new(x,y,w,h));
            }
            self.last_sun_mane_step=Some(mane_step);
        } else {
            self.last_sun_wheel_step=None;
            self.last_sun_mane_step=None;
        }
        // Every projected planet owns an independent damage key. A body moving,
        // animating, entering, or leaving the camera invalidates only its old/new
        // native footprint; there is no current-planet swap or visitor lifecycle.
        let mut next_planets=[None;8];
        for (slot,planet) in planets.iter().enumerate() {
            next_planets[slot]=(*planet).map(|p|(p.kind,p.center.x,p.center.y,
                if self.planet_appearance==crate::time_machine::PlanetAppearance::Graphic {
                    planet_sprite::frame_key_for(p.kind,world.tick)
                } else {0}));
            if next_planets[slot]!=self.last_planets[slot] {
                for p in [self.last_planets[slot],next_planets[slot]].into_iter().flatten() {
                    if self.planet_appearance==crate::time_machine::PlanetAppearance::Graphic {
                        let (x,y,w,h)=planet_sprite::native_bounds(p.0,p.1,p.2);
                        self.canvas.mark(Rect::new(x,y,w,h));
                    } else {
                        self.canvas.mark(Rect::logical(p.1-2,p.2-2,5,5));
                    }
                    let (x,y,w,h)=pixel_scenery::planet_label_bounds(
                        p.0,p.1,p.2,self.planet_appearance);
                    self.canvas.mark(Rect::new(x,y,w,h));
                }
            }
        }
        self.last_planets=next_planets;
        // Expiry has its own tiny damage path: clock, cabinet, process rows,
        // and MEM/DISK/NET retain their last pixels during a worker stall.
        let (billboard_x,billboard_y)=if legacy_zoo_scene_enabled(world.sky.fast()) {
            (crate::billboard::X,crate::billboard::Y)
        } else {
            (crate::billboard::NULL_ISLAND_X,crate::billboard::NULL_ISLAND_Y)
        };
        if let Some(board)=&mut self.billboard {
            if board.expire_cpu_if_stale(std::time::Instant::now()) {
                for &(x,y,w,h) in &crate::billboard::DIE_CORE_RECTS {
                    self.canvas.mark(Rect::new(billboard_x*ART_SCALE as i32+x as i32,
                        billboard_y*ART_SCALE as i32+y as i32,w as i32,h as i32));
                }
            }
        }
        // The existing 0.451 billboard cache emits only clock / sign /proc
        // changes. No cabinet repaint just because an animal took a step.
        if self.billboard.is_some() {
            let gyro=world.tick/5;
            let lighting=!world.clock.is_day() && (2..=5).contains(&((world.tick/12)%8));
            let (_,date)=world.clock.instrument_time(world.sky.fast());
            if self.billboard_updated {
                self.canvas.mark(Rect::logical(billboard_x,billboard_y,
                    crate::billboard::W as i32,crate::billboard::H as i32));
            } else {
                if self.last_billboard_gyro!=Some(gyro) || self.last_billboard_date!=Some(date) {
                    self.canvas.mark(Rect::new(billboard_x*3+85,
                        billboard_y*3+112,102,102));
                }
                if self.last_billboard_lighting!=Some(lighting) {
                    self.canvas.mark(Rect::new(billboard_x*3+259,
                        billboard_y*3+108,52,109));
                }
            }
            self.last_billboard_gyro=Some(gyro);
            self.last_billboard_lighting=Some(lighting);
            self.last_billboard_date=Some(date);
        }
        self.billboard_updated=false;
        // The default scene uses only the generalized foreground actor engine.
        if !legacy_zoo_scene_enabled(world.sky.fast()) {
            if self.cryptids_visible(world) {
                let next_bipedal=crate::bipedal::visual_key(world.tick);
                if self.last_bipedal_key!=Some(next_bipedal) {
                    for rect in crate::bipedal::damage_delta(self.last_bipedal_key,next_bipedal)
                        .into_iter().flatten()
                    {
                        self.canvas.mark(Rect::new(rect.0,rect.1,rect.2,rect.3));
                    }
                    self.last_bipedal_key=Some(next_bipedal);
                }
            } else if let Some(old)=self.last_bipedal_key.take() {
                if let Some(rect)=crate::bipedal::damage_bounds(old) {
                    self.canvas.mark(Rect::new(rect.0,rect.1,rect.2,rect.3));
                }
            }
            self.current_actors.clear();
            if self.background_animals_visible {
                crate::living_animals::for_each_null_island_background_bound(
                    world.clock.phase(),world.tick,|x,y,w,h| {
                        self.current_actors.push(Rect::new(x,y,w,h));
                    });
            }
        } else {
            self.last_bipedal_key=None;
            self.actor_damage(world);
        }
        for r in self.last_actors.iter().chain(self.current_actors.iter()) {
            self.canvas.mark(*r);
        }
        // If any dynamic damage touched a diagnostic dot, invalidate its adjacent
        // label and marker too. This makes tag suppression/return follow the actual
        // rendered dot without changing the underlying projected coordinate.
        if world.sky.fast() {
            if let Some(map)=&self.star_map {
                for point in map.diagnostics().iter().filter(|_|map.cultures.debugstars()) {
                    if self.canvas.touched(point.pixel.x as usize,point.pixel.y as usize) {
                        self.canvas.mark(diagnostic_label_rect(point));
                        self.canvas.mark(diagnostic_crosshair_rect(point));
                    }
                }
            }
        }
        std::mem::swap(&mut self.last_actors,&mut self.current_actors);
    }
    // Both framebuffer and desktop window receive the SAME 960x600 pixels.
    // No 320->960 enlarge stage and no extra desktop copy.
    pub fn draw_native(&mut self, world: &World) -> &[u32] {
        self.expire_hud_message();
        if world.sky.fast() {
            // Use unavailable (never fabricated zeroes) only until the first
            // real 2-second telemetry sample arrives from the worker.
            if self.billboard.is_none() {
                self.billboard=Some(crate::billboard::Billboard::unavailable());
                self.billboard_updated=true;
            }
            let hud=crate::billboard::TimeHud::from_sky(&world.sky);
            if self.simulation_hud.as_ref()!=Some(&hud) {
                self.simulation_hud=Some(hud);
                self.time_hud_changed=true;
            }
        } else if self.simulation_hud.take().is_some() {
            self.time_hud_changed=true;
        }
        self.refresh_admin1(world);
        let sky_light=world.sky.fast().then(||world.sky.sky_light_state());
        let sky_light_changed=if let Some(light)=sky_light {
            // Damage follows what can actually change on screen: star dots cross
            // one hard day/civil boundary, while constellation opacity is an
            // 8-bit blend. Continuous limiting-magnitude membership is handled
            // by StarMap's bounded astronomy refresh below.
            let key=(light.visible_limiting_magnitude.is_some(),
                (light.constellation_visibility*255.0).round().clamp(0.0,255.0) as u8);
            let changed=self.last_sky_light_key!=Some(key);
            self.last_sky_light_key=Some(key);
            changed
        } else {
            self.last_sky_light_key.take().is_some()
        };
        let moved=if world.sky.fast() {
            self.star_map.as_mut().is_some_and(|map|map.update(world))
        } else {false};
        self.star_map_changed=moved || sky_light_changed || self.visual_change_pending;
        let dimensions=(world.width, world.height);
        let daylight=world.sky.solar_daylight(world.clock.seconds_today());
        // In --ff, an hour can pass in one real second. Rebuilding all scenery
        // on every simulated minute would redraw the entire stable day/night
        // sky needlessly. The cached backdrop depends on daylight strength,
        // not the minute number. Twilight changes still repaint accurately.
        let reset=if world.sky.fast() {
            self.cached_dimensions!=dimensions || self.cached_fast_daylight!=Some(daylight)
        } else {
            self.cached_sky_minute!=Some(world.sky.now_secs()/60) ||
            self.cached_hour!=Some(world.clock.hour()) ||
            self.cached_minute!=Some(world.clock.minute()) ||
            self.cached_dimensions!=dimensions
        };
        if reset {
            pixel_scenery::draw_static(&mut self.scenery, world);
            self.cached_fast_daylight=Some(daylight);
            self.cached_sky_minute=Some(world.sky.now_secs()/60);
            self.cached_hour=Some(world.clock.hour());
            self.cached_minute=Some(world.clock.minute());
            self.cached_dimensions=dimensions;
        }
        let planets=self.projected_planets(world);
        self.mark_animation(world,reset || self.visual_change_pending,&planets);
        self.visual_change_pending=false;
        self.canvas.restore_marked(&self.scenery);
        // The instrument is always wall-clock-local in --ff; the sky is not.
        let (instrument_seconds,instrument_date)=world.clock.instrument_time(world.sky.fast());
        if let Some(board)=&mut self.billboard {board.set_date(instrument_date);}
        // Original draw order: twinkling stars underneath sun, trees, hills,
        // buildings. The occlusion mask preserves that exactly, including the
        // moon's dark cutout, without repainting scenery every frame.
        if world.sky.fast() {
            let light=sky_light.expect("fast mode has sky-light state");
            if let Some(map)=&self.star_map {
                draw_culture_layer(&mut self.canvas,&self.scenery,&map.cultures.current,
                    light.constellation_visibility);
                if light.visible_limiting_magnitude.is_some() {
                    draw_catalogue_star_layer(&mut self.canvas,&self.scenery,map.pixels());
                }
            }
        } else {
            let mask=self.scenery.occluded.as_ref().expect("scenery has occlusion mask");
            for star in world.background.stars() {
                if star.x>=0 && star.y>=0 && star.x < LOGICAL_W as i32 && star.y < LOGICAL_H as i32 {
                    let at=star.y as usize*LOGICAL_W + star.x as usize;
                    if !mask[at] { self.canvas.put(star.x,star.y,pixel_scenery::star_color(world)); }
                }
            }
        }
        // Native subpixels, clipped against the EXISTING static scenery mask.
        // Compose behind the moon/sun/planets, not on top of celestial bodies.
        // v0.51.01 deliberately strips this legacy layer from --ff only.
        if legacy_constellation_layer_enabled(world.sky.fast()) {
            self.constellations.draw(&mut self.canvas,&self.scenery,
                world.clock.observing_unix(world.sky.now_secs()),
                world.sky.solar_daylight(world.clock.seconds_today()),
                world.tick);
        }
        pixel_scenery::draw_dynamic_sky(&mut self.canvas,world,&mut self.moon,&mut self.sun,
            &planets,&mut self.planet_sprites,self.planet_appearance);
        pixel_scenery::occlude_sky(&mut self.canvas,&self.scenery,world,&planets,self.planet_appearance);

        // The default fast scene keeps the accepted Null Island waterfront and
        // renders only the generalized foreground cryptid actor layer. The retained
        // --og painter is untouched.
        if !legacy_zoo_scene_enabled(world.sky.fast()) {
            crate::null_island::draw_island(&mut self.canvas);
            // v0.55.42: small retained zoo sprites are an ambient background
            // layer on Null Island. Foreground cryptids intentionally paint later.
            if self.background_animals_visible {
                crate::living_animals::draw_null_island_background(
                    &mut self.canvas,world.clock.phase(),world.tick);
            }
            if self.cryptids_visible(world) {
                crate::bipedal::draw(&mut self.canvas,world.tick);
                crate::bipedal::draw_label(&mut self.canvas,world.tick);
            }
            if let Some(map)=&self.star_map {
                // Keep the true projected astronomical horizon as the final sky
                // annotation rather than treating decorative mountain ridges as terrain.
                draw_horizon_layer(&mut self.canvas,&self.scenery,map.horizon_pixels());
            }
            if self.jumbotron_visible {
                if let Some(board)=&mut self.billboard {
                    board.draw_supports_at(&mut self.canvas,world.clock.is_day(),
                        world.tick,instrument_seconds,
                        crate::billboard::NULL_ISLAND_X,crate::billboard::NULL_ISLAND_Y);
                }
            }
        } else {
            self.refresh_coop(world);
        // Painter's order uses feet-depth rather than spawn order: adults
        // occupy backfield, hens mid-yard, dog/cat the foreground. Reuse the
        // sorting allocation to keep Bertha's framebuffer path lightweight.
        self.depth_order.clear();
        let existing=world.animals.len();
        self.depth_order.extend(0..existing);
        self.depth_order.extend((0..4).filter(|&i|world.deer.residents[i].visible)
            .map(|i|existing+i));
        self.depth_order.extend((0..2).filter(|&i|world.mythic.visitors[i].visible())
            .map(|i|existing+4+i));
        self.depth_order.sort_unstable_by_key(|&i| {
            if i<existing {
                let a=&world.animals[i];
                staging::depth(a.species,a.visual_pos(world.tick))
            } else if i<existing+4 {
                world.deer.residents[i-existing].feet.y
            } else {world.mythic.visitors[i-existing-4].feet.y}
        });
        // Two physical structures join the SAME feet-sorted painter pass.
        // Moving animals with feet behind them are obscured by their walls/legs;
        // animals in front paint over them. The instrument faces remain on top.
        let support_depth=Billboard::stand_feet(world.height as i32)[0].y;
        let coop_depth=world.habitat.coop.door.y;
        let mut supports_done=false;
        let mut coop_done=false;
        // Copy indices into a compact reusable fixed-size buffer so we can
        // mutably composite both caches without borrowing the sort vector.
        for n in 0..self.depth_order.len() {
            let i=self.depth_order[n];
            let depth=if i<existing {
                let a=&world.animals[i];
                staging::depth(a.species,a.visual_pos(world.tick))
            } else if i<existing+4 {world.deer.residents[i-existing].feet.y}
                else {world.mythic.visitors[i-existing-4].feet.y};
            if !supports_done && depth>=support_depth {
                if let Some(board)=&mut self.billboard {
                    board.draw_supports(&mut self.canvas,world.clock.is_day(),
                        world.tick,instrument_seconds);
                }
                supports_done=true;
            }
            if !coop_done && depth>=coop_depth {
                self.composite_coop(world);
                coop_done=true;
            }
            if i<existing {
                let animal=&world.animals[i];
                if !animal.is_perching() {
                    pixel_sprite::draw_animal(&mut self.canvas,animal,world.tick);
                }
            } else if i<existing+4 {
                world.deer.draw_pixel(&mut self.canvas,i-existing,world.tick);
            } else {
                world.mythic.draw_pixel(&mut self.canvas,i-existing-4,world.tick);
            }
        }
        if !supports_done {
            if let Some(board)=&mut self.billboard {
                board.draw_supports(&mut self.canvas,world.clock.is_day(),
                    world.tick,instrument_seconds);
            }
        }
        if !coop_done {self.composite_coop(world);}
        // Flying/perched guineas must paint above the roof after the normal
        // feet-sorted ground pass, as the invited birds already do.
        for animal in &world.animals {
            if animal.is_perching() {
                pixel_sprite::draw_animal(&mut self.canvas,animal,world.tick);
            }
        }
        world.bats.draw_pixel(&mut self.canvas,world.tick);
        for bird in world.birds.visual(world.tick,world.width) {
            pixel_birds::draw(&mut self.canvas,&bird);
        }
        if let Some(visitor)=world.events.visual(world.tick,world.width) {
            if let Some(kind)=world.events.visible_kind() {
                pixel_sprite::draw_visitor(&mut self.canvas,kind,visitor.pos,world.tick);
            }
        }
        } // end retained --og zoo painter
        if world.sky.fast() {
            if let Some(map)=&self.star_map {
                // Camera state is permanent UI; :debugstars adds tracked-star labels/crosshairs.
                draw_star_diagnostic_overlay(&mut self.canvas,&self.scenery,map,
                    &mut self.diagnostic_label_rects,self.hud_visible);
            }
        }
        if self.hud_visible {
            self.draw_simulation_hud();
            self.draw_admin1_overlay();
        }
        self.draw_hud_message();
        self.draw_time_panel(world);
        // The live /proc monitor is the absolute foreground UI. Nothing in the
        // Time Machine or diagnostic layers may cover its display faces.
        if self.jumbotron_visible {
            if let Some(board)=&mut self.billboard {
                if legacy_zoo_scene_enabled(world.sky.fast()) {
                    board.draw_faces(&mut self.canvas,world.clock.is_day(),
                        world.tick,instrument_seconds);
                } else {
                    board.draw_faces_at(&mut self.canvas,world.clock.is_day(),
                        world.tick,instrument_seconds,
                        crate::billboard::NULL_ISLAND_X,crate::billboard::NULL_ISLAND_Y);
                }
            }
        }
        &self.canvas.cells
    }

    // Physical Linux VT only: the open Esc command screen is the ultimate
    // foreground layer. X11/window and terminal-emulator paths retain the
    // previously accepted compositing order without alteration.
    pub fn draw_console_with_damage(&mut self,world:&World)->(&[u32],&[bool]) {
        self.draw_native(world);
        if self.time_panel.as_ref().is_some_and(|panel|panel.open) {
            // Repaint only damage tiles, preserving the incremental renderer.
            // Existing animation invalidation already marks the whole panel on
            // opening/closing and the cursor region while editing.
            self.draw_time_panel(world);
        }
        (&self.canvas.cells,self.canvas.damage.as_deref().unwrap())
    }

    // Expose pixels and corresponding native 16x16 damage in one borrow.
    // Only the console backend consumes this incremental-output hint.
    pub fn draw_with_damage(&mut self,world:&World)->(&[u32],&[bool]) {
        self.draw_native(world);
        (&self.canvas.cells,self.canvas.damage.as_deref().unwrap())
    }
    pub fn draw(&mut self, world: &World) -> &[u32] { self.draw_native(world) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{DisplayMode,World};
    use crate::telemetry::Row;

    #[test]
    fn gimbal_cancelled_message_is_nonblocking_and_approximately_two_seconds() {
        let mut painter=PixelRenderer::new();
        let before=std::time::Instant::now();
        painter.show_hud_message("GIMBAL CANCELLED",std::time::Duration::from_secs(2));
        let (text,deadline)=painter.hud_message.as_ref().expect("message installed");
        assert_eq!(text,"GIMBAL CANCELLED");
        let remaining=deadline.saturating_duration_since(before);
        assert!(remaining>=std::time::Duration::from_millis(1900));
        assert!(remaining<=std::time::Duration::from_millis(2100));
        assert!(painter.hud_message_changed);
    }

    #[test]
    fn water_admin1_rows_render_name_only_without_wireframe() {
        let water=crate::admin1_geo::Admin1Match {
            admin1:"PACIFIC OCEAN".into(),
            country:"__WATER__".into(),
        };
        let land=crate::admin1_geo::Admin1Match {
            admin1:"Colorado".into(),
            country:"United States".into(),
        };
        assert_eq!(admin1_overlay_label(&water),("PACIFIC OCEAN".into(),false));
        assert_eq!(admin1_overlay_label(&land),("Colorado, United States".into(),true));
    }

    #[test]
    fn horizon_designator_is_centered_below_the_projected_line() {
        let points=[
            space_zoo::star_projection::StarPixel{x:470,y:320},
            space_zoo::star_projection::StarPixel{x:479,y:318},
            space_zoo::star_projection::StarPixel{x:492,y:315},
        ];
        let rect=horizon_label_rect(&points).expect("visible horizon gets a label");
        assert_eq!(rect.x,WIDTH as i32/2-(HORIZON_LABEL.chars().count() as i32*6)/2);
        assert_eq!(rect.y,323);
        assert_eq!(rect.w,HORIZON_LABEL.chars().count() as i32*6);
        assert_eq!(rect.h,7);
    }

    #[test]
    fn expanded_time_machine_panel_fits_native_framebuffer_without_overlap() {
        let (_,y,w,h)=TIME_PANEL_RECT;
        assert_eq!((w,h),(640,480));
        assert!(TIME_PANEL_CONTENT_TOP + crate::time_machine::MENU_VISIBLE_ROWS as i32*TIME_PANEL_LINE_HEIGHT < TIME_PANEL_FOOTER_Y);
        assert!(TIME_PANEL_INPUT_TEXT_Y+7 < h);
        assert!(y+h <= HEIGHT as i32);
    }
    #[test]
    fn command_center_small_viewport_reduces_visible_rows() {
        assert_eq!(command_center_visible_rows(COMMAND_CENTER_H),crate::time_machine::MENU_VISIBLE_ROWS);
        assert_eq!(command_center_visible_rows(432),23);
        assert_eq!(command_center_visible_rows(272),10);
    }
    #[test]
    fn command_center_fits_native_framebuffer_and_keeps_pinned_regions_separate() {
        let (_,y,w,h)=TIME_PANEL_RECT;
        assert_eq!(w,640);
        assert_eq!(h,480);
        assert!(TIME_PANEL_CONTENT_TOP + crate::time_machine::MENU_VISIBLE_ROWS as i32*TIME_PANEL_LINE_HEIGHT < TIME_PANEL_FOOTER_Y);
        assert!(TIME_PANEL_FOOTER_Y < TIME_PANEL_MESSAGE_Y);
        assert!(TIME_PANEL_MESSAGE_Y < TIME_PANEL_INPUT_BOX_Y);
        assert!(TIME_PANEL_INPUT_TEXT_Y+7 < h);
        assert!(y+h <= HEIGHT as i32);
    }
    #[test]
    fn culture_lines_are_one_pixel_masked_and_star_dots_supersede_lines() {
        let mut scenery=Canvas::native_scenery();
        scenery.clear(0x102030);
        // A three-native-pixel-wide opaque building masks the existing scene.
        scenery.rect(3,3,1,1,0x102030);
        let mut canvas=Canvas::native_scene();
        canvas.clear(0x102030);
        canvas.enable_damage();canvas.mark_all();
        let picture=crate::constellation_render::Picture {
            lines:vec![crate::constellation_render::Line{x0:7,y0:10,x1:13,y1:10}],
            names:Vec::new(),
        };
        draw_culture_layer(&mut canvas,&scenery,&picture,1.0);
        assert_eq!(canvas.cells[10*WIDTH+7],crate::constellation_render::LINE_RGB);
        assert_eq!(canvas.cells[10*WIDTH+10],0x102030); // opaque scenery wins
        let stars=[crate::star_map::RenderedStar{pixel:space_zoo::star_projection::StarPixel{x:7,y:10},rgb:0xffffff,
            id:space_zoo::stellar::production::TychoId{region:1,number:1,component:1}}];
        draw_catalogue_star_layer(&mut canvas,&scenery,&stars);
        assert_eq!(canvas.cells[10*WIDTH+7],0xffffff);
        canvas.restore_marked(&scenery); // :culture off repaints background
        assert_eq!(canvas.cells[10*WIDTH+7],0x102030);
    }
    #[test]
    fn fast_forward_utc_jump_does_not_turn_hardware_watch_hands() {
        use crate::telemetry::SystemStats;
        let place=crate::astronomy::Location::new(40.123,-105.456).unwrap();
        let mut world=World::with_hour(320,200,49204,DisplayMode::Pixel,None);
        world.configure_sky(place,true);
        let mut painter=PixelRenderer::new();
        painter.update_billboard(Snapshot{dies:Vec::new(),rows:Vec::new(),
            system:SystemStats::default()});
        // Keep simulation and animation ticks fixed while jumping the sky
        // five days and eight hours: only sky and Time Machine HUD may change.
        world.sky.freeze_preview(1_792_000_000);
        world.refresh_time_machine();
        let capture_dial=|image:&[u32]|->Vec<u32> {
            let cx=crate::billboard::NULL_ISLAND_X as i32*ART_SCALE as i32+135;
            let cy=crate::billboard::NULL_ISLAND_Y as i32*ART_SCALE as i32+162;
            (-32..=32).flat_map(|dy|(-32..=32).filter_map(move |dx| {
                (dx*dx+dy*dy<=32*32).then_some(
                    image[(cy+dy) as usize*WIDTH+(cx+dx) as usize])
            })).collect()
        };
        let first=capture_dial(painter.draw_native(&world));
        world.sky.freeze_preview(1_792_000_000+5*86400+8*3600);
        world.refresh_time_machine();
        let second=capture_dial(painter.draw_native(&world));
        assert_eq!(first,second,"--ff UTC jump must not move the host clock's hands or date");
    }

    #[test]
    fn time_machine_keeps_live_sysmon_and_separate_utc_display() {
        use crate::telemetry::{SystemStats,Meter};
        let place=crate::astronomy::Location::new(40.123,-105.456).unwrap();
        let mut world=World::with_hour(320,200,49204,DisplayMode::Pixel,None);
        world.configure_sky(place,true);
        world.sky.set_command_pause(true); // pause must not pause host stats
        let mut painter=PixelRenderer::new();
        let mut system=SystemStats::default();
        system.memory=Meter{used:5_000_000,total:10_000_000};
        let snapshot=Snapshot {dies:Vec::new(),system,rows:vec![
            Row{name:"bertha".into(),mem:"5.0m".into(),cpu:"50%".into()},
            Row{name:"LIVEPROC".into(),mem:"3m".into(),cpu:"2%".into()},
        ]};
        painter.update_billboard(snapshot);
        let image=painter.draw_native(&world);
        // The original MEM meter and first process row must remain exposed:
        // previously the Time Machine panel completely covered both.
        let ox=crate::billboard::NULL_ISLAND_X as usize*ART_SCALE;
        let oy=crate::billboard::NULL_ISLAND_Y as usize*ART_SCALE;
        assert_eq!(image[(oy+254)*WIDTH+ox+28],0xd8e2e8); // centered MEM / 50% heading, first M pixel
        assert_eq!(image[(oy+303)*WIDTH+ox+18],0xece5d5); // top process
        // The five-row status retains a content-sized width; the larger damage
        // rectangle remains invisible and only clears stale pixels.
        let title="SPACE TIME MACHINE | FORWARD 1X";
        let utc="UTC 0000-00-00 00:00:00";
        let location="LAT +40.123000  LON -105.456000";
        let w=(title.chars().count().max(utc.chars().count())
            .max(location.chars().count())
            .max(FF_CONTROL_HINTS[0].chars().count())
            .max(FF_CONTROL_HINTS[1].chars().count()) as i32*6+14)
            .min(FF_STATUS_DAMAGE_RECT.2);
        let x=WIDTH as i32-w;
        let y=0;
        let h=FF_STATUS_VISIBLE_HEIGHT;
        assert!(w<222,"content-sized SPACE TIME MACHINE HUD should be tighter than the v0.54.09 fixed box");
        assert_eq!(image[y as usize*WIDTH+(x+1) as usize],0x83a9b7); // top status border, clear of left-border corner
        // `image` borrows `painter`; inspect the HUD after its final use.
        // This preserves the regression assertions and avoids E0502.
        assert!(painter.simulation_hud.is_some());
        assert!(painter.canvas.overlaps(x,y,w,h));
        let initial_clock=painter.simulation_hud.clone();
        painter.update_billboard(Snapshot {dies:Vec::new(),system:SystemStats::default(),rows:vec![
            Row{name:"bertha".into(),mem:"4.0m".into(),cpu:"10%".into()},
            Row{name:"LIVEPROC2".into(),mem:"6m".into(),cpu:"4%".into()},
        ]});
        painter.draw_native(&world);
        assert_eq!(painter.simulation_hud,initial_clock,
            "new live telemetry must not alter paused simulated UTC");
    }

    #[test]
    fn control_hints_fit_right_hud_without_widening_it_or_hitting_left_panel() {
        assert_eq!(FF_CONTROL_HINTS, [
            "WASD: VIEW  ARROWS: LOCATION",
            "R: RESET VIEW  ESC: COMMANDS",
        ]);
        let widest=FF_CONTROL_HINTS.iter().map(|s|s.chars().count()).max().unwrap() as i32*6+14;
        assert!(widest<222);
        assert!(FF_STATUS_VISIBLE_HEIGHT<=FF_STATUS_DAMAGE_RECT.3);
        assert!(FF_STATUS_VISIBLE_HEIGHT>52+7);
        assert!(WIDTH as i32-FF_STATUS_DAMAGE_RECT.2>=STAR_DIAGNOSTIC_PANEL_RECT.0+STAR_DIAGNOSTIC_PANEL_RECT.2);
    }

    #[test]
    fn simultaneous_sun_moon_invalidate_both_orb_footprints() {
        let mut world=World::with_hour(320,200,63,DisplayMode::Pixel,Some(13));
        world.configure_sky(crate::astronomy::Location::new(39.7392,-104.9903).unwrap(),false);
        world.sky.freeze_preview(1_791_998_800); // before the 17:29Z moonrise
        let mut painter=PixelRenderer::new();
        painter.draw_native(&world);
        assert!(painter.last_orb.is_some()); // daytime Sun remains visible
        assert!(painter.last_concurrent_moon.is_none());
        world.sky.freeze_preview(1_792_000_800); // 18:00Z: Sun AND Moon
        painter.draw_native(&world);
        let moon=painter.last_concurrent_moon.expect("daytime Moon must be tracked");
        assert_eq!(moon.0,crate::sky::Orb::Moon);
        let r=crate::lunar::OCCLUSION_RADIUS;
        assert!(painter.canvas.overlaps(moon.1-r,moon.2-r,2*r+1,2*r+1));
        assert_eq!(painter.last_orb.unwrap().0,crate::sky::Orb::Sun);
        world.sky.freeze_preview(1_792_004_400); // lunar motion independently damages old/new
        painter.draw_native(&world);
        let moved=painter.last_concurrent_moon.unwrap();
        assert_ne!((moon.1,moon.2),(moved.1,moved.2));
        assert!(painter.canvas.overlaps(moved.1-r,moved.2-r,2*r+1,2*r+1));
    }

    #[test]
    fn coop_front_and_back_are_occluded_in_the_correct_order() {
        use crate::sprite::Species;
        let mut world=World::with_hour(320,200,20261011,DisplayMode::Pixel,Some(13));
        world.animals.retain(|a|a.species==Species::Chicken);
        // Make every resident invisible so the exact coop artwork can be read.
        for a in &mut world.animals {a.inside_coop=true;}
        let mut painter=PixelRenderer::new();
        let plain=painter.draw_native(&world).to_vec();
        let coop=world.habitat.coop;
        // Its red wall is present, but nearby transparent roof corners remain scenery.
        let wall_x=(coop.x+5) as usize*ART_SCALE;
        let wall_y=(coop.y+coop.height-5) as usize*ART_SCALE;
        let roof_corner_x=(coop.x+1) as usize*ART_SCALE;
        let roof_corner_y=(coop.y+1) as usize*ART_SCALE;
        assert_eq!(plain[wall_y*WIDTH+wall_x],0xa83d36);
        assert_ne!(plain[roof_corner_y*WIDTH+roof_corner_x],0xa83d36);
    }
    #[test]
    fn native_scene_renders_day_dawn_and_night_without_changing_dimensions() {
        let mut sky_colors=Vec::new();
        for hour in [0,5,6,13,17,18,19,22] {
            let mut world=World::with_hour(320,200,91,DisplayMode::Pixel,Some(hour));
            let mut painter=PixelRenderer::new();
            for tick in 0..8 {
                if tick==4 {world.notice();}
                if tick==7 {world.greet();}
                world.advance();
                assert_eq!(painter.draw_native(&world).len(),WIDTH*HEIGHT);
            }
            // The sky is smoothly time colored; all animal simulation inputs
            // and original 3x block art remain at the same logical resolution.
            sky_colors.push(painter.draw_native(&world)[WIDTH+450]);
        }
        assert_ne!(sky_colors[0],sky_colors[3],"no day/night color variation");
    }

    #[test]
    fn native_billboard_only_changes_its_footprint() {
        let world=World::with_hour(320,200,123,DisplayMode::Pixel,Some(13));
        let mut painter=PixelRenderer::new();
        let baseline=painter.draw_native(&world).to_vec();
        painter.update_billboard(Snapshot {dies:Vec::new(),system:crate::telemetry::SystemStats::default(),rows:vec![Row {
            name:"bertha".into(),mem:"1.2g".into(),cpu:"3%".into()
        }]});
        let with_board=painter.draw_native(&world);
        assert_ne!(baseline,with_board);
        for y in 0..HEIGHT {for x in 0..WIDTH {
            if x < crate::billboard::X as usize*ART_SCALE ||
                x >= (crate::billboard::X as usize+crate::billboard::W)*ART_SCALE ||
                y < crate::billboard::Y as usize*ART_SCALE ||
                y >= (crate::billboard::Y as usize+crate::billboard::H)*ART_SCALE {
                assert_eq!(baseline[y*WIDTH+x],with_board[y*WIDTH+x]);
            }
        }}
    }

    #[test]
    fn native_billboard_has_sub_art_pixel_detail() {
        let world=World::with_hour(320,200,123,DisplayMode::Pixel,Some(13));
        let mut painter=PixelRenderer::new();
        let mut system=crate::telemetry::SystemStats::default();
        system.memory=crate::telemetry::Meter {used:1,total:2};
        painter.update_billboard(Snapshot {dies:Vec::new(),system,rows:vec![Row {
            name:"bertha".into(),mem:"1.2g".into(),cpu:"3%".into()
        }]});
        let image=painter.draw_native(&world);
        // The centered "MEM  50%" heading begins at local native x=28, y=254.
        // Its 5x7 M paints x=28 while adjacent x=29 remains meter background,
        // proving telemetry text still retains one-native-pixel detail.
        let x=crate::billboard::X as usize*ART_SCALE+28;
        let y=crate::billboard::Y as usize*ART_SCALE+254;
        assert_eq!(image[y*WIDTH+x],0xd8e2e8);
        assert_eq!(image[y*WIDTH+x+1],0x0e1a24);
    }

    #[test]
    fn helios_wheel_invalidation_follows_tourbillon_not_corona(){
        let mut world=World::with_hour(320,200,463,DisplayMode::Pixel,Some(13));
        let mut renderer=PixelRenderer::new();
        renderer.draw_native(&world);
        assert_eq!(renderer.last_sun_wheel_step,Some(0));
        world.tick=4;
        renderer.draw_native(&world);
        assert_eq!(renderer.last_sun_wheel_step,Some(0));
        world.tick=6; // visible 15 FPS draw immediately after tick 5
        renderer.draw_native(&world);
        assert_eq!(renderer.last_sun_wheel_step,Some(1));
        let [sun,_]=world.sky.celestial_pair(world.clock.seconds_today(),world.width,world.height);
        let sun=sun.expect("daytime sun");
        let (x,y,w,h)=crate::solar::wheel_damage_rect(sun.center.x,sun.center.y);
        assert!(renderer.canvas.overlaps(x,y,w,h));
        assert_eq!(renderer.last_orb.unwrap().3,0,"corona must keep 8-tick cadence");
        world.tick=8;
        renderer.draw_native(&world);
        assert_eq!(renderer.last_sun_wheel_step,Some(1));
        assert_eq!(renderer.last_orb.unwrap().3,1);
    }
    #[test]
    fn helios_mane_transition_invalidates_just_its_changed_native_pixels() {
        let mut world=World::with_hour(320,200,463,DisplayMode::Pixel,Some(13));
        let mut painter=PixelRenderer::new();
        world.tick=41;
        painter.draw_native(&world);
        let before=painter.last_orb.expect("daytime sun");
        assert_eq!(painter.last_sun_mane_step,Some(0));
        world.tick=42; // corona step 5 -> 5; wheel step 8 -> 8
        painter.draw_native(&world);
        assert_eq!(painter.last_orb,Some(before));
        assert_eq!(painter.last_sun_wheel_step,Some(8));
        assert_eq!(painter.last_sun_mane_step,Some(1));
        let (x,y,w,h)=crate::solar::mane_damage_rect(before.1,before.2);
        assert!(painter.canvas.overlaps(x,y,w,h));
        assert!(painter.canvas.touched(x as usize,y as usize));
        // Old whole-sun invalidation must not be necessary for this pose swap.
        assert!(painter.canvas.regions().unwrap().iter().any(|&tile|!tile));
    }
    #[test]
    fn retained_damage_matches_a_complete_repaint_for_day_and_night() {
        // Keep a separate source-faithful painter that redraws all 576,000
        // cells every frame. Compare every native pixel, not a checksum.
        for hour in [6,13,18,22] {
            let mut world=World::with_hour(320,200,452,DisplayMode::Pixel,Some(hour));
            let mut incremental=PixelRenderer::new();
            let mut reference=PixelRenderer::new();
            for frame in 0..125 {
                if frame==0 || frame==60 {
                    let snapshot=Snapshot {
                        rows:vec![crate::telemetry::Row {
                            name:"bertha".into(),mem:"1gb".into(),
                            cpu:if frame==0 {"0.6%".into()} else {"0.5%".into()},
                        }],
                        dies:Vec::new(),system:crate::telemetry::SystemStats::default(),
                    };
                    incremental.update_billboard(snapshot.clone());
                    reference.update_billboard(snapshot);
                }
                if frame==8 {world.greet();}
                if frame==12 {world.notice();}
                if frame==22 {world.helicopter();}
                if frame==33 {world.release_chickens();}
                if frame==56 {
                    // Force both disappearance (from the old region) and
                    // appearance (in a new region), with overlap repair.
                    world.animals[0].inside_coop=true;
                }
                if frame==62 {world.animals[0].inside_coop=false;}
                world.advance();
                reference.cached_hour=None; // force full scenery and canvas
                let expected=reference.draw_native(&world);
                let actual=incremental.draw_native(&world);
                assert_eq!(expected,actual,"hour={hour}, frame={frame}");
            }
        }
    }

    #[test]
    fn unchanged_background_is_not_copied_wholesale() {
        let world=World::with_hour(320,200,457,DisplayMode::Pixel,Some(13));
        let mut painter=PixelRenderer::new();
        let (_,first)=painter.draw_with_damage(&world);
        assert!(first.iter().all(|&tile|tile));
        let (_,second)=painter.draw_with_damage(&world);
        assert!(second.iter().any(|&tile|!tile),
            "an unchanged second frame should preserve untouched tiles");
    }

    #[test]
    fn native_rapid_mixed_input_does_not_panic() {
        use crate::input::{InputController,Action};
        use crossterm::event::{KeyCode,KeyEvent,KeyModifiers};
        let mut world=World::with_hour(320,200,321,DisplayMode::Pixel,Some(22));
        let mut painter=PixelRenderer::new();
        let mut input=InputController::new();
        let keys=[KeyCode::Left,KeyCode::Right,KeyCode::Up,KeyCode::Down,
            KeyCode::Enter,KeyCode::Esc,KeyCode::Char('x'),KeyCode::Char(' ')];
        for tick in 0..1500 {
            for key in keys {
                if let Some(action)=input.handle(KeyEvent::new(key,KeyModifiers::NONE),world.tick) {
                    match action {
                        Action::Quit=>{},
                        Action::Helicopter=>world.helicopter(),
                        Action::Dog(x,y)=>world.steer_dog(x,y),
                        Action::Greet=>world.greet(),
                        Action::Space=>world.space(),
                        Action::Notice=>world.notice(),
                        Action::ReleaseChickens=>{world.release_chickens();},
                        Action::Chupacabra=>{world.summon_mythic(crate::mythic::MythKind::Chupacabra);},
                        Action::Sasquatch=>{world.summon_mythic(crate::mythic::MythKind::Sasquatch);},
                    }
                }
            }
            world.advance();
            if tick % 30 == 0 {
                assert_eq!(painter.draw_native(&world).len(),WIDTH*HEIGHT);
            }
        }
    }

    #[test]
    fn catalogue_star_layer_erases_old_pixels_and_draws_new_pixels() {
        use space_zoo::star_projection::StarPixel;
        use space_zoo::stellar::production::TychoId;
        let render=|pixel,rgb| crate::star_map::RenderedStar {
            pixel,rgb,id:TychoId{region:1,number:1,component:1}
        };
        let mut scenery=Canvas::native_scenery();
        scenery.clear(0x112233);
        let mut canvas=Canvas::native_scene();
        canvas.enable_damage();
        canvas.mark_all();
        canvas.restore_marked(&scenery);
        let old=StarPixel{x:100,y:100};
        draw_catalogue_star_layer(&mut canvas,&scenery,&[render(old,0xaabbcc)]);
        assert_eq!(canvas.cells[100*WIDTH+100],0xaabbcc);

        canvas.enable_damage();
        let new=StarPixel{x:140,y:110};
        canvas.mark(Rect::new(old.x,old.y,1,1));
        canvas.mark(Rect::new(new.x,new.y,1,1));
        canvas.restore_marked(&scenery);
        draw_catalogue_star_layer(&mut canvas,&scenery,&[render(new,0xddeeff)]);
        assert_eq!(canvas.cells[100*WIDTH+100],0x112233,"old star must erase cleanly");
        assert_eq!(canvas.cells[110*WIDTH+140],0xddeeff,"new star must appear with native photometry RGB");
    }

    #[test]
    fn legacy_constellation_layer_is_disabled_only_in_ff_mode() {
        assert!(legacy_constellation_layer_enabled(false));
        assert!(!legacy_constellation_layer_enabled(true));
    }

    #[test]
    fn v055_null_island_is_default_and_legacy_zoo_is_og_only() {
        assert!(!legacy_zoo_scene_enabled(true));
        assert!(legacy_zoo_scene_enabled(false));
    }

    #[test]
    fn catalogue_stars_respect_static_occlusion_and_reappear_after_moving_actor_damage() {
        use space_zoo::star_projection::StarPixel;
        use space_zoo::stellar::production::TychoId;
        let render=|pixel,rgb| crate::star_map::RenderedStar {
            pixel,rgb,id:TychoId{region:1,number:1,component:1}
        };
        let mut scenery=Canvas::native_scenery();
        scenery.clear(0x010203);
        // Logical foreground cell covers native x=150..152, y=120..122.
        scenery.put(50,40,0x556677);
        let hidden=StarPixel{x:151,y:121};
        let revealed=StarPixel{x:200,y:200};
        let mut canvas=Canvas::native_scene();
        canvas.enable_damage();canvas.mark_all();canvas.restore_marked(&scenery);
        draw_catalogue_star_layer(&mut canvas,&scenery,&[render(hidden,0x102030),render(revealed,0xa0b0c0)]);
        assert_eq!(canvas.cells[121*WIDTH+151],0x556677,"static foreground must occlude star");
        assert_eq!(canvas.cells[200*WIDTH+200],0xa0b0c0);

        // A moving actor paints over a current star after the sky layer. On the
        // next frame its old rectangle is damaged/restored, then the same current
        // star layer repaints before the actor's new position.
        canvas.native_rect(199,199,3,3,0xaa0000);
        assert_eq!(canvas.cells[200*WIDTH+200],0xaa0000);
        canvas.enable_damage();canvas.mark(Rect::new(199,199,3,3));
        canvas.restore_marked(&scenery);
        draw_catalogue_star_layer(&mut canvas,&scenery,&[render(revealed,0xa0b0c0)]);
        assert_eq!(canvas.cells[200*WIDTH+200],0xa0b0c0,"moving foreground must reveal current star layer");
    }

}
