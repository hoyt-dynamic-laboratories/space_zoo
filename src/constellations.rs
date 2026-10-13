//! Rev .4427: persistent names for the .4426 northern-hemisphere star atlas.
//! J2000-ish bright-star coordinates (RA hours, declination degrees) are fixed;
//! Earth rotation / the calendar determine their apparent placement.  At 40N,
//! 100W, a full 360-degree compressed E-S-W-N panorama keeps both seasonal
//! southern figures and northern circumpolar figures in the same small scene.
//! This is an artistic cylindrical panorama, not a calibrated planetarium lens.
//! A native-pixel overlay is masked by the EXISTING hills/pines, before solar
//! bodies are drawn. No allocations, clock reads, or ephemeris calls per frame.
use std::f64::consts::{PI, TAU};
use crate::pixel::{Canvas, ART_SCALE};
use crate::world::FPS;

#[derive(Clone, Copy)]
struct Star { ra: f64, dec:f64, mag:f32, tint:u32 }
const fn s(ra:f64,dec:f64,mag:f32,tint:u32)->Star{Star{ra,dec,mag,tint}}
const SILVER:u32=0xe4edff;
const BLUE:u32=0xbcdcf6;
const GOLD:u32=0xf3d6aa;

// Only deliberately legible bright-star asterisms, not painted silhouettes.
// A few fourth-magnitude stars complete their recognizable outlines.
const ORION:[Star;8]=[
    s(5.9195,7.407,0.42,GOLD), s(5.4189,6.35,1.64,BLUE),
    s(5.5334,-0.30,2.23,BLUE),s(5.6036,-1.20,1.69,SILVER),
    s(5.6793,-1.94,1.74,BLUE),s(5.7959,-9.67,2.07,BLUE),
    s(5.2423,-8.20,0.13,BLUE),s(5.5856,9.93,3.33,SILVER)];
const ORION_LINES:&[(u8,u8)]=&[(0,7),(7,1),(0,4),(1,2),(2,3),(3,4),(4,5),(2,6),(5,6)];
const TAURUS:[Star;7]=[
    s(4.599,16.51,0.87,GOLD),s(5.438,28.61,1.65,BLUE),
    s(5.627,21.14,2.97,BLUE),s(4.477,15.87,3.4,SILVER),
    s(4.329,15.63,3.65,SILVER),s(4.382,17.54,3.76,SILVER),
    s(4.476,19.18,3.54,SILVER)];
const TAURUS_LINES:&[(u8,u8)]=&[(4,5),(5,6),(6,1),(4,3),(3,0),(0,2)];
const GEMINI:[Star;8]=[
    s(7.5766,31.89,1.58,SILVER),s(7.7553,28.03,1.14,GOLD),
    s(7.3354,21.98,3.53,SILVER),s(6.6285,16.40,1.93,BLUE),
    s(6.732,25.13,3.06,SILVER),s(6.382,22.51,2.87,GOLD),
    s(6.247,22.50,3.30,SILVER),s(6.754,12.90,3.35,SILVER)];
const GEMINI_LINES:&[(u8,u8)]=&[(0,4),(4,5),(5,6),(1,2),(2,3),(3,7),(0,1)];
const LEO:[Star;8]=[
    s(10.1395,11.97,1.36,BLUE),s(10.3329,19.84,2.01,GOLD),
    s(10.278,23.42,3.43,SILVER),s(9.879,26.01,3.90,SILVER),
    s(9.764,23.77,2.98,SILVER),s(11.235,20.52,2.56,SILVER),
    s(11.818,14.57,2.14,BLUE),s(11.238,15.43,3.33,SILVER)];
const LEO_LINES:&[(u8,u8)]=&[(0,1),(1,2),(2,3),(3,4),(4,1),(1,5),(5,6),(6,7),(7,0)];
const BOOTES:[Star;7]=[
    s(14.261,19.18,-0.05,GOLD),s(14.749,27.07,2.35,GOLD),
    s(15.032,40.39,3.49,SILVER),s(14.535,38.31,3.03,SILVER),
    s(13.911,18.40,2.68,GOLD),s(15.259,33.32,3.47,SILVER),
    s(14.530,30.37,3.58,SILVER)];
const BOOTES_LINES:&[(u8,u8)]=&[(0,4),(4,3),(3,2),(2,5),(5,1),(1,0),(3,6),(6,1)];
const DIPPER:[Star;7]=[
    s(11.062,61.75,1.79,GOLD),s(11.031,56.38,2.37,SILVER),
    s(11.897,53.69,2.44,SILVER),s(12.258,57.03,3.31,SILVER),
    s(12.901,55.96,1.77,SILVER),s(13.398,54.93,2.27,BLUE),
    s(13.792,49.31,1.86,BLUE)];
const DIPPER_LINES:&[(u8,u8)]=&[(0,1),(1,2),(2,3),(3,0),(3,4),(4,5),(5,6)];
const CYGNUS:[Star;6]=[
    s(20.690,45.28,1.25,BLUE),s(20.370,40.26,2.23,SILVER),
    s(20.771,33.97,2.46,GOLD),s(19.512,27.96,3.05,GOLD),
    s(19.750,45.13,2.86,SILVER),s(21.215,30.23,3.20,SILVER)];
const CYGNUS_LINES:&[(u8,u8)]=&[(0,1),(1,2),(2,3),(4,1),(1,5)];
const LYRA:[Star;5]=[
    s(18.6156,38.78,0.03,BLUE),s(18.8347,33.36,3.52,SILVER),
    s(18.9824,32.69,3.25,SILVER),s(18.908,36.90,4.28,SILVER),
    s(18.745,37.61,4.37,SILVER)];
const LYRA_LINES:&[(u8,u8)]=&[(0,4),(4,3),(3,2),(2,1),(1,4)];
const AQUILA:[Star;8]=[
    s(19.846,8.87,0.77,BLUE),s(19.771,10.61,2.72,GOLD),
    s(19.922,6.41,3.71,SILVER),s(19.424,3.11,3.36,SILVER),
    s(19.090,13.86,2.99,SILVER),s(20.189,-0.82,3.24,SILVER),
    s(19.104,-4.88,3.43,SILVER),s(19.525,11.93,3.33,SILVER)];
const AQUILA_LINES:&[(u8,u8)]=&[(1,0),(0,2),(0,7),(7,4),(0,3),(3,6),(0,5)];
const PEGASUS:[Star;7]=[
    s(23.079,15.21,2.49,SILVER),s(23.063,28.08,2.44,GOLD),
    s(0.1398,29.09,2.06,BLUE),s(0.2206,15.18,2.83,BLUE),
    s(21.736,9.88,2.39,GOLD),s(22.691,10.83,3.41,SILVER),
    s(22.717,30.22,2.95,SILVER)];
const PEGASUS_LINES:&[(u8,u8)]=&[(0,1),(1,2),(2,3),(3,0),(0,5),(5,4),(1,6)];
const CASSIOPEIA:[Star;5]=[
    s(0.153,59.15,2.27,SILVER),s(0.675,56.54,2.24,GOLD),
    s(0.945,60.72,2.15,BLUE),s(1.430,60.24,2.68,SILVER),
    s(1.906,63.67,3.35,BLUE)];
const CASSIOPEIA_LINES:&[(u8,u8)]=&[(0,1),(1,2),(2,3),(3,4)];
const ANDROMEDA:[Star;6]=[
    s(0.1398,29.09,2.06,BLUE),s(0.655,30.86,3.27,SILVER),
    s(1.162,35.62,2.07,GOLD),s(2.065,42.33,2.10,GOLD),
    s(0.946,38.50,3.85,SILVER),s(0.830,41.08,4.50,SILVER)];
const ANDROMEDA_LINES:&[(u8,u8)]=&[(0,1),(1,2),(2,3),(2,4),(4,5)];
// The celestial north-pole reference makes the circumpolar rotation legible.
const POLARIS:Star=s(2.5303,89.264,1.98,SILVER);

#[derive(Clone,Copy)]
struct Figure { name:&'static str, stars:&'static [Star], segments:&'static [(u8,u8)] }
const FIGURES:[Figure;12]=[
    Figure{name:"ORION",stars:&ORION,segments:ORION_LINES},
    Figure{name:"TAURUS",stars:&TAURUS,segments:TAURUS_LINES},
    Figure{name:"GEMINI",stars:&GEMINI,segments:GEMINI_LINES},
    Figure{name:"LEO",stars:&LEO,segments:LEO_LINES},
    Figure{name:"BOOTES",stars:&BOOTES,segments:BOOTES_LINES},
    Figure{name:"BIG DIPPER",stars:&DIPPER,segments:DIPPER_LINES},
    Figure{name:"CYGNUS",stars:&CYGNUS,segments:CYGNUS_LINES},
    Figure{name:"LYRA",stars:&LYRA,segments:LYRA_LINES},
    Figure{name:"AQUILA",stars:&AQUILA,segments:AQUILA_LINES},
    Figure{name:"PEGASUS",stars:&PEGASUS,segments:PEGASUS_LINES},
    Figure{name:"CASSIOPEIA",stars:&CASSIOPEIA,segments:CASSIOPEIA_LINES},
    Figure{name:"ANDROMEDA",stars:&ANDROMEDA,segments:ANDROMEDA_LINES},
];
const STAR_TOTAL:usize=82; // checked by tests; reused Alpheratz is intentionally in two figures
const LATITUDE:f64=40.0;
const LONGITUDE:f64=-100.0; // generic central North America, no geolocation/service
// Place the cylindrical seam behind the west-southwest horizon, not through
// the northern W / Big Dipper or the winter/summer anchor silhouettes.
const PANORAMA_SEAM_DEGREES:f64=215.0;

#[derive(Clone,Copy,Default)]
struct Pose {x:f32,y:f32,alt:f32,mag:f32,tint:u32}

// Julian Date -> Greenwich mean sidereal degrees (J2000/IAU conventional approximation).
fn sidereal_deg(unix_secs:i64)->f64 {
    let jd=unix_secs as f64/86_400.0+2_440_587.5;
    let d=jd-2_451_545.0;
    let t=d/36_525.0;
    (280.46061837+360.98564736629*d+0.000387933*t*t-t*t*t/38_710_000.0)
        .rem_euclid(360.0)
}

// Local equatorial-to-horizon projection with azimuth clockwise from NORTH.
fn horizon(ra_hours:f64, dec_degrees:f64, sidereal:f64)->(f64,f64){
    let lat=LATITUDE.to_radians();
    let dec=dec_degrees.to_radians();
    let ha=(sidereal+LONGITUDE-ra_hours*15.0).to_radians();
    let sin_alt=dec.sin()*lat.sin()+dec.cos()*lat.cos()*ha.cos();
    let alt=sin_alt.clamp(-1.0,1.0).asin().to_degrees();
    let east=-dec.cos()*ha.sin();
    let north=dec.sin()*lat.cos()-dec.cos()*lat.sin()*ha.cos();
    let az=east.atan2(north).rem_euclid(TAU).to_degrees();
    (az,alt)
}

fn pose(star:Star, lst:f64, w:usize,h:usize)->Pose {
    let (az,alt)=horizon(star.ra,star.dec,lst);
    // All 360 degrees fit the scene. WSW is the left/right wrap seam;
    // North, South, East remain fixed compass directions year-round.
    // Horizon is behind the existing distant hillside silhouette at ~2/3H.
    // Mild elevation compression reserves the height of the sky for stars.
    let top=h as f64*0.025;
    let horizon_y=h as f64*0.655;
    Pose {x:((az-PANORAMA_SEAM_DEGREES).rem_euclid(360.0)/360.0*w as f64) as f32,
        y:(horizon_y-(horizon_y-top)*alt/90.0) as f32,
        alt:alt as f32,mag:star.mag,tint:star.tint}
}
fn fade(a:f32,b:f32,x:f32)->f32 {
    let t=((x-a)/(b-a)).clamp(0.0,1.0);
    t*t*(3.0-2.0*t)
}
fn color_mix(old:u32,ink:u32,opacity:f32)->u32 {
    let a=opacity.clamp(0.0,1.0);
    let blend=|shift:u32| {
        let b=((old>>shift)&255) as f32;
        let f=((ink>>shift)&255) as f32;
        (b+(f-b)*a).round() as u32
    };
    blend(16)<<16 | blend(8)<<8 | blend(0)
}
fn ink(c:&mut Canvas,mask:&[bool],x:i32,y:i32,rgb:u32,alpha:f32){
    if alpha<=0.002 || x<0 || y<0 || x>=(c.w*c.scale) as i32 || y>=(c.h*c.scale) as i32 {return;}
    let x=x as usize;let y=y as usize;
    if mask[(y/c.scale)*c.w+x/c.scale] || !c.touched(x,y) {return;}
    let at=y*c.w*c.scale+x;
    c.cells[at]=color_mix(c.cells[at],rgb,alpha);
}
fn dot(c:&mut Canvas,mask:&[bool],p:Pose,night:f32,tick:u64,id:usize){
    if p.alt<0.0 {return;}
    let air=fade(0.0,12.0,p.alt);
    let bright=if p.mag<0.8 {1.0} else if p.mag<2.0 {0.82}
        else if p.mag<3.1 {0.64} else {0.45};
    let twinkle=0.968+0.032*((tick as f64/(FPS as f64*1.8)+id as f64*2.39).sin() as f32);
    let a=night*air*bright*twinkle;
    let cx=(p.x*c.scale as f32).round() as i32;
    let cy=(p.y*c.scale as f32).round() as i32;
    // Only strong stars have a tiny, feathered native-pixel halo.
    if p.mag<2.5 {
        for (dx,dy,weight) in [(-2,0,0.10),(2,0,0.10),(0,-2,0.10),(0,2,0.10),
                              (-1,0,0.29),(1,0,0.29),(0,-1,0.29),(0,1,0.29)] {
            ink(c,mask,cx+dx,cy+dy,p.tint,a*weight);
        }
    }
    ink(c,mask,cx,cy,p.tint,(a*1.30).min(1.0));
}
fn hairline(c:&mut Canvas,mask:&[bool],a:Pose,b:Pose,night:f32){
    // Clip segment to astronomical horizon; terrain is separately masked in ink().
    if a.alt<=0.0 && b.alt<=0.0 {return;}
    let mut x0=a.x;let mut y0=a.y;let mut alt0=a.alt;
    let mut x1=b.x;let mut y1=b.y;let mut alt1=b.alt;
    // Wrap the shortest way across the NORTH seam instead of drawing a 960px diagonal.
    let w=c.w as f32;
    if x1-x0>w*0.5 {x1-=w;} else if x0-x1>w*0.5 {x1+=w;}
    if alt0<0.0 {
        let t=-alt0/(alt1-alt0);
        x0+=(x1-x0)*t;y0+=(y1-y0)*t;alt0=0.0;
    } else if alt1<0.0 {
        let t=alt0/(alt0-alt1);
        x1=x0+(x1-x0)*t;y1=y0+(y1-y0)*t;alt1=0.0;
    }
    // 1 native px, softly blended. DDA traces with an intensity matched
    // to the subpixel crossing (not thick 3x3 game-pixel strokes).
    let sc=c.scale as f32;
    let x0=x0*sc;let y0=y0*sc;let x1=x1*sc;let y1=y1*sc;
    let steps=((x1-x0).abs().max((y1-y0).abs())).ceil() as usize;
    if steps==0 || steps>2500 {return;}
    let opacity=night*0.30*fade(0.0,9.0,alt0.max(alt1));
    for i in 0..=steps {
        let t=i as f32/steps as f32;
        let px=(x0+(x1-x0)*t).round() as i32;
        let py=(y0+(y1-y0)*t).round() as i32;
        // Repeated projected pixels are intentionally harmless; faint strokes.
        ink(c,mask,px,py,0x87a9db,opacity);
        // Seam continuation if the line crosses either edge.
        if px<0 {ink(c,mask,px+c.w as i32*c.scale as i32,py,0x87a9db,opacity);}
        if px>=(c.w*c.scale) as i32 {ink(c,mask,px-c.w as i32*c.scale as i32,py,0x87a9db,opacity);}
    }
}

fn mini_glyph(ch:char)->[u8;5]{
    match ch {
        'A'=>[2,5,7,5,5],'B'=>[6,5,6,5,6],'C'=>[3,4,4,4,3],
        'D'=>[6,5,5,5,6],'E'=>[7,4,6,4,7],'F'=>[7,4,6,4,4],
        'G'=>[3,4,5,5,3],'H'=>[5,5,7,5,5],'I'=>[7,2,2,2,7],
        'J'=>[3,1,1,5,2],'K'=>[5,5,6,5,5],'L'=>[4,4,4,4,7],
        'M'=>[5,7,7,5,5],'N'=>[5,7,7,7,5],'O'=>[2,5,5,5,2],
        'P'=>[6,5,6,4,4],'Q'=>[2,5,5,7,3],'R'=>[6,5,6,5,5],
        'S'=>[3,4,2,1,6],'T'=>[7,2,2,2,2],'U'=>[5,5,5,5,7],
        'V'=>[5,5,5,5,2],'W'=>[5,5,7,7,5],'X'=>[5,5,2,5,5],
        'Y'=>[5,5,2,2,2],'Z'=>[7,1,2,4,7],' '=>[0;5],_=>[0;5],
    }
}
fn label(c:&mut Canvas,mask:&[bool],x:i32,y:i32,title:&str) {
    for (i,ch) in title.chars().enumerate() {
        for (dy,bits) in mini_glyph(ch).iter().copied().enumerate() {
            for dx in 0..3 {
                if bits & (1<<(2-dx))!=0 {
                    ink(c,mask,x+i as i32*5+dx,y+dy as i32,0xa2bdd9,0.70);
                }
            }
        }
    }
}

pub struct ConstellationOverlay {
    cache_key:i64,
    points:Vec<Pose>,
    polaris:Pose,
    width:usize,
    height:usize,
}
impl ConstellationOverlay {
    pub fn new()->Self {Self{cache_key:i64::MIN,points:vec![Pose::default();
        FIGURES.iter().map(|f|f.stars.len()).sum()],polaris:Pose::default(),width:0,height:0}}
    // Star colors twinkle every frame, but the line map and labels only
    // reproject every four seconds. Return their small native halos for damage.
    pub fn star_centers(&self)->impl Iterator<Item=(i32,i32)> + '_ {
        self.points.iter().chain(std::iter::once(&self.polaris))
            .filter(|p|p.alt>=0.0)
            .map(|p|((p.x*ART_SCALE as f32).round() as i32,
                      (p.y*ART_SCALE as f32).round() as i32))
    }
    fn refresh(&mut self,unix:i64,w:usize,h:usize,diagnostic:bool){
        let bucket=if diagnostic {unix/60} else {unix/4};
        if self.cache_key==bucket && self.width==w && self.height==h {return;}
        let lst=sidereal_deg(unix);
        let mut n=0;
        for f in FIGURES {
            for star in f.stars {
                self.points[n]=pose(*star,lst,w,h);
                n+=1;
            }
        }
        self.polaris=pose(POLARIS,lst,w,h);
        self.cache_key=bucket;self.width=w;self.height=h;
    }
    /// `clock_unix`: system date plus `--hour` clock override, if any.
    /// Diagnostic mode advances star time alone at 30min per wall second;
    /// animal schedules and the existing moon/sun/planet systems do not change.
    pub fn draw(&mut self,c:&mut Canvas,scenery:&Canvas,clock_unix:i64,
        daylight:f32,tick:u64,diagnostic:bool) {
        let Some(mask)=scenery.occluded.as_ref() else {return;};
        let night=if diagnostic {1.0} else {(1.0-daylight).clamp(0.0,1.0).powi(3)};
        if night<0.015 {return;}
        let unix=if diagnostic {clock_unix.saturating_add((tick*60) as i64)}
            else {clock_unix};
        self.refresh(unix,c.w,c.h,diagnostic);
        // A steady true-north pivot; none of the constellations are sprites.
        dot(c,mask,self.polaris,night,tick,STAR_TOTAL);
        // .4427: display the same native-pixel names in normal night mode.
        // `diagnostic` still controls ONLY the accelerated inspection clock.
        if self.polaris.alt>12.0 {
            label(c,mask,(self.polaris.x*ART_SCALE as f32) as i32+5,
                (self.polaris.y*ART_SCALE as f32) as i32-12,"POLARIS");
        }
        let mut start=0;
        for f in FIGURES {
            let p=&self.points[start..start+f.stars.len()];
            for &(a,b) in f.segments {hairline(c,mask,p[a as usize],p[b as usize],night);}
            for (i,star) in p.iter().copied().enumerate() {
                dot(c,mask,star,night,tick,start+i);
            }
            // The identical position/visibility logic now runs in both modes.
            // Each label follows its stars, respecting the horizon and hills.
            {
                let (mut count,mut sum_x,mut sum_y)=(0,0.0_f32,0.0_f32);
                let (mut min_x,mut max_x)=(c.w as f32,0.0_f32);
                for s in p.iter().filter(|s|s.alt>12.0) {
                    count+=1;sum_x+=s.x;sum_y+=s.y;
                    min_x=min_x.min(s.x);max_x=max_x.max(s.x);
                }
                // Avoid long labels crossing the north wrap seam.
                if count>=3 && max_x-min_x<c.w as f32*0.4 {
                    label(c,mask,(sum_x/count as f32*ART_SCALE as f32) as i32,
                        (sum_y/count as f32*ART_SCALE as f32) as i32+10,f.name);
                }
            }
            start+=f.stars.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn all_twelve_figures_are_index_safe(){
        assert_eq!(FIGURES.len(),12);
        let n:usize=FIGURES.iter().map(|f|f.stars.len()).sum();
        assert_eq!(n,STAR_TOTAL);
        for figure in FIGURES {
            for &(a,b) in figure.segments {
                assert!((a as usize)<figure.stars.len());
                assert!((b as usize)<figure.stars.len());
            }
        }
    }
    #[test] fn north_pole_is_always_above_horizon_at_40n(){
        for t in [0,1_700_000_000,2_000_000_000] {
            let (_,alt)=horizon(2.5303,89.264,sidereal_deg(t));
            assert!((alt-40.0).abs()<1.0);
        }
    }
    #[test] fn one_sidereal_day_brings_same_star_back(){
        let f=ORION[0];
        let a=horizon(f.ra,f.dec,sidereal_deg(1_700_000_000));
        let b=horizon(f.ra,f.dec,sidereal_deg(1_700_086_164));
        assert!((a.0-b.0).abs()<0.03);
        assert!((a.1-b.1).abs()<0.03);
    }
    #[test] fn stars_below_the_horizon_are_suppressed(){
        let mut a=Canvas::scaled(100,70,ART_SCALE,false);
        a.clear(0x101010);
        let foreground=Canvas::scaled(100,70,ART_SCALE,true);
        let mask=foreground.occluded.unwrap();
        dot(&mut a,&mask,Pose{x:22.0,y:30.0,alt:-5.0,mag:1.0,tint:SILVER},1.0,0,0);
        assert!(a.cells.iter().all(|&rgb|rgb==0x101010));
    }
    #[test] fn occluded_star_never_paints_over_a_hill(){
        let mut a=Canvas::scaled(100,70,ART_SCALE,false);a.clear(0x101010);
        let mut scenery=Canvas::scaled(100,70,ART_SCALE,true);
        scenery.put(22,30,0x305030);
        let mask=scenery.occluded.unwrap();
        dot(&mut a,&mask,Pose{x:22.0,y:30.0,alt:40.0,mag:1.0,tint:SILVER},1.0,0,0);
        let pixel=(30*ART_SCALE+1)*100*ART_SCALE+22*ART_SCALE+1;
        assert_eq!(a.cells[pixel],0x101010);
    }
    #[test] fn normal_night_labels_polaris_without_inspection_flag(){
        // This glyph location is separate from the star itself. In .4426 the
        // label was missing without --constellation; in .4427 it is permanent.
        let mut overlay=ConstellationOverlay::new();
        let mut frame=Canvas::scaled(320,200,ART_SCALE,false);
        frame.clear(0x101010);
        let scenery=Canvas::scaled(320,200,ART_SCALE,true);
        overlay.draw(&mut frame,&scenery,1_800_000_000,0.0,0,false);
        assert!(overlay.polaris.alt>12.0);
        let x=(overlay.polaris.x*ART_SCALE as f32) as usize+5;
        let y=(overlay.polaris.y*ART_SCALE as f32) as usize-12;
        let first_pixel=frame.cells[y*320*ART_SCALE+x];
        // Top left stroke of the P: unchanged .4426 glyph and ink opacity.
        assert_eq!(first_pixel,color_mix(0x101010,0xa2bdd9,0.70));
    }
    #[test] fn star_map_is_night_only_and_has_no_fullscreen_buffers(){
        let mut overlay=ConstellationOverlay::new();
        assert_eq!(overlay.points.len(),STAR_TOTAL);
        let mut frame=Canvas::scaled(100,70,ART_SCALE,false);frame.clear(0x101010);
        let scenery=Canvas::scaled(100,70,ART_SCALE,true);
        overlay.draw(&mut frame,&scenery,1_700_000_000,1.0,0,false);
        assert!(frame.cells.iter().all(|&p|p==0x101010));
    }
}
