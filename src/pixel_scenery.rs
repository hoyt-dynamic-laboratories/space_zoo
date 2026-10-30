//! Pixel scenery ONLY: original twenty-star background, hills/pines, red coop.
//! Animal state and keyboard handling do not belong in this file.
use crate::habitat::Place;
use crate::pixel::{Canvas,ART_SCALE};
use crate::world::World;
use crate::sky::{PlanetKind,PlanetVisual};
use crate::lunar::MoonSprite;
use crate::solar::SunSprite;
use crate::planet_sprite;

// Paint a minute-cached sky with native pixel-row gradients; no per-frame fill.
// We use native_rect so these sky pixels do NOT count as foreground occluders.
pub fn draw_static(c:&mut Canvas,world:&World){
    c.clear(0);
    draw_sky_gradient(c,world);
    if world.sky.fast() {
        crate::null_island::draw_mountains(c);
    } else {
        draw_foreground(c,world);
    }
}
fn lerp(a:u8,b:u8,t:f32)->u32 {(a as f32+(b as f32-a as f32)*t).round() as u32}
fn rgb_blend(a:u32,b:u32,t:f32)->u32{
    let r=lerp((a>>16) as u8,(b>>16) as u8,t);
    let g=lerp((a>>8) as u8,(b>>8) as u8,t);
    let bl=lerp(a as u8,b as u8,t);
    (r<<16)|(g<<8)|bl
}
fn draw_sky_gradient(c:&mut Canvas,world:&World){
    let amount=world.sky.solar_daylight(world.clock.seconds_today());
    let twilight=1.0-((amount-0.5).abs()*2.0).min(1.0);
    for y in 0..c.h {
        let depth=y as f32/c.h as f32;
        let night=rgb_blend(0x040817,0x142840,depth);
        let blue=rgb_blend(0x7ab9e9,0xa7d9f5,depth);
        let mut color=rgb_blend(night,blue,amount);
        // Rose-gold around the horizon, like a tiny enamel watch dial.
        let low=((depth-0.32)/0.40).clamp(0.0,1.0);
        color=rgb_blend(color,0xd9a38f,twilight*low*0.38);
        c.native_rect(0,(y*c.scale) as i32,(c.w*c.scale) as i32,c.scale as i32,color);
    }
}
pub fn star_color(world:&World)->u32 {
    if world.sky.solar_daylight(world.clock.seconds_today())>0.50 {0xe8fbff} else {0x55deeb}
}

// Deliberate jewelled hour indices, not 23 stray-looking track points.
fn arc_jewels(c:&mut Canvas,world:&World){
    let tint=if world.sky.solar_daylight(world.clock.seconds_today())>0.45 {
        0xd9e8ee
    } else {0x93bfd5};
    for n in 1..8 {
        let point=crate::sky::position_on_sun_arc(n as f32/8.0,world.width,world.height);
        c.put(point.x,point.y,0x456478);
        c.native_put(point.x*3+1,point.y*3+1,tint);
        if n==3 || n==6 {c.native_put(point.x*3+2,point.y*3,0xffd1a0);}
    }
}

pub fn draw_dynamic_sky(c:&mut Canvas,world:&World,moon_sprite:&mut MoonSprite,sun_sprite:&mut SunSprite,
        planets:&[Option<PlanetVisual>;8],planet_sprites:&mut planet_sprite::PlanetSpriteCache,
        appearance:crate::time_machine::PlanetAppearance){
    arc_jewels(c,world);
    let [sun,moon]=world.sky.celestial_pair(world.clock.seconds_today(),world.width,world.height);
    if let Some(sun)=sun {
        sun_sprite.draw(c,sun.center.x,sun.center.y,world.tick);
    }
    if let Some(moon)=moon {
        // The solar sprite paints first, and its *actual* visible pixels are
        // protected when the lunar pass overlaps it. Its translucent corona
        // therefore keeps exactly the same color; nothing covers the Sun.
        let sun_protection=sun.map(|s|(&*sun_sprite,s.center.x,s.center.y,world.tick));
        moon_sprite.draw_with_sun(c,moon.center.x,moon.center.y,
            world.sky.moon_phase(),
            world.sky.solar_daylight(world.clock.seconds_today()),sun_protection);
    }
    if appearance==crate::time_machine::PlanetAppearance::Graphic {
        planet_sprites.sync_visible(planets);
        for planet in planets.iter().flatten().copied() {
            if !planet_sprites.draw(c,planet.kind,planet.center.x,planet.center.y,world.tick){
                draw_planet(c,planet);
            }
            draw_planet_label(c,planet,appearance);
        }
    } else {
        planet_sprites.sync_visible(&[None;8]);
        for planet in planets.iter().flatten().copied() {
            draw_planet_point(c,planet);
            draw_planet_label(c,planet,appearance);
        }
    }
}

// Only restore foreground pixels inside celestial footprints, not entire scenery.
pub fn occlude_sky(c:&mut Canvas,scenery:&Canvas,world:&World,planets:&[Option<PlanetVisual>;8],
        appearance:crate::time_machine::PlanetAppearance){
    for orb in world.sky.celestial_pair(world.clock.seconds_today(),world.width,world.height)
        .into_iter().flatten(){
        let radius=if orb.kind==crate::sky::Orb::Moon {
            crate::lunar::OCCLUSION_RADIUS
        } else {crate::solar::OCCLUSION_RADIUS};
        restore_occluded(c,scenery,orb.center.x,orb.center.y,radius);
    }
    for p in planets.iter().flatten() {
        if appearance==crate::time_machine::PlanetAppearance::Graphic {
            let (x,y,w,h)=planet_sprite::native_bounds(p.kind,p.center.x,p.center.y);
            let s=c.scale as i32;
            restore_occluded_area(c,scenery,x.div_euclid(s),y.div_euclid(s),
                (x+w-1).div_euclid(s),(y+h-1).div_euclid(s));
        } else { restore_occluded(c,scenery,p.center.x,p.center.y,1); }
        let (x,y,w,h)=planet_label_bounds(p.kind,p.center.x,p.center.y,appearance);
        let s=c.scale as i32;
        restore_occluded_area(c,scenery,x.div_euclid(s),y.div_euclid(s),
            (x+w-1).div_euclid(s),(y+h-1).div_euclid(s));
    }
    for n in 1..8 {
        let point=crate::sky::position_on_sun_arc(n as f32/8.0,world.width,world.height);
        restore_occluded(c,scenery,point.x,point.y,0);
    }
}
fn restore_occluded(c:&mut Canvas,scenery:&Canvas,x:i32,y:i32,r:i32){
    restore_occluded_area(c,scenery,x-r,y-r,x+r,y+r);
}
fn restore_occluded_area(c:&mut Canvas,scenery:&Canvas,
        left:i32,top:i32,right:i32,bottom:i32){
    let Some(mask)=&scenery.occluded else{return;};
    let scale=c.scale;
    let width=c.w*scale;
    if left>right || top>bottom {return;}
    for yy in top.max(0)..=bottom.min(c.h as i32-1){
        for xx in left.max(0)..=right.min(c.w as i32-1){
            if !mask[yy as usize*c.w+xx as usize]{continue;}
            let sx=xx as usize*scale;
            for sub in 0..scale {
                let base=(yy as usize*scale+sub)*width+sx;
                for dx in 0..scale {
                    if c.touched(sx+dx,yy as usize*scale+sub) {
                        c.cells[base+dx]=scenery.cells[base+dx];
                    }
                }
            }
        }
    }
}


const PLANET_LABEL_HEIGHT:i32=7;
const PLANET_LABEL_GAP:i32=4;

pub(crate) fn planet_label_bounds(kind:PlanetKind,x:i32,y:i32,appearance:crate::time_machine::PlanetAppearance)->(i32,i32,i32,i32) {
    let text_w=kind.name().chars().count() as i32*6;
    let (body_left,body_top,body_w,body_h)=if appearance==crate::time_machine::PlanetAppearance::Graphic {
        planet_sprite::native_bounds(kind,x,y)
    } else {
        let s=ART_SCALE as i32;
        (x*s-s,y*s-s,s*3,s*3)
    };
    let body_center_x=body_left+body_w/2;
    let x=(body_center_x-text_w/2).clamp(2,(crate::pixel::WIDTH as i32-text_w-2).max(2));
    let below=body_top+body_h+PLANET_LABEL_GAP;
    let y=if below+PLANET_LABEL_HEIGHT<=crate::pixel::HEIGHT as i32-2 {below}
        else {(body_top-PLANET_LABEL_GAP-PLANET_LABEL_HEIGHT).max(2)};
    (x,y,text_w,PLANET_LABEL_HEIGHT)
}

fn draw_planet_label(c:&mut Canvas,planet:PlanetVisual,appearance:crate::time_machine::PlanetAppearance) {
    let (x,y,_,_)=planet_label_bounds(planet.kind,planet.center.x,planet.center.y,appearance);
    crate::billboard::overlay_text(c,x,y,planet.kind.name(),1,crate::constellation_render::NAME_RGB);
}

fn draw_planet_point(c:&mut Canvas,planet:PlanetVisual) {
    let x=planet.center.x;let y=planet.center.y;
    c.disc(x,y,1,planet.color);
    c.native_put(x*c.scale as i32+1,y*c.scale as i32+1,planet.accent);
}

fn draw_planet(c: &mut Canvas, planet: PlanetVisual) {
    let x = planet.center.x;
    let y = planet.center.y;
    if planet.ring {
        c.rect(x - planet.radius - 3, y - 1, planet.radius * 2 + 6, 1, 0xb28f65);
        c.rect(x - planet.radius - 2, y, planet.radius * 2 + 4, 1, 0xe7d1a4);
    }
    c.disc(x, y, planet.radius, planet.color);
    c.disc(x - planet.radius / 3, y - planet.radius / 3, (planet.radius / 2).max(1), planet.accent);
    if matches!(planet.kind, crate::sky::PlanetKind::Jupiter) {
        c.rect(x - planet.radius, y - 1, planet.radius * 2, 1, 0xbd8854);
    }
}

// Exact far-hill silhouette, shared with the billboard's embedded feet.
// No new drawing or terrain pass is needed to mask the support ends.
pub(crate) fn far_hill_top(x:i32, height:i32)->i32 {
    let wave=((x*7/19)%25-12).abs();
    height*2/3-wave/2
}

fn draw_foreground(c: &mut Canvas, world: &World) {
    let daylight=world.sky.solar_daylight(world.clock.seconds_today());
    let far=rgb_blend(0x102d38,0x40798b,daylight);
    let near_color=rgb_blend(0x193c31,0x467c5d,daylight);
    for x in 0..c.w as i32 {
        let high=far_hill_top(x,c.h as i32);
        c.rect(x,high,1,c.h as i32-high,
            far);
        let near_y=c.h as i32*4/5+((x/10)%7-3).abs();
        c.rect(x,near_y,1,c.h as i32-near_y,
            near_color);
    }
    for landmark in world.habitat.landmarks.iter().filter(|l|l.place==Place::Pine) {
        let x=landmark.point.x;
        let base=landmark.point.y;
        let tree_h=16+(x%5)*2;
        for dy in 0..tree_h {
            let half=(tree_h-dy)/4+1;
            c.rect(x-half,base-dy,half*2+1,1,0x295744);
            if dy%5==0 { c.put(x-half+2,base-dy,0x39735b); }
        }
        c.rect(x-1,base,3,5,0x443c2e);
    }
    // Same Rev .21 curiosity landmarks; no additional interaction destinations.
    for mark in &world.habitat.landmarks {
        match mark.place {
            Place::Scratch => {
                c.put(mark.point.x,mark.point.y,0x746549);
                c.put(mark.point.x+2,mark.point.y+1,0x746549);
            }
            Place::Rock => {
                c.rect(mark.point.x-2,mark.point.y-1,5,3,0x53616a);
                c.put(mark.point.x-1,mark.point.y-1,0x8a9294);
            }
            _=>{}
        }
    }
}

// Called on day/geometry changes into a tiny transparent, native-resolution
// buffer. Placement is handled by the depth compositor in pixel.rs.
pub fn paint_coop(c: &mut Canvas, p:crate::coop::Coop, day:bool) {
    let (x,y,w,h)=(p.x,p.y,p.width,p.height);
    // SNES-inspired small handcrafted building: layered roof, clapboard,
    // window, dark functional doorway, and nighttime warm windowlight.
    let roof_h=(h/3).max(6);
    for row in 0..roof_h {
        let inset=(roof_h-row)*w/(roof_h*3);
        c.rect(x+inset-2,y+row,w-2*inset+4,1,
            if row<3 {0x4d1e26} else if row%3==0 {0x873337} else {0x682732});
    }
    let wall_y=y+roof_h;
    c.rect(x+3,wall_y,w-6,h-roof_h,0x571f26);
    c.rect(x+5,wall_y+2,w-10,h-roof_h-4,0xa83d36);
    for ry in (wall_y+4..y+h-2).step_by(5) {
        c.rect(x+5,ry,w-10,1,0xc95542);
        c.put(x+9,ry+1,0x7e2932);
        c.put(x+w-9,ry+1,0x7e2932);
    }
    // Dark doorway is visible and reachable by the birds.
    let dw=(w/4).max(9);
    let dh=(h/2).max(11);
    let dx=p.door.x-dw/2;
    let dy=y+h-dh;
    c.rect(dx-2,dy-2,dw+4,dh+2,0xd48b54);
    c.rect(dx,dy,dw,dh,0x100f19);
    c.rect(dx+2,dy+2,dw-4,dh-2,0x22171e);
    c.put(dx+2,dy+3,0x69413b);
    // Little window left of entrance, illuminated after sunset.
    let wx=x+11;
    let wy=wall_y+6;
    c.rect(wx-2,wy-2,12,11,0x63302d);
    c.rect(wx,wy,8,7,if day {0x8fb6a2} else {0xf3c76a});
    c.rect(wx+3,wy,1,7,0x5e312b);
    c.rect(wx,wy+3,8,1,0x5e312b);
    c.rect(x+2,y+h-2,w-4,3,0x56382d);
}
