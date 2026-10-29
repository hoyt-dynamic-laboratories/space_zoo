//! v0.53.3: session-only rendering of audited, uniquely bound Stellarium line definitions.
//! No constellation interpolation, catalog changes, missing-endpoint repair, or frame-time I/O.
use std::collections::HashMap;
use space_zoo::stellar::culture::{CultureDataset, EndpointStatus};
use space_zoo::stellar::production::TychoId;
use space_zoo::star_projection::{Camera, HORIZONTAL_FOV_DEG};

pub const WESTERN_DEFAULT: &str = "modern_iau"; // Cleaner IAU-defined modern Western culture
pub const LINE_RGB: u32 = 0x485867;
pub const NAME_RGB: u32 = 0x87949f;

#[derive(Clone, Copy, Debug)]
pub struct Horizontal { pub altitude: f64, pub azimuth: f64 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {pub x0:i32,pub y0:i32,pub x1:i32,pub y1:i32}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {pub text:String,pub x:i32,pub y:i32,pub first_line:usize,pub end_line:usize}
#[derive(Default)]
pub struct Picture {pub lines:Vec<Line>,pub names:Vec<Name>}

struct Figure {name: String, pairs:Vec<(usize,usize)>}
struct Culture {id:String, figures:Vec<Figure>}
#[derive(Default)]
pub struct Layer {
    cultures:Vec<Culture>, active:Option<usize>, diagnostics:bool,
    pub current:Picture, pub previous:Picture,
}

impl Layer {
    /// All successful bindings are tied to the same selected Tycho records as the star dots.
    pub fn new(data:&CultureDataset, selected:&HashMap<TychoId,usize>)->Result<Self,String> {
        let mut cultures=Vec::with_capacity(data.cultures.len());
        for culture in &data.cultures {
            let mut figures=Vec::with_capacity(culture.figure_count);
            for figure in &data.figures[culture.first_figure..culture.first_figure+culture.figure_count] {
                let mut pairs=Vec::new();
                for seg in &data.segments[figure.first_segment..figure.first_segment+figure.segment_count] {
                    if !seg.selection_complete { continue; }
                    if !seg.identity_resolved || seg.a.status!=EndpointStatus::UniqueMatch ||
                        seg.b.status!=EndpointStatus::UniqueMatch || seg.a.matches.len()!=1 || seg.b.matches.len()!=1 ||
                        !seg.a.matches[0].render_eligible || !seg.b.matches[0].render_eligible {
                        return Err(format!("audited eligibility disagreement at segment {}",seg.segment_index));
                    }
                    let a=selected.get(&seg.a.matches[0].tycho_id).copied()
                        .ok_or_else(||format!("selected Tycho endpoint missing: {}",seg.a.matches[0].tycho_id))?;
                    let b=selected.get(&seg.b.matches[0].tycho_id).copied()
                        .ok_or_else(||format!("selected Tycho endpoint missing: {}",seg.b.matches[0].tycho_id))?;
                    pairs.push((a,b));
                }
                figures.push(Figure{name:figure.figure_name.clone(),pairs});
            }
            cultures.push(Culture{id:culture.culture.clone(),figures});
        }
        let active=cultures.iter().position(|c|c.id==WESTERN_DEFAULT)
            .ok_or_else(||format!("canonical Western culture '{WESTERN_DEFAULT}' absent"))?;
        Ok(Self {cultures,active:Some(active),diagnostics:false,current:Picture::default(),previous:Picture::default()})
    }
    pub fn selected(&self)->&str {self.active.map_or("off",|i|&self.cultures[i].id)}
    pub fn available(&self)->Vec<&str> {self.cultures.iter().map(|c|c.id.as_str()).collect()}
    pub fn debugstars(&self)->bool {self.diagnostics}
    pub fn set_debugstars(&mut self,on:bool)->bool {let changed=self.diagnostics!=on;self.diagnostics=on;changed}
    pub fn set_culture(&mut self,id:&str)->Result<bool,String> {
        let choice=if id.eq_ignore_ascii_case("off") {None} else {
            Some(self.cultures.iter().position(|c|c.id.eq_ignore_ascii_case(id))
                .ok_or_else(||format!("unknown culture: {id}; use :cultures"))?)
        };
        let changed=self.active!=choice;
        if changed {self.active=choice;}
        Ok(changed)
    }
    pub fn reproject(&mut self,coordinates:&[Option<Horizontal>],width:usize,height:usize,camera:Camera) {
        std::mem::swap(&mut self.previous,&mut self.current);
        self.current.lines.clear();
        self.current.names.clear();
        let Some(index)=self.active else{return;};
        for figure in &self.cultures[index].figures {
            let first_line=self.current.lines.len();
            let mut sx=0_i64; let mut sy=0_i64; let mut n=0_i64;
            for &(a,b) in &figure.pairs {
                let (Some(Some(a)),Some(Some(b)))=(coordinates.get(a),coordinates.get(b)) else {continue};
                if let Some(line)=project_segment(*a,*b,width,height,camera) {
                    sx+=(line.x0 as i64+line.x1 as i64)/2;
                    sy+=(line.y0 as i64+line.y1 as i64)/2;
                    n+=1;
                    self.current.lines.push(line);
                }
            }
            // No visible segments = no name, even when other figures in the culture are visible.
            if n>0 && !figure.name.trim().is_empty() {
                let name=figure.name.trim();
                // Compact single line; clip glyph text inside the native scene.
                let text:String=name.chars().take(36).collect();
                let width_px=text.chars().count() as i32*6;
                if width_px>0 && width_px<width as i32 {
                    let (cx,cy)=((sx/n) as i32,(sy/n) as i32);
                    let mut position=((cx+5).clamp(0,width as i32-width_px),(cy-11).clamp(0,height as i32-7));
                    // Prefer local non-overlap among previously placed names; never
                    // displace a name away from the projected figure or invent text.
                    for (ox,oy) in [(5,-11),(5,5),(-width_px-5,-11),(-width_px-5,5),
                        (5,-23),(5,17),(-width_px/2,-31),(-width_px/2,25)] {
                        let x=(cx+ox).clamp(0,width as i32-width_px);
                        let y=(cy+oy).clamp(0,height as i32-7);
                        if self.current.names.iter().all(|prior| {
                            x>=prior.x+prior.text.chars().count() as i32*6+2 ||
                            prior.x>=x+width_px+2 || y>=prior.y+9 || prior.y>=y+9
                        }) {position=(x,y);break;}
                    }
                    self.current.names.push(Name{text,x:position.0,y:position.1,
                        first_line,end_line:self.current.lines.len()});
                }
            }
        }
    }
}

// Camera-space unit direction. Projection uses exactly the star-map runtime camera
// basis/focal length; chords of a great circle project as straight line segments.
fn camera_space(h:Horizontal,camera:Camera)->Option<[f64;4]> {
    if !h.altitude.is_finite() || !h.azimuth.is_finite() {return None;}
    let (sa,ca)=h.altitude.sin_cos();
    let delta=h.azimuth-camera.azimuth_deg.to_radians();
    let (sd,cd)=delta.sin_cos();
    let (sc,cc)=camera.altitude_deg.to_radians().sin_cos();
    let north=ca*cd;
    Some([ca*sd,-north*sc+sa*cc,north*cc+sa*sc,sa]) // x,y,z,above-horizon
}

fn clip_positive(a:f64,b:f64,limit:f64,t0:&mut f64,t1:&mut f64)->bool {
    let d=b-a;
    if d.abs()<1e-15 {return a>=limit;}
    let crossing=(limit-a)/d;
    if d>0.0 {*t0=(*t0).max(crossing);} else {*t1=(*t1).min(crossing);}
    *t0<=*t1
}

/// Two stages: clip great-circle chord at geometric horizon and camera near-plane,
/// then clip its gnomonic (straight) projection against all viewport boundaries.
fn project_segment(a:Horizontal,b:Horizontal,width:usize,height:usize,camera:Camera)->Option<Line> {
    if width==0 || height==0 {return None;}
    let a=camera_space(a,camera)?;let b=camera_space(b,camera)?;
    let mut t0=0.0;let mut t1=1.0;
    if !clip_positive(a[3],b[3],0.0,&mut t0,&mut t1) ||
        !clip_positive(a[2],b[2],1e-5,&mut t0,&mut t1) {return None;}
    let focal=(width as f64*0.5)/(HORIZONTAL_FOV_DEG.to_radians()*0.5).tan();
    let to_xy=|t:f64| {
        let x=a[0]+(b[0]-a[0])*t;
        let y=a[1]+(b[1]-a[1])*t;
        let z=a[2]+(b[2]-a[2])*t;
        (width as f64*0.5+focal*x/z,height as f64*0.5-focal*y/z)
    };
    let (ax,ay)=to_xy(t0);let (bx,by)=to_xy(t1);
    if ![ax,ay,bx,by].iter().all(|v|v.is_finite()){return None;}
    let dx=bx-ax;let dy=by-ay;
    let mut u0:f64=0.0;let mut u1:f64=1.0;
    for (p,q) in [(-dx,ax),(dx,width as f64-1.0-ax),(-dy,ay),(dy,height as f64-1.0-ay)] {
        if p.abs()<1e-15 {if q<0.0 {return None;}continue;}
        let r=q/p;
        if p<0.0 {u0=u0.max(r);} else {u1=u1.min(r);}
        if u0>u1{return None;}
    }
    let point=|t:f64| ((ax+dx*t).round().clamp(0.0,width as f64-1.0) as i32,
                        (ay+dy*t).round().clamp(0.0,height as f64-1.0) as i32);
    let (x0,y0)=point(u0);let (x1,y1)=point(u1);
    Some(Line{x0,y0,x1,y1})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(a:f64,z:f64)->Horizontal {Horizontal{altitude:a.to_radians(),azimuth:z.to_radians()}}
    #[test] fn camera_matches_fixed_star_projection_at_endpoints() {
        let line=project_segment(h(30.0,0.0),h(30.0,10.0),960,600,Camera::default()).unwrap();
        let p=space_zoo::star_projection::project_alt_az(30f64.to_radians(),0.0,960,600).unwrap();
        assert_eq!((line.x0,line.y0),(p.x,p.y));
        let q=space_zoo::star_projection::project_alt_az(30f64.to_radians(),10f64.to_radians(),960,600).unwrap();
        assert!((line.x1-q.x).abs()<=1 && (line.y1-q.y).abs()<=1);
    }
    #[test] fn moved_camera_matches_star_projection_at_endpoints() {
        let camera=Camera{altitude_deg:40.0,azimuth_deg:25.0};
        let line=project_segment(h(40.0,25.0),h(40.0,35.0),960,600,camera).unwrap();
        let p=space_zoo::star_projection::project_alt_az_with_camera(40f64.to_radians(),25f64.to_radians(),960,600,camera).unwrap();
        let q=space_zoo::star_projection::project_alt_az_with_camera(40f64.to_radians(),35f64.to_radians(),960,600,camera).unwrap();
        assert_eq!((line.x0,line.y0),(p.x,p.y));
        assert!((line.x1-q.x).abs()<=1 && (line.y1-q.y).abs()<=1);
    }
    #[test] fn horizon_and_viewport_clip_without_spurious_bridges() {
        assert!(project_segment(h(-20.0,0.0),h(-10.0,0.0),960,600,Camera::default()).is_none());
        let line=project_segment(h(-20.0,0.0),h(50.0,0.0),960,600,Camera::default()).unwrap();
        for p in [line.x0,line.x1] {assert!((0..960).contains(&p));}
        for p in [line.y0,line.y1] {assert!((0..600).contains(&p));}
        assert!(project_segment(h(30.0,180.0),h(40.0,180.0),960,600,Camera::default()).is_none());
        assert!(project_segment(h(f64::NAN,0.0),h(30.0,0.0),960,600,Camera::default()).is_none());
    }
    #[test] fn culture_commands_preserve_selection_and_audited_gaps() {
        let data=CultureDataset::load_dir(std::path::Path::new("data/cultures/stellarium-v26.3")).unwrap();
        let mut selected=HashMap::new();
        for seg in &data.segments {
            if seg.selection_complete {
                for endpoint in [&seg.a,&seg.b] {
                    let id=endpoint.matches[0].tycho_id;
                    let next=selected.len();selected.entry(id).or_insert(next);
                }
            }
        }
        let mut layer=Layer::new(&data,&selected).unwrap();
        assert_eq!(layer.selected(),"modern_iau");
        assert!(!layer.debugstars());
        assert_eq!(layer.available().len(),42);
        let total:usize=layer.cultures.iter().flat_map(|c|&c.figures).map(|f|f.pairs.len()).sum();
        assert_eq!(total,16681); // only audited render-eligible pairs, gaps never bridged
        assert!(layer.set_culture("not_a_culture").is_err());
        assert_eq!(layer.selected(),"modern_iau");
        assert!(layer.set_culture("off").unwrap());
        assert_eq!(layer.selected(),"off");
        assert!(layer.set_culture("chinese").unwrap());
        assert_eq!(layer.selected(),"chinese");
        assert!(layer.set_debugstars(true));
        assert_eq!(layer.selected(),"chinese");
        assert!(layer.set_debugstars(false));
        assert!(!layer.debugstars());
        // Offline synthetic observation: same input ray reused across all cultures.
        let points=vec![Some(h(30.0,0.0));selected.len()];
        layer.reproject(&points,960,600,Camera::default());
        assert!(!layer.current.lines.is_empty());
        assert!(layer.current.names.iter().all(|n|!n.text.is_empty()));
        layer.set_culture("off").unwrap();
        layer.reproject(&points,960,600,Camera::default());
        assert!(layer.current.lines.is_empty() && layer.current.names.is_empty());
        assert!(!layer.previous.lines.is_empty()); // required for clean-damage invalidation
    }
}
