//! Rev 0.49.02: preserve the two phase-lit photographic atlases, with a
//! cached daylight matte derived from their existing illumination.
//!
//! Normal mode uses NASA SVS-derived 64-frame lunar artwork, and --gm
//! substitutes the original Space Zoo interpretation of Georges Melies'
//! Le Voyage dans la Lune (1902). Both are baked OFFLINE from the supplied
//! art-study HTML: there is no runtime WebP decoding, disk read, shader,
//! image-library dependency, or new heap allocation in the render loop.
//!
//! Grayscale 144px cells save space without losing the lunar surface relief;
//! a light color grade restores the moonstone palette. Two neighboring
//! frames are interpolated at the legacy 4096-step lunar cadence.
use crate::pixel::{Canvas, ART_SCALE};

const CELL: usize = 144;
const FRAMES: usize = 64;
const FRAME_LEN: usize = CELL * CELL;
const ATLAS_LEN: usize = FRAMES * FRAME_LEN;
const NASA: &[u8; ATLAS_LEN] = include_bytes!("../assets/luna/nasa_64x144.l8");
const MELIES: &[u8; ATLAS_LEN] = include_bytes!("../assets/luna/melies_64x144.l8");
const RADIUS: f32 = 67.0;
const EXTENT: i32 = 94; // preserve original foreground-restoration footprint
const SIDE: usize = (2 * EXTENT + 1) as usize;
const PHASE_STEPS: u32 = 4096;

/// Existing foreground occlusion logic is deliberately unchanged.
pub const OCCLUSION_RADIUS: i32 = (EXTENT + ART_SCALE as i32 - 1) / ART_SCALE as i32;

/// Convert astronomical phase to the same 4096 (~10.4-min) image steps as
/// the previous version. The dirty-tile tracker uses this exact key too.
#[inline]
pub fn phase_key(phase: f32) -> u16 {
    ((phase.rem_euclid(1.0) * PHASE_STEPS as f32).round() as u32 % PHASE_STEPS) as u16
}

#[inline]
fn mix_pixel(bg: u32, fg: u32, alpha: u32) -> u32 {
    let inv = 255 - alpha;
    let r = (((bg >> 16) & 255) * inv + ((fg >> 16) & 255) * alpha + 127) / 255;
    let g = (((bg >> 8) & 255) * inv + ((fg >> 8) & 255) * alpha + 127) / 255;
    let b = ((bg & 255) * inv + (fg & 255) * alpha + 127) / 255;
    (r << 16) | (g << 8) | b
}

/// For dark NASA earthshine, keep the slight blue stone tint of the review;
/// bright terrain in both versions stays near-neutral / gently warm.
#[inline]
fn tint(value: u8, melies: bool) -> u32 {
    let v = value as u32;
    let (r, g, b) = if !melies && v < 38 {
        (v * 88 / 100, v, (v * 116 / 100).min(255))
    } else {
        ((v * 103 / 100).min(255), v, v * 97 / 100)
    };
    (r << 16) | (g << 8) | b
}

// Prepared from the SAME photographic frame as the night sprite. The daylight
// mask is based on the relative illumination of each small surface patch, not
// on absolute darkness (which would erase real maria, facial shadows and the
// rocket). The reference at phase zero contains NASA's blue earthshine; phase
// 32 supplies a fully lit reference at identical native atlas coordinates.
// A neighborhood maximum lets genuinely dark photographic details inherit
// the illumination of their surroundings without painting an opaque dark disk.
fn patch_max(atlas:&[u8; ATLAS_LEN],frame:usize,x:usize,y:usize)->u8 {
    let left=x.saturating_sub(3);
    let right=(x+3).min(CELL-1);
    let top=y.saturating_sub(3);
    let bottom=(y+3).min(CELL-1);
    let mut brightest=0;
    for yy in top..=bottom {
        for xx in left..=right {
            brightest=brightest.max(atlas[frame*FRAME_LEN+yy*CELL+xx]);
        }
    }
    brightest
}

#[inline]
fn daylight_coverage(current:u8, earthshine:u8, full:u8)->u8 {
    // A small offset absorbs atlas quantization and changing earthshine.
    // Normalize by local full-moon relief so dark craters/rocket shadows
    // retain the SAME coverage as adjacent illuminated surface pixels.
    let range=full.saturating_sub(earthshine).max(24) as f32;
    let ratio=(current.saturating_sub(earthshine) as f32 / range - 0.06)
        / 0.22;
    let x=ratio.clamp(0.0,1.0);
    (255.0 * x*x*(3.0-2.0*x)).round() as u8
}

#[inline]
fn daylight_tint(gray:u8,melies:bool)->u32 {
    // Atmospheric haze makes NASA terrain pale without flattening maria;
    // the more contrasty Méliès palette keeps the face and dark rocket legible.
    let v=gray as u32;
    let level=if melies {145+(v*105+127)/255} else {170+(v*80+127)/255};
    (level<<16)|(level<<8)|level
}

pub struct MoonSprite {
    rgb: Vec<u32>,
    alpha: Vec<u8>,
    day_rgb: Vec<u32>,
    day_alpha: Vec<u8>,
    phase_step: Option<u16>,
    melies: bool,
}

impl MoonSprite {
    pub fn new() -> Self { Self::with_melies(false) }

    pub fn with_melies(melies: bool) -> Self {
        Self { rgb: vec![0; SIDE * SIDE], alpha: vec![0; SIDE * SIDE],
            day_rgb: vec![0; SIDE * SIDE], day_alpha: vec![0; SIDE * SIDE],
            phase_step: None, melies }
    }

    /// Only called on startup when --gm is set, never as a per-frame switch.
    pub fn set_melies(&mut self, melies: bool) {
        if self.melies != melies {
            self.melies = melies;
            self.phase_step = None;
        }
    }

    fn prepare(&mut self, phase: f32) {
        let key = phase_key(phase);
        if self.phase_step == Some(key) { return; }
        self.phase_step = Some(key);
        let atlas = if self.melies { MELIES } else { NASA };
        let f0 = (key as usize / 64) * FRAME_LEN;
        let f1 = ((key as usize / 64 + 1) % FRAMES) * FRAME_LEN;
        let mix = key as usize % 64;
        let lit_fraction = 0.5 * (1.0 - (std::f32::consts::TAU *
            key as f32 / PHASE_STEPS as f32).cos());
        let bloom = 0.16 * lit_fraction.sqrt();
        for py in 0..SIDE {
            let dy = py as i32 - EXTENT;
            for px in 0..SIDE {
                let dx = px as i32 - EXTENT;
                let idx = py * SIDE + px;
                let distance = ((dx * dx + dy * dy) as f32).sqrt();
                // The source is a centered 144px square, inside the old 189px
                // halo footprint. A few pixels beyond the sphere's rim may
                // contain the Méliès rocket: do not clip them to a circle.
                let sx = dx + CELL as i32 / 2;
                let sy = dy + CELL as i32 / 2;
                let gray = if sx >= 0 && sy >= 0 && sx < CELL as i32 && sy < CELL as i32 {
                    let sub = sy as usize * CELL + sx as usize;
                    ((atlas[f0 + sub] as usize * (64 - mix) +
                      atlas[f1 + sub] as usize * mix + 32) / 64) as u8
                } else { 0 };
                let in_sphere=distance <= RADIUS+0.5;
                if in_sphere {
                    self.rgb[idx] = tint(gray, self.melies);
                    self.alpha[idx] = (255.0 * (RADIUS + 0.5 - distance)
                        .clamp(0.0, 1.0)).round() as u8;
                } else if gray >= 5 && self.melies && distance < CELL as f32 * 0.71 {
                    // Preserve protruding rocket highlights at the lunar limb.
                    self.rgb[idx] = tint(gray, true);
                    self.alpha[idx] = (gray as u32 * 12).min(255) as u8;
                } else if distance < EXTENT as f32 {
                    // Very weak photographic lens bloom, not a solid ring.
                    let spread = ((distance - RADIUS) / (EXTENT as f32 - RADIUS))
                        .clamp(0.0, 1.0);
                    self.rgb[idx] = 0xd7d9dc;
                    self.alpha[idx] = (255.0 * bloom * (1.0 - spread).powi(3))
                        .round() as u8;
                } else {
                    self.rgb[idx] = 0;
                    self.alpha[idx] = 0;
                }
                // Only the retained baked frame determines the daylight matte.
                // No independent phase engine or runtime pixel classification.
                self.day_rgb[idx] = daylight_tint(gray,self.melies);
                self.day_alpha[idx] = if in_sphere && self.alpha[idx]!=0 {
                    let f_a=key as usize/64;
                    let f_b=(f_a+1)%FRAMES;
                    let current=patch_max(atlas,f_a,sx as usize,sy as usize);
                    let following=patch_max(atlas,f_b,sx as usize,sy as usize);
                    let lit=((current as usize*(64-mix)+following as usize*mix+32)/64) as u8;
                    let earthshine=patch_max(atlas,0,sx as usize,sy as usize);
                    let full=patch_max(atlas,FRAMES/2,sx as usize,sy as usize);
                    let mask=daylight_coverage(lit,earthshine,full) as u32;
                    let feather=if self.melies {
                        ((RADIUS-distance)/3.0).clamp(0.0,1.0)
                    } else {1.0};
                    (self.alpha[idx] as f32 * mask as f32 * (215.0/65025.0)
                        * feather).round() as u8
                } else if self.melies && !in_sphere && self.alpha[idx]!=0 {
                    // The separately silhouetted Méliès rocket projects past
                    // the limb; keep its *existing* occlusion, never a dark ring.
                    (self.alpha[idx] as u32 * 215 / 255) as u8
                } else {0}; // Night-only bloom is completely absent by day.
            }
        }
    }

    pub fn draw(&mut self, canvas: &mut Canvas, x: i32, y: i32, phase: f32) {
        self.draw_with_sun(canvas,x,y,phase,0.0,None);
    }

    /// `daylight` is the existing solar sky opacity, from the same simulated
    /// UTC clock as the Moon. The Sun mask protects exactly those native pixels
    /// it already painted; this retains both its opaque coin and soft corona.
    pub fn draw_with_sun(&mut self, canvas: &mut Canvas, x: i32, y: i32,
            phase: f32, daylight: f32,
            sun: Option<(&crate::solar::SunSprite,i32,i32,u64)>) {
        let stride = canvas.w * canvas.scale;
        let height = canvas.h * canvas.scale;
        let left = x * canvas.scale as i32 - EXTENT;
        let top = y * canvas.scale as i32 - EXTENT;
        let x0 = (-left).max(0) as usize;
        let x1 = (stride as i32 - left).min(SIDE as i32).max(0) as usize;
        let y0 = (-top).max(0) as usize;
        let y1 = (height as i32 - top).min(SIDE as i32).max(0) as usize;
        if x0 >= x1 || y0 >= y1 || !canvas.overlaps(left, top, SIDE as i32, SIDE as i32) {
            return;
        }
        self.prepare(phase);
        let day_mix=(daylight.clamp(0.0,1.0)*255.0).round() as u32;
        for row in y0..y1 {
            let src_row = row * SIDE;
            let dst_row = (top + row as i32) as usize * stride;
            for col in x0..x1 {
                let src = src_row + col;
                let nx=left+col as i32;
                let ny=top+row as i32;
                if !canvas.touched(nx as usize,ny as usize) {continue;}
                // Sun was rendered first. Discard only lunar samples that would
                // overwrite an actual Sun pixel, not its empty bounding square.
                if sun.is_some_and(|(sprite,sx,sy,tick)|
                    sprite.protects_native_pixel(sx,sy,nx,ny,tick)) {continue;}
                let night_a=self.alpha[src] as u32;
                let a=if day_mix==0 {night_a}
                    else if day_mix==255 {self.day_alpha[src] as u32}
                    else {(night_a*(255-day_mix)+self.day_alpha[src] as u32*day_mix+127)/255};
                if a==0 {continue;}
                let rgb=if day_mix==0 {self.rgb[src]}
                    else if day_mix==255 {self.day_rgb[src]}
                    else {mix_pixel(self.rgb[src],self.day_rgb[src],day_mix)};
                let dst=dst_row+nx as usize;
                canvas.cells[dst]=if a==255 {rgb}
                    else {mix_pixel(canvas.cells[dst],rgb,a)};
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_64_frame_artwork_atlases_embedded() {
        assert_eq!(NASA.len(), 64 * 144 * 144);
        assert_eq!(MELIES.len(), NASA.len());
        assert_ne!(&NASA[32 * FRAME_LEN..33 * FRAME_LEN],
                   &MELIES[32 * FRAME_LEN..33 * FRAME_LEN]);
    }
    #[test]
    fn lunar_phase_order_and_wrap() {
        assert_eq!(phase_key(0.0), 0);
        assert_eq!(phase_key(0.25), 1024);
        assert_eq!(phase_key(0.5), 2048);
        assert_eq!(phase_key(0.75), 3072);
        assert_eq!(phase_key(1.0), 0);
    }
    #[test]
    fn nasa_full_lit_and_melies_distinct() {
        let mut nasa = MoonSprite::new();
        let center = EXTENT as usize * SIDE + EXTENT as usize;
        nasa.prepare(0.0);
        let new = nasa.rgb[center] & 255;
        nasa.prepare(0.5);
        assert!((nasa.rgb[center] & 255) > new + 75);
        let mut cinema = MoonSprite::with_melies(true);
        cinema.prepare(0.5);
        let differing = nasa.rgb.iter().zip(&cinema.rgb).filter(|(a,b)|a != b).count();
        assert!(differing > 1000, "optional Moon must have distinct art");
    }
    #[test]
    fn daytime_new_moon_disappears_without_hiding_the_nighttime_earthshine() {
        let mut moon=MoonSprite::new();
        moon.prepare(0.0);
        let center=EXTENT as usize*SIDE+EXTENT as usize;
        assert!(moon.alpha[center]>0);
        assert!(moon.rgb[center]!=0, "original NASA earthshine remains at night");
        assert_eq!(moon.day_alpha[center],0);
        assert!(moon.day_alpha.iter().all(|&v|v==0),
            "new Moon must not become a daytime black disk or halo");
    }

    #[test]
    fn eight_phases_have_a_lit_daytime_mask_but_no_shadow_disk() {
        for gm in [false,true] {
            let mut moon=MoonSprite::with_melies(gm);
            for phase in [0.0,0.125,0.25,0.375,0.5,0.625,0.75,0.875] {
                moon.prepare(phase);
                let center=EXTENT as usize*SIDE+EXTENT as usize;
                let right=EXTENT as usize*SIDE+EXTENT as usize+35;
                let left=EXTENT as usize*SIDE+EXTENT as usize-35;
                if phase==0.0 {
                    assert_eq!(moon.day_alpha[center],0);
                } else {
                    assert!(moon.day_alpha.iter().any(|&v|v>80),
                        "phase {phase}, melies {gm} should illuminate daylight");
                }
                if phase==0.25 {
                    assert!(moon.day_alpha[right]>moon.day_alpha[left]+100,
                        "first quarter right lit; shadow blends into sky");
                }
                if phase==0.75 {
                    assert!(moon.day_alpha[left]>moon.day_alpha[right]+100,
                        "last quarter left lit; shadow blends into sky");
                }
                if phase==0.5 {
                    assert!(moon.day_alpha[center]>150,
                        "full Moon preserves illuminated center");
                }
            }
        }
    }

    #[test]
    fn daylight_preserves_lunar_relief_and_melies_rocket() {
        for gm in [false,true] {
            let mut moon=MoonSprite::with_melies(gm);
            moon.prepare(0.5);
            let mut different=std::collections::HashSet::new();
            for y in EXTENT as usize-42..EXTENT as usize+42 {
                for x in EXTENT as usize-42..EXTENT as usize+42 {
                    let i=y*SIDE+x;
                    if moon.day_alpha[i]>170 {
                        different.insert(moon.day_rgb[i]);
                    }
                }
            }
            assert!(different.len()>25,
                "photographic craters, maria and face must not become a flat disk");
            if gm {
                // Actual baked rocket pixels protrude past the lunar circle.
                let projected=(0..SIDE*SIDE).filter(|&i| {
                    let x=(i%SIDE) as i32-EXTENT;
                    let y=(i/SIDE) as i32-EXTENT;
                    x*x+y*y>68*68 && moon.alpha[i]>0 && moon.day_alpha[i]>0
                }).count();
                assert!(projected>0,"Méliès rocket must remain visible beyond limb");
            }
        }
    }

    #[test]
    fn twilight_lerp_preserves_original_night_pixel_and_reaches_day_pixel() {
        let mut moon=MoonSprite::new();
        let mut night=Canvas::scaled(320,200,ART_SCALE,false);
        night.cells.fill(0x7ab9e9);
        moon.draw(&mut night,100,50,0.25);
        let mut zero=Canvas::scaled(320,200,ART_SCALE,false);
        zero.cells.fill(0x7ab9e9);
        moon.draw_with_sun(&mut zero,100,50,0.25,0.0,None);
        assert_eq!(night.cells,zero.cells,"night path is unchanged");
        let mut day=Canvas::scaled(320,200,ART_SCALE,false);
        day.cells.fill(0x7ab9e9);
        moon.draw_with_sun(&mut day,100,50,0.25,1.0,None);
        let mut twilight=Canvas::scaled(320,200,ART_SCALE,false);
        twilight.cells.fill(0x7ab9e9);
        moon.draw_with_sun(&mut twilight,100,50,0.25,0.5,None);
        // Inspect a fully shadowed part of the quarter Moon.
        let px=100*ART_SCALE-35;
        let py=50*ART_SCALE;
        let i=py*320*ART_SCALE+px;
        assert_eq!(day.cells[i],0x7ab9e9,"shadow should be sky, not dark disk");
        assert_ne!(night.cells[i],day.cells[i]);
        assert_ne!(twilight.cells[i],day.cells[i]);
        assert_ne!(twilight.cells[i],night.cells[i]);
    }

    #[test]
    fn sun_artwork_is_bit_identical_through_lunar_overlap() {
        for gm in [false,true] {
            for tick in [0,15,42,120] {
                let mut scene=Canvas::scaled(320,200,ART_SCALE,false);
                scene.cells.fill(0x7ab9e9);
                let mut sun=crate::solar::SunSprite::new();
                sun.draw(&mut scene,100,55,tick);
                let without_moon=scene.cells.clone();
                let mut moon=MoonSprite::with_melies(gm);
                moon.draw_with_sun(&mut scene,133,55,0.5,1.0,
                    Some((&sun,100,55,tick)));
                let mut protected=0;
                let mut exposed=0;
                for y in 0..200*ART_SCALE {
                    for x in 0..320*ART_SCALE {
                        let i=y*320*ART_SCALE+x;
                        if sun.protects_native_pixel(100,55,x as i32,y as i32,tick) {
                            assert_eq!(scene.cells[i],without_moon[i],
                                "Sun must be perfectly unaffected by Moon overlay");
                            protected+=1;
                        } else if scene.cells[i]!=without_moon[i] {
                            exposed+=1;
                        }
                    }
                }
                assert!(protected>1000);
                assert!(exposed>100,"uncovered Moon remains visible");
            }
        }
    }

    #[test]
    fn moon_clips_cleanly_on_all_backends() {
        let mut canvas = Canvas::scaled(320, 200, ART_SCALE, false);
        let mut moon = MoonSprite::new();
        moon.draw(&mut canvas, -10, 5, 0.4);
        moon.draw(&mut canvas, 330, 205, 0.6);
        moon.set_melies(true);
        moon.draw(&mut canvas, 145, 62, 0.5);
        assert_eq!(canvas.cells.len(), 960 * 600);
    }
}
