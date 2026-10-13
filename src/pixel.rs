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

pub struct PixelRenderer {
    canvas: Canvas,        // Exact native 960x600 scene, no second enlarged copy.
    scenery: Canvas,       // Cached native scenery, rebuilt on hour change.
    cached_hour: Option<u8>,
    cached_minute: Option<u8>,
    cached_dimensions: (u16, u16),
    depth_order: Vec<usize>,
    billboard: Option<Billboard>,
    moon:MoonSprite,
    sun:SunSprite,
    constellations:ConstellationOverlay,
    inspect_constellations:bool,
    coop_art: Canvas,           // 62x41 logical, 3x native; no full-screen layer
    cached_coop: Option<(crate::coop::Coop,bool)>,
    last_stars:Vec<crate::world::Point>,
    last_actors:Vec<Rect>,
    current_actors:Vec<Rect>,
    last_orb:Option<(crate::sky::Orb,i32,i32,u64)>,
    last_planet:Option<(crate::sky::PlanetKind,i32,i32)>,
    last_constellation_key:Option<i64>,
    last_billboard_gyro:Option<u64>,
    last_billboard_lighting:Option<bool>,
    billboard_updated:bool,
    last_billboard_date:Option<crate::circadian::ClockDate>,
    last_arc_jewel_day:Option<bool>,
}
impl PixelRenderer {
    pub fn new() -> Self {
        Self {
            canvas: Canvas::native_scene(),
            scenery: Canvas::native_scenery(),
            cached_hour: None,
            cached_minute: None,
            cached_dimensions: (0, 0),
            depth_order: Vec::with_capacity(28),
            billboard: None,
            moon:MoonSprite::new(),
            sun:SunSprite::new(),
            constellations:ConstellationOverlay::new(),
            inspect_constellations:false,
            coop_art: Canvas::scaled(62,41,ART_SCALE,false),
            cached_coop: None,
            last_stars:Vec::with_capacity(20),
            last_actors:Vec::with_capacity(40),
            current_actors:Vec::with_capacity(40),
            last_orb:None,last_planet:None,
            last_constellation_key:None,
            last_billboard_gyro:None,last_billboard_lighting:None,
            billboard_updated:false,last_billboard_date:None,
            last_arc_jewel_day:None,
        }
    }

    pub fn set_constellation_inspection(&mut self,on:bool){
        if self.inspect_constellations!=on {
            self.inspect_constellations=on;
            self.cached_hour=None; // guarantee a complete inspection-mode repaint
        }
    }

    // Telemetry arrives only once per 2 seconds. Never read /proc on a frame.
    pub fn update_billboard(&mut self, snapshot: Snapshot) {
        if let Some(b) = &mut self.billboard { b.update(snapshot); }
        else { self.billboard = Some(Billboard::new(snapshot)); }
        self.billboard_updated=true;
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
        if let Some(probe)=&world.debug_probe {
            if let Some(r)=crate::living_animals::animal_bounds(&probe.animal,world.tick) {
                self.current_actors.push(Rect::new(r.0,r.1,r.2,r.3));
            }
            return;
        }
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
    fn mark_animation(&mut self,world:&World,reset:bool) {
        self.canvas.enable_damage();
        if reset {self.canvas.mark_all();}
        // On ordinary frames the sky gradient and static hills aren't touched.
        // Mark previous and current small moving background stars.
        for p in self.last_stars.iter().chain(world.background.stars()) {
            self.canvas.mark(Rect::logical(p.x,p.y,1,1));
        }
        self.last_stars.clear();
        self.last_stars.extend_from_slice(world.background.stars());
        // The stellar catalog reprojects on its own existing four-second key.
        // Between projections only the native ~5px star halos twinkle.
        let unix=world.clock.observing_unix(world.sky.now_secs());
        let constellation_key=if self.inspect_constellations {
            unix.saturating_add((world.tick*60) as i64)/60
        } else {unix/4};
        let seconds=world.clock.seconds_today();
        let daylight=crate::sky::daylight_strength(seconds);
        let active=self.inspect_constellations || daylight<0.985;
        // During dawn/dusk the constellation line opacity changes continuously;
        // preserve the original full-scene fade rather than leaving stale ink.
        let twilight=(19_800..=27_000).contains(&seconds) ||
            (63_000..=70_200).contains(&seconds);
        if active {
            if twilight || self.last_constellation_key!=Some(constellation_key) {
                self.canvas.mark_all();
            } else {
                for (x,y) in self.constellations.star_centers() {
                    self.canvas.mark(Rect::new(x-3,y-3,7,7));
                }
            }
        }
        self.last_constellation_key=Some(constellation_key);
        let jewel_day=daylight>0.45;
        if self.last_arc_jewel_day.is_some_and(|last|last!=jewel_day) {
            for n in 1..8 {
                let p=crate::sky::position_on_sun_arc(n as f32/8.0,
                    world.width,world.height);
                self.canvas.mark(Rect::logical(p.x,p.y,2,1));
            }
        }
        self.last_arc_jewel_day=Some(jewel_day);
        // The corona really changes once per eight simulation ticks; avoid
        // touching its area on the intervening 15 FPS refreshes.
        let orb=crate::sky::celestial_pair(world.clock.seconds_today(),world.width,world.height)
            .into_iter().flatten().next();
        let phase=world.sky.moon_phase().to_bits() as u64;
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
        let planet=world.sky.active_planet(world.width,world.height);
        let next_planet=planet.map(|p|(p.kind,p.center.x,p.center.y));
        if next_planet!=self.last_planet {
            for p in [self.last_planet,next_planet].into_iter().flatten() {
                let radius=p.0.radius()+6;
                self.canvas.mark(Rect::logical(p.1-radius,p.2-radius,radius*2+1,radius*2+1));
            }
        }
        self.last_planet=next_planet;
        // The existing 0.451 billboard cache emits only clock / sign /proc
        // changes. No cabinet repaint just because an animal took a step.
        if self.billboard.is_some() {
            let gyro=world.tick/5;
            let lighting=!world.clock.is_day() && (2..=5).contains(&((world.tick/12)%8));
            let date=world.clock.date();
            if self.billboard_updated {
                self.canvas.mark(Rect::logical(crate::billboard::X,crate::billboard::Y,
                    crate::billboard::W as i32,crate::billboard::H as i32));
            } else {
                if self.last_billboard_gyro!=Some(gyro) || self.last_billboard_date!=Some(date) {
                    self.canvas.mark(Rect::new(crate::billboard::X*3+85,
                        crate::billboard::Y*3+112,102,102));
                }
                if self.last_billboard_lighting!=Some(lighting) {
                    self.canvas.mark(Rect::new(crate::billboard::X*3+259,
                        crate::billboard::Y*3+108,52,109));
                }
            }
            self.last_billboard_gyro=Some(gyro);
            self.last_billboard_lighting=Some(lighting);
            self.last_billboard_date=Some(date);
        }
        self.billboard_updated=false;
        self.actor_damage(world);
        for r in self.last_actors.iter().chain(self.current_actors.iter()) {
            self.canvas.mark(*r);
        }
        std::mem::swap(&mut self.last_actors,&mut self.current_actors);
    }
    // Both framebuffer and desktop window receive the SAME 960x600 pixels.
    // No 320->960 enlarge stage and no extra desktop copy.
    pub fn draw_native(&mut self, world: &World) -> &[u32] {
        let dimensions=(world.width, world.height);
        let reset=self.cached_hour != Some(world.clock.hour()) ||
            self.cached_minute != Some(world.clock.minute()) ||
            self.cached_dimensions != dimensions;
        if reset {
            pixel_scenery::draw_static(&mut self.scenery, world);
            self.cached_hour=Some(world.clock.hour());
            self.cached_minute=Some(world.clock.minute());
            self.cached_dimensions=dimensions;
        }
        self.mark_animation(world,reset);
        self.canvas.restore_marked(&self.scenery);
        // The physical day/date window advances with Linux local midnight.
        if let Some(board)=&mut self.billboard {board.set_date(world.clock.date());}
        // Original draw order: twinkling stars underneath sun, trees, hills,
        // buildings. The occlusion mask preserves that exactly, including the
        // moon's dark cutout, without repainting scenery every frame.
        let mask=self.scenery.occluded.as_ref().expect("scenery has occlusion mask");
        for star in world.background.stars() {
            if star.x>=0 && star.y>=0 && star.x < LOGICAL_W as i32 && star.y < LOGICAL_H as i32 {
                let at=star.y as usize*LOGICAL_W + star.x as usize;
                if !mask[at] { self.canvas.put(star.x,star.y,pixel_scenery::star_color(world)); }
            }
        }
        // Native subpixels, clipped against the EXISTING static scenery mask.
        // Compose behind the moon/sun/planets, not on top of celestial bodies.
        self.constellations.draw(&mut self.canvas,&self.scenery,
            world.clock.observing_unix(world.sky.now_secs()),
            crate::sky::daylight_strength(world.clock.seconds_today()),
            world.tick,self.inspect_constellations);
        pixel_scenery::draw_dynamic_sky(&mut self.canvas, world,&mut self.moon,&mut self.sun);
        pixel_scenery::occlude_sky(&mut self.canvas,&self.scenery,world);
        self.refresh_coop(world);
        // Painter's order uses feet-depth rather than spawn order: adults
        // occupy backfield, hens mid-yard, dog/cat the foreground. Reuse the
        // sorting allocation to keep Bertha's framebuffer path lightweight.
        self.depth_order.clear();
        let existing=world.animals.len();
        if world.debug_probe.is_some() {
            // Keep the original isolated --debug anchor probe unchanged.
            self.depth_order.push(usize::MAX);
        } else {
            self.depth_order.extend(0..existing);
            self.depth_order.extend((0..4).filter(|&i|world.deer.residents[i].visible)
                .map(|i|existing+i));
            self.depth_order.extend((0..2).filter(|&i|world.mythic.visitors[i].visible())
                .map(|i|existing+4+i));
        }
        self.depth_order.sort_unstable_by_key(|&i| {
            if i==usize::MAX {
                let a=&world.debug_probe.as_ref().unwrap().animal;
                staging::depth(a.species,a.visual_pos(world.tick))
            } else if i<existing {
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
            let depth=if i==usize::MAX {
                let a=&world.debug_probe.as_ref().unwrap().animal;
                staging::depth(a.species,a.visual_pos(world.tick))
            } else if i<existing {
                let a=&world.animals[i];
                staging::depth(a.species,a.visual_pos(world.tick))
            } else if i<existing+4 {world.deer.residents[i-existing].feet.y}
                else {world.mythic.visitors[i-existing-4].feet.y};
            if !supports_done && depth>=support_depth {
                if let Some(board)=&mut self.billboard {
                    board.draw_supports(&mut self.canvas,world.clock.is_day(),
                        world.tick,world.clock.seconds_today());
                }
                supports_done=true;
            }
            if !coop_done && depth>=coop_depth {
                self.composite_coop(world);
                coop_done=true;
            }
            if i==usize::MAX {
                pixel_sprite::draw_animal(&mut self.canvas,
                    &world.debug_probe.as_ref().unwrap().animal,world.tick);
            } else if i<existing {
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
                    world.tick,world.clock.seconds_today());
            }
        }
        if !coop_done {self.composite_coop(world);}
        // Flying/perched guineas must paint above the roof after the normal
        // feet-sorted ground pass, as the invited birds already do.
        if world.debug_probe.is_none() {
            for animal in &world.animals {
                if animal.is_perching() {
                    pixel_sprite::draw_animal(&mut self.canvas,animal,world.tick);
                }
            }
        }
        // In --debug, keep the one probe easy to follow; the sky and all
        // stationary scenery continue to render normally.
        if world.debug_probe.is_none() {
        world.bats.draw_pixel(&mut self.canvas,world.tick);
        for bird in world.birds.visual(world.tick,world.width) {
            pixel_birds::draw(&mut self.canvas,&bird);
        }
        if let Some(visitor)=world.events.visual(world.tick,world.width) {
            if let Some(kind)=world.events.visible_kind() {
                pixel_sprite::draw_visitor(&mut self.canvas,kind,visitor.pos,world.tick);
            }
        }
        }
        if let Some(board)=&mut self.billboard {
            board.draw_faces(&mut self.canvas,world.clock.is_day(),
                world.tick,world.clock.seconds_today());
        }
        &self.canvas.cells
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
        painter.update_billboard(Snapshot {dies:Vec::new(),system:crate::telemetry::SystemStats::default(),rows:vec![Row {
            name:"bertha".into(),mem:"1.2g".into(),cpu:"3%".into()
        }]});
        let image=painter.draw_native(&world);
        let x=crate::billboard::X as usize*ART_SCALE+17;
        let y=crate::billboard::Y as usize*ART_SCALE+235;
        // The micro-text's first digit is a native 1px stroke.
        assert_eq!(image[y*WIDTH+x],0x9cafbb);
        assert_eq!(image[y*WIDTH+x+4],0x0e1a24);
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

}
