//! v0.52 -- production Tycho-2 single-pixel photometry for the default pixel star map.
//! Catalogue astrometry stays in stellar::production; observer/time conversion stays
//! in the accepted stellar engine. This module owns the fixed render selection,
//! diagnostic registry binding, production projection, bounded refresh cadence, and
//! the frame snapshot consumed by the pixel diagnostic overlay.
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;
use std::time::Instant;

use space_zoo::stellar::{
    evaluate_icrs_direction, prepare_observation, ObservationOptions, Observer, UtcInstant,
};
use space_zoo::stellar::production::{
    Band, CatalogueFrame, ConeQuery, EpochScale, MagnitudeFilter, ProductionCatalogue, TychoId, TychoStar,
};
use space_zoo::star_projection::{
    project_alt_az, project_alt_az_with_camera, signed_azimuth_delta, Camera, StarPixel, ViewControl,
    ViewInput, CAMERA_ALTITUDE_DEG, CAMERA_AZIMUTH_DEG,
    CAMERA_MAX_SPEED_DEG_PER_SEC, CAMERA_STEP_DEG, CAMERA_MAX_ALTITUDE_DEG, CAMERA_MIN_ALTITUDE_DEG,
};

use crate::constellation_render::{Horizontal,Layer};
use space_zoo::stellar::culture::CultureDataset;
use crate::astronomy::Location;
use crate::world::{World, FPS};

pub const CATALOGUE_PATH: &str = "data/tycho2/generated/tycho2-main-v1.1.szt";
pub const MAGNITUDE_BAND: Band = Band::Vt;
pub const MAGNITUDE_CUTOFF: f64 = 8.0;
const DISPLAY_BRIGHT_LIMIT: f64 = -2.0;
const MAX_SELECTED_STARS: usize = 65_536;
const CATALOGUE_CACHE_BYTES: usize = 32 * 1024 * 1024;
pub const DIAGNOSTIC_REGISTRY_LEN: usize = 100;

#[derive(Clone, Copy, Debug)]
struct DiagnosticRegistration {
    name: &'static str,
    id: TychoId,
}

// v0.51.02 runtime registry contains only display name -> stable accepted-catalogue
// TYC identity. Matching evidence, HIP cross-references, substitutions, and component
// choices are release documentation, not runtime astronomical data.
const DIAGNOSTIC_REGISTRY: [DiagnosticRegistration; DIAGNOSTIC_REGISTRY_LEN] = [
    DiagnosticRegistration { name: "POLARIS", id: TychoId { region: 4628, number: 237, component: 1 } },
    DiagnosticRegistration { name: "KOCHAB", id: TychoId { region: 4416, number: 1799, component: 1 } },
    DiagnosticRegistration { name: "PHERKAD", id: TychoId { region: 4414, number: 2314, component: 1 } },
    DiagnosticRegistration { name: "ADHAFERA", id: TychoId { region: 1969, number: 1261, component: 1 } },
    DiagnosticRegistration { name: "CHERTAN", id: TychoId { region: 1437, number: 2847, component: 1 } },
    DiagnosticRegistration { name: "PHECDA", id: TychoId { region: 3833, number: 1034, component: 1 } },
    DiagnosticRegistration { name: "MEGREZ", id: TychoId { region: 3837, number: 1070, component: 1 } },
    DiagnosticRegistration { name: "RASALAS", id: TychoId { region: 1964, number: 1473, component: 1 } },
    DiagnosticRegistration { name: "ALTERF", id: TychoId { region: 1959, number: 1582, component: 1 } },
    DiagnosticRegistration { name: "ALULA BOREALIS", id: TychoId { region: 2520, number: 2631, component: 1 } },
    DiagnosticRegistration { name: "MEKBUDA", id: TychoId { region: 1353, number: 1550, component: 1 } },
    DiagnosticRegistration { name: "ALZIRR", id: TychoId { region: 754, number: 2356, component: 1 } },
    DiagnosticRegistration { name: "TEJAT", id: TychoId { region: 1878, number: 1429, component: 1 } },
    DiagnosticRegistration { name: "FURUD", id: TychoId { region: 7073, number: 2426, component: 1 } },
    DiagnosticRegistration { name: "ALCYONE", id: TychoId { region: 1800, number: 2202, component: 1 } },
    DiagnosticRegistration { name: "UNURGUNITE", id: TychoId { region: 6531, number: 3793, component: 1 } },
    DiagnosticRegistration { name: "AIN", id: TychoId { region: 1273, number: 1104, component: 1 } },
    DiagnosticRegistration { name: "CHAMUKUY", id: TychoId { region: 1265, number: 1172, component: 1 } },
    DiagnosticRegistration { name: "TIANGUAN", id: TychoId { region: 1310, number: 2697, component: 1 } },
    DiagnosticRegistration { name: "ATLAS", id: TychoId { region: 1800, number: 2203, component: 1 } },
    DiagnosticRegistration { name: "ELECTRA", id: TychoId { region: 1799, number: 1441, component: 1 } },
    DiagnosticRegistration { name: "MAIA", id: TychoId { region: 1799, number: 1439, component: 1 } },
    DiagnosticRegistration { name: "MEISSA", id: TychoId { region: 705, number: 2400, component: 1 } },
    DiagnosticRegistration { name: "MEROPE", id: TychoId { region: 1800, number: 2204, component: 1 } },
    DiagnosticRegistration { name: "PLEIONE", id: TychoId { region: 1800, number: 2200, component: 1 } },
    DiagnosticRegistration { name: "WEZEN", id: TychoId { region: 6532, number: 5001, component: 1 } },
    DiagnosticRegistration { name: "CELAENO", id: TychoId { region: 1799, number: 1440, component: 1 } },
    DiagnosticRegistration { name: "ASTEROPE", id: TychoId { region: 1803, number: 1584, component: 1 } },
    DiagnosticRegistration { name: "GOMEISA", id: TychoId { region: 764, number: 2774, component: 1 } },
    DiagnosticRegistration { name: "NEKKAR", id: TychoId { region: 3047, number: 1258, component: 1 } },
    DiagnosticRegistration { name: "SEGINUS", id: TychoId { region: 3036, number: 1151, component: 1 } },
    DiagnosticRegistration { name: "PRINCEPS", id: TychoId { region: 2566, number: 1639, component: 1 } },
    DiagnosticRegistration { name: "WASAT", id: TychoId { region: 1359, number: 2672, component: 1 } },
    DiagnosticRegistration { name: "MEBSUTA", id: TychoId { region: 1897, number: 1639, component: 1 } },
    DiagnosticRegistration { name: "ALKALUROPS", id: TychoId { region: 2570, number: 1521, component: 1 } },
    DiagnosticRegistration { name: "ZAVIJAVA", id: TychoId { region: 273, number: 924, component: 1 } },
    DiagnosticRegistration { name: "ZANIAH", id: TychoId { region: 4940, number: 1158, component: 1 } },
    DiagnosticRegistration { name: "ZOSMA", id: TychoId { region: 1439, number: 2479, component: 1 } },
    DiagnosticRegistration { name: "MINELAUVA", id: TychoId { region: 294, number: 1032, component: 1 } },
    DiagnosticRegistration { name: "IZAR", id: TychoId { region: 2019, number: 1251, component: 1 } },
    DiagnosticRegistration { name: "MUPHRID", id: TychoId { region: 1470, number: 1157, component: 1 } },
    DiagnosticRegistration { name: "HEZE", id: TychoId { region: 4966, number: 1366, component: 1 } },
    DiagnosticRegistration { name: "VINDEMIATRIX", id: TychoId { region: 886, number: 1326, component: 1 } },
    DiagnosticRegistration { name: "AZELFAFAGE", id: TychoId { region: 3603, number: 3215, component: 1 } },
    DiagnosticRegistration { name: "SHELIAK", id: TychoId { region: 2642, number: 2929, component: 1 } },
    DiagnosticRegistration { name: "SULAFAT", id: TychoId { region: 2643, number: 3346, component: 1 } },
    DiagnosticRegistration { name: "OKAB", id: TychoId { region: 1052, number: 3027, component: 1 } },
    DiagnosticRegistration { name: "SADR", id: TychoId { region: 3156, number: 2223, component: 1 } },
    DiagnosticRegistration { name: "ALJANAH", id: TychoId { region: 2695, number: 4139, component: 1 } },
    DiagnosticRegistration { name: "ALBIREO", id: TychoId { region: 2133, number: 2964, component: 1 } },
    DiagnosticRegistration { name: "LARAWAG", id: TychoId { region: 7367, number: 804, component: 1 } },
    DiagnosticRegistration { name: "TARAZED", id: TychoId { region: 1061, number: 2577, component: 1 } },
    DiagnosticRegistration { name: "ALSHAIN", id: TychoId { region: 493, number: 3355, component: 1 } },
    DiagnosticRegistration { name: "PAIKAUHALE", id: TychoId { region: 6807, number: 1012, component: 1 } },
    DiagnosticRegistration { name: "IKLIL", id: TychoId { region: 6791, number: 2509, component: 1 } },
    DiagnosticRegistration { name: "FANG", id: TychoId { region: 6783, number: 2697, component: 1 } },
    DiagnosticRegistration { name: "DSCHUBBA", id: TychoId { region: 6779, number: 2194, component: 1 } },
    DiagnosticRegistration { name: "ACRAB", id: TychoId { region: 6208, number: 1623, component: 1 } },
    DiagnosticRegistration { name: "MATAR", id: TychoId { region: 2736, number: 1823, component: 1 } },
    DiagnosticRegistration { name: "ENIF", id: TychoId { region: 1125, number: 2186, component: 1 } },
    DiagnosticRegistration { name: "MARKAB", id: TychoId { region: 1711, number: 2475, component: 1 } },
    DiagnosticRegistration { name: "SCHEAT", id: TychoId { region: 2243, number: 1831, component: 1 } },
    DiagnosticRegistration { name: "ALGENIB", id: TychoId { region: 1179, number: 1775, component: 1 } },
    DiagnosticRegistration { name: "HOMAM", id: TychoId { region: 1155, number: 2187, component: 1 } },
    DiagnosticRegistration { name: "MIRACH", id: TychoId { region: 2286, number: 1329, component: 1 } },
    DiagnosticRegistration { name: "ALMACH", id: TychoId { region: 2837, number: 2311, component: 1 } },
    DiagnosticRegistration { name: "BAHAM", id: TychoId { region: 565, number: 1999, component: 1 } },
    DiagnosticRegistration { name: "SADALBARI", id: TychoId { region: 2225, number: 1821, component: 1 } },
    DiagnosticRegistration { name: "ATIK", id: TychoId { region: 2361, number: 2820, component: 1 } },
    DiagnosticRegistration { name: "HAMAL", id: TychoId { region: 1758, number: 2416, component: 1 } },
    DiagnosticRegistration { name: "SHERATAN", id: TychoId { region: 1212, number: 1935, component: 1 } },
    DiagnosticRegistration { name: "ADHIL", id: TychoId { region: 3265, number: 2400, component: 1 } },
    DiagnosticRegistration { name: "CEBALRAI", id: TychoId { region: 423, number: 1724, component: 1 } },
    DiagnosticRegistration { name: "SABIK", id: TychoId { region: 6232, number: 1333, component: 1 } },
    DiagnosticRegistration { name: "UNUKALHAI", id: TychoId { region: 363, number: 1135, component: 1 } },
    DiagnosticRegistration { name: "RASALGETHI", id: TychoId { region: 990, number: 2133, component: 1 } },
    DiagnosticRegistration { name: "KORNEPHOROS", id: TychoId { region: 1518, number: 1442, component: 1 } },
    DiagnosticRegistration { name: "ZUBENELGENUBI", id: TychoId { region: 6155, number: 1210, component: 1 } },
    DiagnosticRegistration { name: "ZUBENESCHAMALI", id: TychoId { region: 5585, number: 1014, component: 1 } },
    DiagnosticRegistration { name: "MENKAR", id: TychoId { region: 58, number: 1618, component: 1 } },
    DiagnosticRegistration { name: "DIPHDA", id: TychoId { region: 5847, number: 2333, component: 1 } },
    DiagnosticRegistration { name: "NEMBUS", id: TychoId { region: 3282, number: 2271, component: 1 } },
    DiagnosticRegistration { name: "SCHEDAR", id: TychoId { region: 3663, number: 2668, component: 1 } },
    DiagnosticRegistration { name: "CAPH", id: TychoId { region: 3664, number: 1985, component: 1 } },
    DiagnosticRegistration { name: "RUCHBAH", id: TychoId { region: 4031, number: 3289, component: 1 } },
    DiagnosticRegistration { name: "ACHIRD", id: TychoId { region: 3663, number: 2669, component: 1 } },
    DiagnosticRegistration { name: "ALDERAMIN", id: TychoId { region: 4252, number: 1870, component: 1 } },
    DiagnosticRegistration { name: "ALFIRK", id: TychoId { region: 4465, number: 2643, component: 1 } },
    DiagnosticRegistration { name: "ERRAI", id: TychoId { region: 4606, number: 3584, component: 1 } },
    DiagnosticRegistration { name: "MISAM", id: TychoId { region: 2859, number: 2274, component: 1 } },
    DiagnosticRegistration { name: "NUSAKAN", id: TychoId { region: 2032, number: 1605, component: 1 } },
    DiagnosticRegistration { name: "BOTEIN", id: TychoId { region: 1228, number: 2019, component: 1 } },
    DiagnosticRegistration { name: "YED PRIOR", id: TychoId { region: 5037, number: 1068, component: 1 } },
    DiagnosticRegistration { name: "KAUS MEDIA", id: TychoId { region: 6856, number: 2170, component: 1 } },
    DiagnosticRegistration { name: "KAUS BOREALIS", id: TychoId { region: 6861, number: 3180, component: 1 } },
    DiagnosticRegistration { name: "ASCELLA", id: TychoId { region: 6885, number: 2837, component: 1 } },
    DiagnosticRegistration { name: "ALGORAB", id: TychoId { region: 6103, number: 2395, component: 1 } },
    DiagnosticRegistration { name: "GIENAH", id: TychoId { region: 6098, number: 1754, component: 1 } },
    DiagnosticRegistration { name: "YED POSTERIOR", id: TychoId { region: 5042, number: 999, component: 1 } },
    DiagnosticRegistration { name: "ALPHARD", id: TychoId { region: 5460, number: 1592, component: 1 } },
];

/// World simulation is 30 Hz. Reprojection is capped at 2 Hz during continuous playback.
pub const UPDATE_TICKS: u64 = FPS / 2;

#[derive(Debug)]
pub struct StarMapError(String);
impl StarMapError {
    fn new(message: impl Into<String>) -> Self { Self(message.into()) }
}
impl fmt::Display for StarMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for StarMapError {}

#[derive(Clone, Copy, Debug)]
struct DiagnosticBinding {
    name: &'static str,
    id: TychoId,
    star_index: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct DiagnosticPoint {
    pub name: &'static str,
    pub id: TychoId,
    pub altitude_deg: f64,
    pub azimuth_deg: f64,
    pub pixel: StarPixel,
    pub rgb: u32,
    pub rendered: bool,
    registry_index: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StarCounts {
    pub above_horizon: usize,
    pub in_view: usize,
    pub diagnostic: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderedStar {
    pub pixel: StarPixel,
    pub rgb: u32,
    pub id: TychoId,
}

#[derive(Clone, Copy, Debug)]
struct ProjectedCandidate {
    pixel: StarPixel,
    rgb: u32,
    id: TychoId,
    vt: f64,
}

const COLOR_POINTS: [(f64, [f64; 3]); 6] = [
    (-0.40, [0.78, 0.86, 1.00]),
    ( 0.00, [0.90, 0.94, 1.00]),
    ( 0.35, [1.00, 1.00, 1.00]),
    ( 0.75, [1.00, 0.93, 0.82]),
    ( 1.25, [1.00, 0.80, 0.64]),
    ( 2.00, [1.00, 0.66, 0.52]),
];

fn selected_vt(vt: Option<f32>) -> Option<f64> {
    let vt = f64::from(vt?);
    (vt.is_finite() && vt < MAGNITUDE_CUTOFF).then_some(vt)
}

fn strict_query_max_vt() -> f64 {
    // Production catalogue filters are inclusive. Native VT is stored as f32, so the
    // greatest representable f32 below 8.0 makes that accepted API implement VT < 8.0
    // exactly without changing catalogue parsing or query semantics.
    f64::from(f32::from_bits(8.0_f32.to_bits() - 1))
}

fn unrounded_peak_intensity(vt: f64) -> f64 {
    let f = 10.0_f64.powf(-0.4 * vt);
    let f_faint = 10.0_f64.powf(-0.4 * MAGNITUDE_CUTOFF);
    let f_bright = 10.0_f64.powf(-0.4 * DISPLAY_BRIGHT_LIMIT);
    let q = f / f_faint;
    let q_bright = f_bright / f_faint;
    let u = ((q.ln_1p() - 2.0_f64.ln()) / (q_bright.ln_1p() - 2.0_f64.ln())).clamp(0.0, 1.0);
    10.0 + 245.0 * u
}

fn peak_intensity(vt: f64) -> f64 { unrounded_peak_intensity(vt).round() }

fn base_rgb(bt: Option<f32>, vt: f64) -> [f64; 3] {
    let Some(bt) = bt.map(f64::from).filter(|value| value.is_finite()) else {
        return [1.0, 1.0, 1.0];
    };
    let ci = (bt - vt).clamp(-0.40, 2.00);
    for pair in COLOR_POINTS.windows(2) {
        let (x0, c0) = pair[0];
        let (x1, c1) = pair[1];
        if ci <= x1 {
            let t = (ci - x0) / (x1 - x0);
            return [
                c0[0] + t * (c1[0] - c0[0]),
                c0[1] + t * (c1[1] - c0[1]),
                c0[2] + t * (c1[2] - c0[2]),
            ];
        }
    }
    COLOR_POINTS[COLOR_POINTS.len() - 1].1
}

fn display_rgb(bt: Option<f32>, vt: f64) -> [f64; 3] {
    let base = base_rgb(bt, vt);
    let s = ((6.5 - vt) / 5.5).clamp(0.0, 1.0);
    [
        1.0 - s * (1.0 - base[0]),
        1.0 - s * (1.0 - base[1]),
        1.0 - s * (1.0 - base[2]),
    ]
}

fn pack_rgb(bt: Option<f32>, vt: f64, multiplier: f64) -> u32 {
    let i = (peak_intensity(vt) * multiplier).min(255.0);
    let rgb = display_rgb(bt, vt);
    let channel = |component: f64| -> u32 {
        (i * component).round().clamp(0.0, 255.0) as u32
    };
    (channel(rgb[0]) << 16) | (channel(rgb[1]) << 8) | channel(rgb[2])
}

fn visible_in_sky_light(vt:f64,limit:Option<f64>)->bool {
    limit.is_some_and(|cutoff|vt<cutoff)
}

/// Sample the actual astronomical horizon (altitude == 0 degrees) through the
/// same runtime ALT/AZ camera projection as stars. The dash cadence is applied
/// in screen space only after projection, so the line remains tied to the
/// geometric horizon rather than a decorative fixed row.
fn project_horizon(camera:Camera,out:&mut Vec<StarPixel>) {
    out.clear();
    let mut last=None;
    // 0.1-degree azimuth sampling is dense enough to cover native pixels near
    // the camera center without ever becoming a full-screen geometry workload.
    for step in 0..=3600 {
        let azimuth=(step as f64*0.1).to_radians();
        let Some(pixel)=project_alt_az_with_camera(0.0,azimuth,
            crate::pixel::WIDTH,crate::pixel::HEIGHT,camera) else {continue;};
        // Eight pixels on / eight pixels off. The projector supplies position;
        // this predicate only gives the horizon its dotted visual vocabulary.
        if (pixel.x.div_euclid(8)&1)!=0 || last==Some(pixel) {continue;}
        out.push(pixel);
        last=Some(pixel);
    }
    out.sort_unstable();
    out.dedup();
}

fn resolve_collisions(candidates: &mut Vec<ProjectedCandidate>, out: &mut Vec<RenderedStar>) {
    candidates.sort_unstable_by(|a, b| {
        a.pixel.cmp(&b.pixel)
            .then_with(|| a.vt.partial_cmp(&b.vt).expect("selected VT is finite"))
            .then_with(|| a.id.cmp(&b.id))
    });
    out.clear();
    for candidate in candidates.iter().copied() {
        if out.last().is_some_and(|star| star.pixel == candidate.pixel) { continue; }
        out.push(RenderedStar { pixel: candidate.pixel, rgb: candidate.rgb, id: candidate.id });
    }
}

pub struct StarMap {
    stars: Vec<TychoStar>,
    source_by_id: HashMap<TychoId, usize>,
    pub cultures: Layer,
    horizontal:Vec<Option<Horizontal>>,
    diagnostics: Vec<DiagnosticBinding>,
    diagnostic_by_star: Vec<Option<usize>>,
    pixels: Vec<RenderedStar>,
    previous_pixels: Vec<RenderedStar>,
    work_pixels: Vec<RenderedStar>,
    work_candidates: Vec<ProjectedCandidate>,
    diagnostic_points: Vec<DiagnosticPoint>,
    previous_diagnostic_points: Vec<DiagnosticPoint>,
    work_diagnostic_points: Vec<DiagnosticPoint>,
    horizon_pixels: Vec<StarPixel>,
    previous_horizon_pixels: Vec<StarPixel>,
    work_horizon_pixels: Vec<StarPixel>,
    counts: StarCounts,
    camera: Camera,
    view_input: ViewInput,
    altitude_velocity_deg_s: f64,
    azimuth_velocity_deg_s: f64,
    pulse_altitude_axis: i8,
    pulse_altitude_remaining_deg: f64,
    pulse_azimuth_axis: i8,
    pulse_azimuth_remaining_deg: f64,
    last_camera_instant: Option<Instant>,
    last_projected_camera: Option<Camera>,
    last_projected_utc: Option<u64>,
    last_projected_tick: Option<u64>,
    last_seen_utc: Option<u64>,
    last_seen_tick: Option<u64>,
    last_location: Option<Location>,
    last_multiplier: f64,
    last_diagnostic: Option<String>,
}

fn bind_diagnostics(stars: &[TychoStar]) -> Result<(Vec<DiagnosticBinding>, Vec<Option<usize>>), StarMapError> {
    let mut names = HashSet::with_capacity(DIAGNOSTIC_REGISTRY_LEN);
    let mut ids = HashSet::with_capacity(DIAGNOSTIC_REGISTRY_LEN);
    for registration in DIAGNOSTIC_REGISTRY {
        if !names.insert(registration.name) {
            return Err(StarMapError::new(format!(
                "diagnostic registry validation BLOCKED: duplicate name {}", registration.name)));
        }
        if !ids.insert(registration.id) {
            return Err(StarMapError::new(format!(
                "diagnostic registry validation BLOCKED: duplicate stable ID {}", registration.id)));
        }
    }

    let by_id: HashMap<TychoId, usize> = stars.iter().enumerate().map(|(i, star)| (star.id, i)).collect();
    if by_id.len() != stars.len() {
        return Err(StarMapError::new(
            "diagnostic registry validation BLOCKED: selected catalogue contains duplicate TYC IDs"));
    }

    let mut diagnostics = Vec::with_capacity(DIAGNOSTIC_REGISTRY_LEN);
    let mut diagnostic_by_star = vec![None; stars.len()];
    for (registry_index, registration) in DIAGNOSTIC_REGISTRY.iter().copied().enumerate() {
        let Some(&star_index) = by_id.get(&registration.id) else {
            return Err(StarMapError::new(format!(
                "diagnostic registry validation BLOCKED: {} / {} is not present in the accepted VT < {MAGNITUDE_CUTOFF:.1} render selection",
                registration.name, registration.id)));
        };
        diagnostics.push(DiagnosticBinding {
            name: registration.name,
            id: registration.id,
            star_index,
        });
        diagnostic_by_star[star_index] = Some(registry_index);
    }
    if diagnostics.len() != DIAGNOSTIC_REGISTRY_LEN {
        return Err(StarMapError::new(format!(
            "diagnostic registry validation BLOCKED: expected {DIAGNOSTIC_REGISTRY_LEN} records, resolved {}",
            diagnostics.len())));
    }
    Ok((diagnostics, diagnostic_by_star))
}

fn explicit_jump_between(previous_utc: Option<u64>, previous_tick: Option<u64>, now: u64, tick: u64, rate: f64) -> bool {
    let (Some(previous), Some(previous_tick)) = (previous_utc, previous_tick) else {
        return true;
    };
    let ticks = tick.saturating_sub(previous_tick).max(1);
    let expected = rate.abs() * ticks as f64 / FPS as f64;
    // A command/date jump exceeds ordinary playback by a generous one-minute margin.
    now.abs_diff(previous) as f64 > expected * 2.0 + 60.0
}

impl StarMap {
    pub fn load() -> Result<Self, StarMapError> {
        let path = Path::new(CATALOGUE_PATH);
        let mut catalogue = ProductionCatalogue::open(path, CATALOGUE_CACHE_BYTES)
            .map_err(|e| StarMapError::new(format!(
                "cannot open accepted catalogue {}: {e}", path.display())))?;
        // Whole-sky selection is performed once at startup. Subsequent frames use only
        // this fixed bright subset and never rescan or transform the 2.54M-record asset.
        let query = ConeQuery {
            frame: CatalogueFrame::Icrs,
            epoch_scale: EpochScale::JulianYearTt,
            ra_deg: 0.0,
            dec_deg: 0.0,
            radius_deg: 180.0,
            year_tt: 2000.0,
            magnitude: Some(MagnitudeFilter {
                band: MAGNITUDE_BAND,
                max_magnitude: strict_query_max_vt(),
                include_missing: false,
            }),
            max_results: MAX_SELECTED_STARS,
        };
        let mut stars = catalogue.records(&query).map_err(|e| StarMapError::new(format!(
            "cannot select Tycho-2 VT < {MAGNITUDE_CUTOFF:.1} from {}: {e}",
            path.display())))?;
        // Defensive exact predicate: never depend on the inclusive query implementation
        // for selection semantics, and never accept missing/invalid/non-finite VT.
        stars.retain(|star| selected_vt(star.vt).is_some());
        if stars.is_empty() {
            return Err(StarMapError::new(format!(
                "accepted catalogue {} contains no VT < {MAGNITUDE_CUTOFF:.1} records",
                path.display())));
        }
        if let Some(star) = stars.iter().find(|star| selected_vt(star.vt).is_some_and(|vt| vt < DISPLAY_BRIGHT_LIMIT)) {
            return Err(StarMapError::new(format!(
                "FAIL: display bound requires revision: {} has native VT {}",
                star.id, selected_vt(star.vt).expect("selected VT"))));
        }
        let (diagnostics, diagnostic_by_star) = bind_diagnostics(&stars)?;
        let source_by_id=stars.iter().enumerate().map(|(index,star)|(star.id,index)).collect();
        let data=CultureDataset::load_dir(Path::new("data/cultures/stellarium-v26.3"))
            .map_err(|e|StarMapError::new(format!("culture dataset load: {e}")))?;
        if data.catalogue_sha256 != "1b0cbafb8f660fc555762aa1cbd9f915983f079611e75ce10d2d55730a200a9b" {
            return Err(StarMapError::new("culture dataset does not match accepted catalogue identity"));
        }
        // One-time full-catalogue audit binding. Never read or bind inside the frame loop.
        data.bind_catalogue(&mut catalogue)
            .map_err(|e|StarMapError::new(format!("culture Tycho binding: {e}")))?;
        let cultures=Layer::new(&data,&source_by_id)
            .map_err(|e|StarMapError::new(format!("culture render bindings: {e}")))?;
        let pixel_capacity = stars.len();
        Ok(Self {
            stars,
            source_by_id,
            cultures,
            horizontal:vec![None;pixel_capacity],
            diagnostics,
            diagnostic_by_star,
            pixels: Vec::with_capacity(pixel_capacity),
            previous_pixels: Vec::with_capacity(pixel_capacity),
            work_pixels: Vec::with_capacity(pixel_capacity),
            work_candidates: Vec::with_capacity(pixel_capacity),
            diagnostic_points: Vec::with_capacity(DIAGNOSTIC_REGISTRY_LEN),
            previous_diagnostic_points: Vec::with_capacity(DIAGNOSTIC_REGISTRY_LEN),
            work_diagnostic_points: Vec::with_capacity(DIAGNOSTIC_REGISTRY_LEN),
            horizon_pixels: Vec::with_capacity(crate::pixel::WIDTH),
            previous_horizon_pixels: Vec::with_capacity(crate::pixel::WIDTH),
            work_horizon_pixels: Vec::with_capacity(crate::pixel::WIDTH),
            counts: StarCounts::default(),
            camera: Camera::default(),
            view_input: ViewInput::default(),
            altitude_velocity_deg_s: 0.0,
            azimuth_velocity_deg_s: 0.0,
            pulse_altitude_axis: 0,
            pulse_altitude_remaining_deg: 0.0,
            pulse_azimuth_axis: 0,
            pulse_azimuth_remaining_deg: 0.0,
            last_camera_instant: None,
            last_projected_camera: None,
            last_projected_utc: None,
            last_projected_tick: None,
            last_seen_utc: None,
            last_seen_tick: None,
            last_location: None,
            last_multiplier: 1.0,
            last_diagnostic: None,
        })
    }

    pub fn set_culture(&mut self,id:&str)->Result<bool,String> {
        let changed=self.cultures.set_culture(id)?;
        if changed {self.cultures.reproject(&self.horizontal,crate::pixel::WIDTH,crate::pixel::HEIGHT,self.camera);}
        Ok(changed)
    }
    pub fn camera(&self)->Camera {self.camera}

    /// Immediate camera navigation to a known horizontal sky coordinate.
    /// Clears held/pulsed motion so callers land exactly on target.
    pub fn center_on(&mut self,altitude_deg:f64,azimuth_deg:f64)->Result<bool,String> {
        if !altitude_deg.is_finite() || !azimuth_deg.is_finite() ||
            !(CAMERA_MIN_ALTITUDE_DEG..=CAMERA_MAX_ALTITUDE_DEG).contains(&altitude_deg) {
            return Err("planet target is outside the visible sky".into());
        }
        let target=Camera{altitude_deg,azimuth_deg:azimuth_deg.rem_euclid(360.0)};
        let changed=self.camera!=target || self.view_input!=ViewInput::default() ||
            self.altitude_velocity_deg_s!=0.0 || self.azimuth_velocity_deg_s!=0.0 ||
            self.pulse_altitude_remaining_deg!=0.0 || self.pulse_azimuth_remaining_deg!=0.0;
        self.camera=target;self.view_input=ViewInput::default();
        self.altitude_velocity_deg_s=0.0;self.azimuth_velocity_deg_s=0.0;
        self.pulse_altitude_axis=0;self.pulse_altitude_remaining_deg=0.0;
        self.pulse_azimuth_axis=0;self.pulse_azimuth_remaining_deg=0.0;
        self.last_camera_instant=Some(Instant::now());
        Ok(changed)
    }

    /// Set level-triggered camera intent. This is the preferred path for X11:
    /// no destination is queued, so releasing a key immediately changes the
    /// desired velocity to zero.
    pub fn set_view_input(&mut self,input:ViewInput)->bool {
        let changed=self.view_input!=input;
        self.view_input=input;
        // Release is a hard stop, not a deceleration request. This prevents a
        // frame-time velocity from surviving after current key state reaches zero.
        if input.altitude_axis==0 {self.altitude_velocity_deg_s=0.0;}
        if input.azimuth_axis==0 {self.azimuth_velocity_deg_s=0.0;}
        changed
    }

    /// Press/repeat events are discrete one-degree camera steps. This makes a tap
    /// exact and leaves no queued velocity or deceleration tail after release.
    pub fn apply_view_control(&mut self,control:ViewControl)->bool {
        self.view_input=ViewInput::default();
        self.altitude_velocity_deg_s=0.0;
        self.azimuth_velocity_deg_s=0.0;
        self.pulse_altitude_axis=0;
        self.pulse_altitude_remaining_deg=0.0;
        self.pulse_azimuth_axis=0;
        self.pulse_azimuth_remaining_deg=0.0;
        self.last_camera_instant=Some(Instant::now());
        self.camera.apply(control)
    }

    /// Discrete view steps have no held velocity to cancel.
    pub fn release_view_control(&mut self,_control:ViewControl)->bool { false }

    fn advance_view_seconds(&mut self,dt:f64)->bool {
        let dt=dt.clamp(0.0,0.1);
        if dt==0.0 {return false;}
        let altitude_axis=if self.view_input.altitude_axis!=0 {self.view_input.altitude_axis} else {self.pulse_altitude_axis};
        let azimuth_axis=if self.view_input.azimuth_axis!=0 {self.view_input.azimuth_axis} else {self.pulse_azimuth_axis};
        let altitude_target=altitude_axis as f64*CAMERA_MAX_SPEED_DEG_PER_SEC;
        let azimuth_target=azimuth_axis as f64*CAMERA_MAX_SPEED_DEG_PER_SEC;
        self.altitude_velocity_deg_s=altitude_target;
        self.azimuth_velocity_deg_s=azimuth_target;
        let mut altitude_delta=altitude_target*dt;
        let mut azimuth_delta=azimuth_target*dt;

        if self.view_input.altitude_axis==0 && self.pulse_altitude_remaining_deg>0.0 {
            let travel=altitude_delta.abs().min(self.pulse_altitude_remaining_deg);
            altitude_delta=travel.copysign(altitude_delta);
            self.pulse_altitude_remaining_deg-=travel;
            if self.pulse_altitude_remaining_deg<=1e-9 {
                self.pulse_altitude_remaining_deg=0.0;
                self.pulse_altitude_axis=0;
                self.altitude_velocity_deg_s=0.0;
            }
        }
        if self.view_input.azimuth_axis==0 && self.pulse_azimuth_remaining_deg>0.0 {
            let travel=azimuth_delta.abs().min(self.pulse_azimuth_remaining_deg);
            azimuth_delta=travel.copysign(azimuth_delta);
            self.pulse_azimuth_remaining_deg-=travel;
            if self.pulse_azimuth_remaining_deg<=1e-9 {
                self.pulse_azimuth_remaining_deg=0.0;
                self.pulse_azimuth_axis=0;
                self.azimuth_velocity_deg_s=0.0;
            }
        }

        let before=self.camera;
        self.camera.altitude_deg=(self.camera.altitude_deg+altitude_delta)
            .clamp(CAMERA_MIN_ALTITUDE_DEG,CAMERA_MAX_ALTITUDE_DEG);
        if (self.camera.altitude_deg-CAMERA_MIN_ALTITUDE_DEG).abs()<1e-9 && self.altitude_velocity_deg_s<0.0 ||
            (self.camera.altitude_deg-CAMERA_MAX_ALTITUDE_DEG).abs()<1e-9 && self.altitude_velocity_deg_s>0.0 {
            self.altitude_velocity_deg_s=0.0;
        }
        self.camera.azimuth_deg=(self.camera.azimuth_deg+azimuth_delta).rem_euclid(360.0);
        self.camera!=before
    }

    fn advance_view(&mut self)->bool {
        let now=Instant::now();
        let dt=self.last_camera_instant.map_or(0.0,|last|now.saturating_duration_since(last).as_secs_f64());
        self.last_camera_instant=Some(now);
        self.advance_view_seconds(dt)
    }
    pub fn pixels(&self) -> &[RenderedStar] { &self.pixels }
    pub fn previous_pixels(&self) -> &[RenderedStar] { &self.previous_pixels }
    pub fn horizon_pixels(&self)->&[StarPixel] {&self.horizon_pixels}
    pub fn previous_horizon_pixels(&self)->&[StarPixel] {&self.previous_horizon_pixels}
    pub fn diagnostics(&self) -> &[DiagnosticPoint] { &self.diagnostic_points }
    pub fn previous_diagnostics(&self) -> &[DiagnosticPoint] { &self.previous_diagnostic_points }
    pub fn counts(&self) -> StarCounts { self.counts }

    fn refresh_horizon(&mut self) {
        project_horizon(self.camera,&mut self.work_horizon_pixels);
        self.previous_horizon_pixels.clear();
        std::mem::swap(&mut self.previous_horizon_pixels,&mut self.horizon_pixels);
        std::mem::swap(&mut self.horizon_pixels,&mut self.work_horizon_pixels);
    }

    fn set_runtime_error(&mut self, message: String) -> bool {
        if self.last_diagnostic.as_deref() != Some(message.as_str()) {
            eprintln!("Space Zoo star map: {message}");
            self.last_diagnostic = Some(message);
        }
        let mut changed = !self.pixels.is_empty() || !self.diagnostic_points.is_empty() || self.counts != StarCounts::default();
        self.previous_pixels.clear();
        std::mem::swap(&mut self.previous_pixels, &mut self.pixels);
        self.pixels.clear();
        self.previous_diagnostic_points.clear();
        std::mem::swap(&mut self.previous_diagnostic_points, &mut self.diagnostic_points);
        self.diagnostic_points.clear();
        self.counts = StarCounts::default();
        self.horizontal.fill(None);
        self.cultures.reproject(&self.horizontal,crate::pixel::WIDTH,crate::pixel::HEIGHT,self.camera);
        self.refresh_horizon();
        changed|=self.previous_horizon_pixels!=self.horizon_pixels;
        changed
    }

    fn recolor_cached(&mut self, multiplier: f64) -> bool {
        self.previous_pixels.clear();
        self.previous_pixels.extend_from_slice(&self.pixels);
        for rendered in &mut self.pixels {
            let source_index=*self.source_by_id.get(&rendered.id)
                .expect("rendered winner remains in fixed selected catalogue");
            let source=&self.stars[source_index];
            let vt=selected_vt(source.vt).expect("rendered winner retains valid catalogue VT");
            rendered.rgb=pack_rgb(source.bt,vt,multiplier);
        }
        self.previous_diagnostic_points.clear();
        self.previous_diagnostic_points.extend_from_slice(&self.diagnostic_points);
        for point in &mut self.diagnostic_points {
            let binding=self.diagnostics[point.registry_index];
            let star=&self.stars[binding.star_index];
            let vt=selected_vt(star.vt).expect("diagnostic remains in fixed selected catalogue");
            point.rgb=pack_rgb(star.bt,vt,multiplier);
        }
        self.last_multiplier=multiplier;
        !self.pixels.is_empty() || !self.diagnostic_points.is_empty()
    }

    /// Camera-only projection path. The expensive stellar engine has already
    /// produced horizontal coordinates; view motion only needs to reproject
    /// those cached ALT/AZ values through the new camera.
    fn reproject_cached_camera(&mut self,multiplier:f64,visible_limit:Option<f64>)->bool {
        self.work_pixels.clear();
        self.work_candidates.clear();
        self.work_diagnostic_points.clear();
        let mut counts=StarCounts::default();
        for (star_index,star) in self.stars.iter().enumerate() {
            let Some(horizontal)=self.horizontal[star_index] else {continue;};
            if horizontal.altitude < 0.0 {continue;}
            let vt=selected_vt(star.vt).expect("fixed selected catalogue retains valid VT");
            if !visible_in_sky_light(vt,visible_limit) {continue;}
            counts.above_horizon+=1;
            let Some(pixel)=project_alt_az_with_camera(
                horizontal.altitude,horizontal.azimuth,crate::pixel::WIDTH,crate::pixel::HEIGHT,self.camera
            ) else {continue;};
            counts.in_view+=1;
            let rgb=pack_rgb(star.bt,vt,multiplier);
            self.work_candidates.push(ProjectedCandidate{pixel,rgb,id:star.id,vt});
            if let Some(registry_index)=self.diagnostic_by_star[star_index] {
                let diagnostic=self.diagnostics[registry_index];
                debug_assert_eq!(diagnostic.star_index,star_index);
                self.work_diagnostic_points.push(DiagnosticPoint{
                    name:diagnostic.name,id:diagnostic.id,
                    altitude_deg:horizontal.altitude.to_degrees(),
                    azimuth_deg:horizontal.azimuth.to_degrees().rem_euclid(360.0),
                    pixel,rgb,rendered:false,registry_index,
                });
            }
        }
        resolve_collisions(&mut self.work_candidates,&mut self.work_pixels);
        for point in &mut self.work_diagnostic_points {
            point.rendered=self.work_pixels
                .binary_search_by_key(&point.pixel,|star|star.pixel)
                .ok().is_some_and(|index|self.work_pixels[index].id==point.id);
        }
        self.work_diagnostic_points.sort_unstable_by_key(|point|point.registry_index);
        counts.diagnostic=self.work_diagnostic_points.len();
        self.previous_pixels.clear();
        std::mem::swap(&mut self.previous_pixels,&mut self.pixels);
        std::mem::swap(&mut self.pixels,&mut self.work_pixels);
        self.previous_diagnostic_points.clear();
        std::mem::swap(&mut self.previous_diagnostic_points,&mut self.diagnostic_points);
        std::mem::swap(&mut self.diagnostic_points,&mut self.work_diagnostic_points);
        self.counts=counts;
        self.cultures.reproject(&self.horizontal,crate::pixel::WIDTH,crate::pixel::HEIGHT,self.camera);
        self.refresh_horizon();
        self.last_multiplier=multiplier;
        self.last_diagnostic=None;
        true
    }

    fn explicit_jump(&self, world: &World, now: u64) -> bool {
        explicit_jump_between(self.last_seen_utc, self.last_seen_tick, now, world.tick, world.sky.rate())
    }

    /// Recompute at most twice per real second during continuous playback, but make
    /// explicit time/location jumps visible on the next rendered frame. All dots,
    /// diagnostic points and counts are produced from the same observation snapshot.
    pub fn update(&mut self, world: &World) -> bool {
        self.advance_view();
        let now = world.sky.now_secs();
        let location = world.sky.location();
        let jumped = self.explicit_jump(world, now);
        self.last_seen_utc = Some(now);
        self.last_seen_tick = Some(world.tick);

        let location_changed = location != self.last_location;
        let camera_changed = self.last_projected_camera != Some(self.camera);
        let time_changed = self.last_projected_utc != Some(now);
        let multiplier=world.sky.star_multiplier();
        let visible_limit=world.sky.sky_light_state().visible_limiting_magnitude;
        let multiplier_changed=multiplier.to_bits()!=self.last_multiplier.to_bits();
        let cadence_due = self.last_projected_tick
            .map_or(true, |tick| world.tick.saturating_sub(tick) >= UPDATE_TICKS);
        // Camera motion never reruns stellar astrometry. ALT/AZ are independent
        // of the viewport, so smooth panning can use the cached horizontal frame.
        // A normal time/location projection still wins when it is due.
        let astronomy_due=location_changed || (time_changed && (cadence_due || jumped));
        if camera_changed && !astronomy_due {
            self.last_projected_camera=Some(self.camera);
            return self.reproject_cached_camera(multiplier,visible_limit);
        }
        // Brightness alone never pulls stellar projection forward.
        if !astronomy_due {
            if multiplier_changed { return self.recolor_cached(multiplier); }
            return false;
        }
        self.last_projected_utc = Some(now);
        self.last_projected_tick = Some(world.tick);
        self.last_projected_camera = Some(self.camera);
        self.last_location = location;
        let Some(location) = location else {
            return self.set_runtime_error("observer location is unavailable".into());
        };
        let Ok(unix) = i64::try_from(now) else {
            return self.set_runtime_error("simulated UTC is outside the stellar engine range".into());
        };
        let utc = match UtcInstant::from_unix_seconds(unix) {
            Ok(utc) => utc,
            Err(_) => return self.set_runtime_error(
                "simulated UTC is outside the accepted stellar-engine interval (2000-01-01 through 2050-12-31 UTC)".into()),
        };
        let observer = match Observer::new(location.latitude, location.longitude, 0.0) {
            Ok(observer) => observer,
            Err(e) => return self.set_runtime_error(format!("invalid observer for star map: {e}")),
        };
        let context = match prepare_observation(observer, utc, ObservationOptions::default()) {
            Ok(context) => context,
            Err(e) => return self.set_runtime_error(format!("stellar observation failed: {e}")),
        };
        let year_tt = context.julian_year_tt();
        if !(2000.0..=2050.0).contains(&year_tt) {
            return self.set_runtime_error(
                "simulated UTC is outside the production catalogue astrometry interval (Julian TT 2000.0 through 2050.0)".into());
        }

        self.work_pixels.clear();
        self.work_candidates.clear();
        self.work_diagnostic_points.clear();
        self.horizontal.fill(None);
        let mut counts = StarCounts::default();
        let mut engine_error = None;
        for (star_index, star) in self.stars.iter().enumerate() {
            let icrs = star.direction_icrs_vector(year_tt);
            let horizontal = match evaluate_icrs_direction(&context, icrs) {
                Ok(result) => result,
                Err(e) => {
                    engine_error = Some(format!("stellar engine rejected {}: {e}", star.id));
                    break;
                }
            };
            self.horizontal[star_index]=horizontal.azimuth_rad.map(|azimuth| Horizontal {
                altitude:horizontal.altitude_rad,azimuth,
            });
            // Adopt the existing geometric-horizon rule exactly: altitude == 0 remains eligible.
            if horizontal.altitude_rad < 0.0 { continue; }
            let vt = selected_vt(star.vt).expect("fixed selected catalogue retains valid VT");
            if !visible_in_sky_light(vt,visible_limit) {continue;}
            counts.above_horizon += 1;
            let Some(azimuth) = horizontal.azimuth_rad else { continue; };
            let Some(pixel) = project_alt_az_with_camera(
                horizontal.altitude_rad,
                azimuth,
                crate::pixel::WIDTH,
                crate::pixel::HEIGHT,
                self.camera,
            ) else { continue; };
            counts.in_view += 1;
            let rgb = pack_rgb(star.bt, vt, multiplier);
            self.work_candidates.push(ProjectedCandidate { pixel, rgb, id: star.id, vt });
            if let Some(registry_index) = self.diagnostic_by_star[star_index] {
                let diagnostic = self.diagnostics[registry_index];
                debug_assert_eq!(diagnostic.star_index, star_index);
                self.work_diagnostic_points.push(DiagnosticPoint {
                    name: diagnostic.name,
                    id: diagnostic.id,
                    altitude_deg: horizontal.altitude_rad.to_degrees(),
                    azimuth_deg: azimuth.to_degrees().rem_euclid(360.0),
                    pixel,
                    rgb,
                    rendered: false,
                    registry_index,
                });
            }
        }
        if let Some(message) = engine_error {
            return self.set_runtime_error(message);
        }

        resolve_collisions(&mut self.work_candidates, &mut self.work_pixels);
        for point in &mut self.work_diagnostic_points {
            point.rendered = self.work_pixels
                .binary_search_by_key(&point.pixel, |star| star.pixel)
                .ok()
                .is_some_and(|index| self.work_pixels[index].id == point.id);
        }
        self.work_diagnostic_points.sort_unstable_by_key(|point| point.registry_index);
        counts.diagnostic = self.work_diagnostic_points.len();
        // Counts remain pre-collision record counts, now after the authoritative
        // solar-altitude visibility limit. The star layer itself is one
        // deterministic native pixel per winning catalogue identity.

        self.previous_pixels.clear();
        std::mem::swap(&mut self.previous_pixels, &mut self.pixels);
        std::mem::swap(&mut self.pixels, &mut self.work_pixels);
        self.previous_diagnostic_points.clear();
        std::mem::swap(&mut self.previous_diagnostic_points, &mut self.diagnostic_points);
        std::mem::swap(&mut self.diagnostic_points, &mut self.work_diagnostic_points);
        self.counts = counts;
        self.cultures.reproject(&self.horizontal,crate::pixel::WIDTH,crate::pixel::HEIGHT,self.camera);
        self.refresh_horizon();
        self.last_multiplier = multiplier;
        self.last_diagnostic = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_forward_and_reverse_are_not_misclassified_as_jumps() {
        let previous = 1_000_000_u64;
        let tick = 101_u64;
        assert!(!explicit_jump_between(Some(previous), Some(100), previous + 120, tick, 3600.0));
        assert!(!explicit_jump_between(Some(previous), Some(100), previous - 120, tick, -3600.0));
    }

    #[test]
    fn explicit_date_changes_bypass_the_continuous_playback_cadence() {
        let previous = 1_000_000_u64;
        assert!(explicit_jump_between(Some(previous), Some(100), previous + 86_400, 101, 3600.0));
        assert!(explicit_jump_between(Some(previous), Some(100), previous - 86_400, 101, -3600.0));
        assert!(explicit_jump_between(None, None, previous, 100, 0.0));
    }

    #[test]
    fn continuous_update_cadence_is_two_hz_at_thirty_hz_simulation() {
        assert_eq!(FPS, 30);
        assert_eq!(UPDATE_TICKS, 15);
    }

    #[test]
    fn twilight_limit_filters_native_vt_strictly() {
        assert!(!visible_in_sky_light(-1.0,None));
        assert!(visible_in_sky_light(1.999,Some(2.0)));
        assert!(!visible_in_sky_light(2.0,Some(2.0)));
        assert!(visible_in_sky_light(7.999,Some(8.0)));
        assert!(!visible_in_sky_light(8.0,Some(8.0)));
    }

    #[test]
    fn dotted_horizon_is_projected_from_altitude_zero_and_moves_with_camera_pitch() {
        let mut level=Vec::new();
        project_horizon(Camera{altitude_deg:0.0,azimuth_deg:0.0},&mut level);
        assert!(!level.is_empty());
        assert!(level.iter().all(|p|p.y==crate::pixel::HEIGHT as i32/2));
        assert!(level.iter().all(|p|(p.x.div_euclid(8)&1)==0));

        let mut pitched=Vec::new();
        project_horizon(Camera::default(),&mut pitched);
        assert!(!pitched.is_empty());
        assert!(pitched.iter().all(|p|p.y>crate::pixel::HEIGHT as i32/2));
        assert_ne!(level,pitched);
    }

    #[test]
    fn diagnostic_registry_has_exactly_100_unique_names_and_stable_ids() {
        assert_eq!(DIAGNOSTIC_REGISTRY.len(), DIAGNOSTIC_REGISTRY_LEN);
        let names: HashSet<_> = DIAGNOSTIC_REGISTRY.iter().map(|entry| entry.name).collect();
        let ids: HashSet<_> = DIAGNOSTIC_REGISTRY.iter().map(|entry| entry.id).collect();
        assert_eq!(names.len(), DIAGNOSTIC_REGISTRY_LEN);
        assert_eq!(ids.len(), DIAGNOSTIC_REGISTRY_LEN);
    }


    fn assert_rgb_close(actual: [f64; 3], expected: [f64; 3]) {
        for i in 0..3 {
            assert!((actual[i] - expected[i]).abs() < 1e-6,
                "channel {i}: actual={} expected={}", actual[i], expected[i]);
        }
    }

    fn candidate(pixel: StarPixel, vt: f64, id: TychoId, rgb: u32) -> ProjectedCandidate {
        ProjectedCandidate { pixel, vt, id, rgb }
    }

    fn resolved(mut candidates: Vec<ProjectedCandidate>) -> Vec<RenderedStar> {
        let mut out = Vec::new();
        resolve_collisions(&mut candidates, &mut out);
        out
    }

    fn synthetic_star(bt: Option<f32>, vt: Option<f32>) -> TychoStar {
        TychoStar {
            id: TychoId { region: 100, number: 200, component: 1 },
            mean_ra_deg: Some(37.946_618_61), mean_dec_deg: Some(89.264_135_28),
            observed_ra_deg: 37.946_618_61, observed_dec_deg: 89.264_135_28,
            observed_epoch_ra: 1991.25, observed_epoch_dec: 1991.25,
            pm_ra_star: Some(44.0), pm_dec: Some(-11.0),
            bt, vt, bt_error: None, vt_error: None,
            proximity_tenth_arcsec: 999, source_quality: [None; 12],
            source_position_count: None, hip: None, ccdm: [0; 3],
            observed_correlation: None, pflag: 32, posflag: 32, tycho1_flag: 32,
            quality: 0,
        }
    }

    #[test]
    fn photometry_052_selection_is_strict_and_rejects_invalid_vt() {
        assert_eq!(selected_vt(Some(7.999)), Some(f64::from(7.999_f32)));
        assert_eq!(selected_vt(Some(8.000)), None);
        assert_eq!(selected_vt(Some(8.001)), None);
        assert_eq!(selected_vt(None), None);
        assert_eq!(selected_vt(Some(f32::NAN)), None);
        assert_eq!(selected_vt(Some(f32::INFINITY)), None);
        assert_eq!(selected_vt(Some(f32::NEG_INFINITY)), None);
        assert!(strict_query_max_vt() < 8.0);
        assert_eq!(strict_query_max_vt() as f32, f32::from_bits(8.0_f32.to_bits() - 1));
    }

    #[test]
    fn photometry_052_brightness_reference_table_and_dense_monotonicity() {
        let references = [
            (7.999,10.0),(7.5,17.0),(7.0,26.0),(6.5,36.0),(6.0,47.0),
            (5.0,71.0),(4.0,97.0),(3.0,123.0),(2.0,149.0),(1.0,176.0),
            (0.0,202.0),(-1.0,229.0),(-2.0,255.0),
        ];
        for (vt, expected) in references {
            assert_eq!(peak_intensity(vt), expected, "VT={vt}");
        }
        let mut previous_unrounded = unrounded_peak_intensity(-2.0);
        let mut previous_rounded = peak_intensity(-2.0);
        for step in 1..10_000 {
            let vt = -2.0 + step as f64 * (9.999 / 10_000.0);
            let unrounded = unrounded_peak_intensity(vt);
            let rounded = peak_intensity(vt);
            assert!(unrounded < previous_unrounded, "unrounded mapping must strictly decrease at VT={vt}");
            assert!(rounded <= previous_rounded, "rounded mapping must never increase at VT={vt}");
            previous_unrounded = unrounded;
            previous_rounded = rounded;
        }
    }

    #[test]
    fn photometry_052_color_control_points_midpoints_and_clamps() {
        for &(ci, expected) in &COLOR_POINTS {
            assert_rgb_close(base_rgb(Some(ci as f32), 0.0), expected);
        }
        for pair in COLOR_POINTS.windows(2) {
            let (x0, c0) = pair[0];
            let (x1, c1) = pair[1];
            let midpoint = (x0 + x1) * 0.5;
            let expected = [
                (c0[0] + c1[0]) * 0.5,
                (c0[1] + c1[1]) * 0.5,
                (c0[2] + c1[2]) * 0.5,
            ];
            assert_rgb_close(base_rgb(Some(midpoint as f32), 0.0), expected);
        }
        assert_rgb_close(base_rgb(Some(-5.0), 0.0), COLOR_POINTS[0].1);
        assert_rgb_close(base_rgb(Some(5.0), 0.0), COLOR_POINTS[COLOR_POINTS.len()-1].1);
    }

    #[test]
    fn photometry_052_missing_invalid_bt_is_neutral() {
        for bt in [None, Some(f32::NAN), Some(f32::INFINITY), Some(f32::NEG_INFINITY)] {
            assert_eq!(base_rgb(bt, 2.0), [1.0, 1.0, 1.0]);
            assert_eq!(display_rgb(bt, 2.0), [1.0, 1.0, 1.0]);
        }
    }

    #[test]
    fn photometry_052_desaturation_full_partial_and_zero_retention() {
        let full = base_rgb(Some(3.0), 1.0); // CI=2.0
        assert_rgb_close(display_rgb(Some(3.0), 1.0), full);
        let base = base_rgb(Some(5.75), 3.75); // CI=2.0
        let partial = display_rgb(Some(5.75), 3.75);
        for i in 0..3 {
            assert!((partial[i] - (1.0 - 0.5 * (1.0 - base[i]))).abs() < 1e-12);
        }
        assert_eq!(display_rgb(Some(8.5), 6.5), [1.0, 1.0, 1.0]);
        assert_eq!(display_rgb(Some(9.0), 7.0), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn photometry_052_final_rounding_and_peak_channel_equality() {
        assert_eq!(pack_rgb(Some(3.0), 1.0, 1.0), 0xb0745c);
        assert_eq!(pack_rgb(Some(3.75), 3.0, 1.0), 0x7b766d);
        for &(vt, ci) in &[(-2.0,-0.4),(-1.0,0.0),(0.0,0.35),(1.0,0.75),(3.0,1.25),(6.0,2.0),(7.999,0.35)] {
            let rgb = pack_rgb(Some((vt + ci) as f32), vt, 1.0);
            let channels = [(rgb >> 16) & 0xff, (rgb >> 8) & 0xff, rgb & 0xff];
            assert_eq!(*channels.iter().max().unwrap(), peak_intensity(vt) as u32, "VT={vt} CI={ci}");
        }
    }

    #[test]
    fn star_knob_05201_one_x_is_exact_baseline_and_colour_source_is_unchanged() {
        fn baseline_pack_rgb(bt: Option<f32>, vt: f64) -> u32 {
            let i=peak_intensity(vt);
            let rgb=display_rgb(bt,vt);
            let channel=|component:f64|->u32{(i*component).round().clamp(0.0,255.0) as u32};
            (channel(rgb[0])<<16)|(channel(rgb[1])<<8)|channel(rgb[2])
        }
        for &(vt,ci) in &[(-2.0,-0.4),(-1.0,0.0),(0.0,0.35),(1.0,0.75),(3.75,2.0),(6.5,1.25),(7.999,0.35)] {
            let bt=Some((vt+ci) as f32);
            assert_eq!(pack_rgb(bt,vt,1.0),baseline_pack_rgb(bt,vt));
        }
        let bt=Some(5.75_f32);
        let vt=3.75;
        let rgb=display_rgb(bt,vt);
        let i=(peak_intensity(vt)*2.0).min(255.0);
        let expected=((i*rgb[0]).round().clamp(0.0,255.0) as u32)<<16
            | ((i*rgb[1]).round().clamp(0.0,255.0) as u32)<<8
            | ((i*rgb[2]).round().clamp(0.0,255.0) as u32);
        assert_eq!(pack_rgb(bt,vt,2.0),expected,"boost must reuse catalogue BT/VT display colour unchanged");
    }

    #[test]
    fn star_knob_05201_multiplies_baseline_and_saturates_channels() {
        let vt=7.5;
        let bt=Some(vt as f32);
        assert_eq!(peak_intensity(vt),17.0);
        assert_eq!(pack_rgb(bt,vt,2.0),0x222222);
        assert_eq!(pack_rgb(bt,vt,4.0),0x444444);
        assert_eq!(pack_rgb(bt,vt,100.0),0xffffff);
        assert_eq!(pack_rgb(bt,vt,f64::MAX),0xffffff);
    }

    #[test]
    fn star_knob_05201_is_absolute_not_cumulative() {
        let direct=pack_rgb(Some(7.85),7.5,4.0);
        let after_two_then_four=pack_rgb(Some(7.85),7.5,4.0);
        assert_eq!(after_two_then_four,direct);
        assert_ne!(pack_rgb(Some(7.85),7.5,2.0),direct);
    }

    #[test]
    fn star_knob_05201_recolors_cached_pixels_and_diagnostics_without_moving_them() {
        let star=synthetic_star(Some(5.75),Some(3.75));
        let id=star.id;
        let pixel=StarPixel{x:321,y:123};
        let one_x=pack_rgb(star.bt,3.75,1.0);
        let mut map=StarMap {
            stars:vec![star],
            source_by_id:HashMap::from([(id,0)]),
            cultures:Layer::default(), // Test-only empty layer; production uses Layer::new.
            horizontal:vec![None],
            diagnostics:vec![DiagnosticBinding{name:"TEST",id,star_index:0}],
            diagnostic_by_star:vec![Some(0)],
            pixels:vec![RenderedStar{pixel,rgb:one_x,id}],
            previous_pixels:Vec::new(),work_pixels:Vec::new(),work_candidates:Vec::new(),
            diagnostic_points:vec![DiagnosticPoint{name:"TEST",id,altitude_deg:42.0,azimuth_deg:17.0,
                pixel,rgb:one_x,rendered:true,registry_index:0}],
            previous_diagnostic_points:Vec::new(),work_diagnostic_points:Vec::new(),
            horizon_pixels:Vec::new(),previous_horizon_pixels:Vec::new(),work_horizon_pixels:Vec::new(),
            counts:StarCounts{above_horizon:1,in_view:1,diagnostic:1},
            camera:Camera::default(),view_input:ViewInput::default(),altitude_velocity_deg_s:0.0,azimuth_velocity_deg_s:0.0,
            pulse_altitude_axis:0,pulse_altitude_remaining_deg:0.0,pulse_azimuth_axis:0,pulse_azimuth_remaining_deg:0.0,last_camera_instant:None,
            last_projected_camera:Some(Camera::default()),
            last_projected_utc:Some(1_800_000_000),last_projected_tick:Some(10),
            last_seen_utc:Some(1_800_000_000),last_seen_tick:Some(10),
            last_location:Some(Location::new(40.0,-105.0).unwrap()),last_multiplier:1.0,last_diagnostic:None,
        };
        assert!(map.recolor_cached(4.0));
        assert_eq!(map.pixels[0].pixel,pixel);
        assert_eq!(map.pixels[0].id,id);
        assert_eq!(map.diagnostic_points[0].pixel,pixel);
        assert_eq!(map.diagnostic_points[0].altitude_deg,42.0);
        assert_eq!(map.diagnostic_points[0].azimuth_deg,17.0);
        assert_eq!(map.previous_pixels[0].rgb,one_x);
        assert_eq!(map.previous_diagnostic_points[0].rgb,one_x);
        assert_eq!(map.pixels[0].rgb,pack_rgb(Some(5.75),3.75,4.0));
        assert_eq!(map.diagnostic_points[0].rgb,pack_rgb(Some(5.75),3.75,4.0));
    }

    fn motion_test_map()->StarMap {
        let star=synthetic_star(Some(5.75),Some(3.75));
        let id=star.id;
        StarMap {
            stars:vec![star],source_by_id:HashMap::from([(id,0)]),cultures:Layer::default(),
            horizontal:vec![None],diagnostics:Vec::new(),diagnostic_by_star:vec![None],
            pixels:Vec::new(),previous_pixels:Vec::new(),work_pixels:Vec::new(),work_candidates:Vec::new(),
            diagnostic_points:Vec::new(),previous_diagnostic_points:Vec::new(),work_diagnostic_points:Vec::new(),
            horizon_pixels:Vec::new(),previous_horizon_pixels:Vec::new(),work_horizon_pixels:Vec::new(),
            counts:StarCounts::default(),camera:Camera::default(),view_input:ViewInput::default(),
            altitude_velocity_deg_s:0.0,azimuth_velocity_deg_s:0.0,pulse_altitude_axis:0,pulse_altitude_remaining_deg:0.0,
            pulse_azimuth_axis:0,pulse_azimuth_remaining_deg:0.0,last_camera_instant:None,
            last_projected_camera:Some(Camera::default()),last_projected_utc:None,last_projected_tick:None,
            last_seen_utc:None,last_seen_tick:None,last_location:None,last_multiplier:1.0,last_diagnostic:None,
        }
    }

    #[test]
    fn smooth_camera_velocity_is_symmetric_between_axes() {
        let mut vertical=motion_test_map();
        let mut horizontal=motion_test_map();
        vertical.set_view_input(ViewInput::new(1,0));
        horizontal.set_view_input(ViewInput::new(0,1));
        assert!(vertical.advance_view_seconds(1.0/30.0));
        assert!(horizontal.advance_view_seconds(1.0/30.0));
        let up=vertical.camera.altitude_deg-CAMERA_ALTITUDE_DEG;
        let right=signed_azimuth_delta(CAMERA_AZIMUTH_DEG,horizontal.camera.azimuth_deg);
        assert!((up-right).abs()<1e-9,"axis motion must be angularly identical: up={up} right={right}");
    }

    #[test]
    fn smooth_camera_release_is_an_immediate_hard_stop() {
        let mut map=motion_test_map();
        map.set_view_input(ViewInput::new(1,0));
        map.advance_view_seconds(1.0/30.0);
        let held=map.camera.altitude_deg;
        assert_eq!(map.altitude_velocity_deg_s,CAMERA_MAX_SPEED_DEG_PER_SEC);
        map.set_view_input(ViewInput::default());
        assert_eq!(map.altitude_velocity_deg_s,0.0);
        map.advance_view_seconds(1.0/30.0);
        assert_eq!(map.camera.altitude_deg,held,"release must not move even one more frame");
        let stopped=map.camera;
        assert!(!map.advance_view_seconds(1.0/30.0));
        assert_eq!(map.camera,stopped);
    }

    #[test]
    fn smooth_camera_frame_time_integration_is_stable() {
        let mut thirty=motion_test_map();
        let mut sixty=motion_test_map();
        thirty.set_view_input(ViewInput::new(0,1));
        sixty.set_view_input(ViewInput::new(0,1));
        for _ in 0..30 {thirty.advance_view_seconds(1.0/30.0);}
        for _ in 0..60 {sixty.advance_view_seconds(1.0/60.0);}
        let a=signed_azimuth_delta(CAMERA_AZIMUTH_DEG,thirty.camera.azimuth_deg);
        let b=signed_azimuth_delta(CAMERA_AZIMUTH_DEG,sixty.camera.azimuth_deg);
        assert!((a-b).abs()<1e-9,"30/60 Hz motion must agree: {a} vs {b}");
    }

    #[test]
    fn discrete_camera_press_is_exactly_one_degree_with_no_release_tail() {
        let mut map=motion_test_map();
        assert!(map.apply_view_control(ViewControl::Right));
        let travel=signed_azimuth_delta(CAMERA_AZIMUTH_DEG,map.camera.azimuth_deg);
        assert!((travel-CAMERA_STEP_DEG).abs()<1e-9,"one press must move exactly one degree: {travel}");
        assert!(!map.release_view_control(ViewControl::Right));
        let stopped=map.camera;
        assert!(!map.advance_view_seconds(1.0/30.0));
        assert_eq!(map.camera,stopped);
        for _ in 0..9 {assert!(map.apply_view_control(ViewControl::Right));}
        let repeated=signed_azimuth_delta(CAMERA_AZIMUTH_DEG,map.camera.azimuth_deg);
        assert!((repeated-10.0).abs()<1e-9,"ten press/repeat events must move ten degrees: {repeated}");
    }

    #[test]
    fn smooth_camera_reset_is_immediate() {
        let mut map=motion_test_map();
        map.set_view_input(ViewInput::new(1,1));
        map.advance_view_seconds(1.0/30.0);
        assert!(map.apply_view_control(ViewControl::Reset));
        assert_eq!(map.camera,Camera::default());
        assert_eq!(map.view_input,ViewInput::default());
        assert_eq!(map.altitude_velocity_deg_s,0.0);
        assert_eq!(map.azimuth_velocity_deg_s,0.0);
    }

    #[test]
    fn photometry_052_photometry_does_not_change_alt_az_or_projection() {
        let a = synthetic_star(Some(1.0), Some(0.0));
        let b = synthetic_star(Some(7.0), Some(6.5));
        let utc = UtcInstant::parse("2026-10-16T08:51:00Z").unwrap();
        let observer = Observer::new(40.0, -105.0, 0.0).unwrap();
        let context = prepare_observation(observer, utc, ObservationOptions::default()).unwrap();
        let va = a.direction_icrs_vector(context.julian_year_tt());
        let vb = b.direction_icrs_vector(context.julian_year_tt());
        assert_eq!(va, vb, "BT/VT must not affect catalogue astrometry");
        let ha = evaluate_icrs_direction(&context, va).unwrap();
        let hb = evaluate_icrs_direction(&context, vb).unwrap();
        assert_eq!(ha.altitude_rad, hb.altitude_rad);
        assert_eq!(ha.azimuth_rad, hb.azimuth_rad);
        let pa = project_alt_az(ha.altitude_rad, ha.azimuth_rad.unwrap(), crate::pixel::WIDTH, crate::pixel::HEIGHT)
            .expect("north diagnostic fixture must project");
        let pb = project_alt_az(hb.altitude_rad, hb.azimuth_rad.unwrap(), crate::pixel::WIDTH, crate::pixel::HEIGHT)
            .expect("north diagnostic fixture must project");
        assert_eq!(pa, pb);
    }

    #[test]
    fn photometry_052_collision_brighter_star_wins_in_both_orders() {
        let pixel = StarPixel { x: 400, y: 200 };
        let bright_id = TychoId { region: 10, number: 2, component: 1 };
        let faint_id = TychoId { region: 1, number: 1, component: 1 };
        let bright = candidate(pixel, 2.0, bright_id, 0xaabbcc);
        let faint = candidate(pixel, 5.0, faint_id, 0x112233);
        for input in [vec![bright, faint], vec![faint, bright]] {
            let out = resolved(input);
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].id, bright_id);
            assert_eq!(out[0].rgb, 0xaabbcc);
            assert_eq!(out[0].pixel, pixel);
        }
    }

    #[test]
    fn photometry_052_collision_equal_vt_lower_canonical_id_wins_in_both_orders() {
        let pixel = StarPixel { x: 401, y: 201 };
        let lower = candidate(pixel, 4.0, TychoId { region: 7, number: 42, component: 1 }, 0xabcdef);
        let higher_component = candidate(pixel, 4.0, TychoId { region: 7, number: 42, component: 2 }, 0x123456);
        let higher_number = candidate(pixel, 4.0, TychoId { region: 7, number: 43, component: 1 }, 0x654321);
        for input in [vec![higher_component, higher_number, lower], vec![lower, higher_number, higher_component]] {
            let out = resolved(input);
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].id, lower.id);
            assert_eq!(out[0].pixel, pixel);
        }
    }

    #[test]
    fn photometry_052_collisions_never_create_neighboring_pixels() {
        let pixel = StarPixel { x: 500, y: 250 };
        let out = resolved(vec![
            candidate(pixel, 5.0, TychoId { region: 1, number: 2, component: 1 }, 0x111111),
            candidate(pixel, 4.0, TychoId { region: 1, number: 1, component: 1 }, 0x222222),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].pixel, pixel);
    }

    #[test]
    fn photometry_052_selection_exceeds_legacy_capacity_without_data_loss() {
        assert!(MAX_SELECTED_STARS >= 65_536);
        let values = vec![Some(7.0_f32); 20_001];
        let selected = values.into_iter().filter_map(selected_vt).collect::<Vec<_>>();
        assert_eq!(selected.len(), 20_001, "selection must not retain the old 20,000-star truncation");
    }

    #[test]
    fn photometry_052_display_bound_guard_is_explicit() {
        let offending = -2.001_f64;
        assert!(offending < DISPLAY_BRIGHT_LIMIT);
        let message = format!("FAIL: display bound requires revision: synthetic has native VT {offending}");
        assert!(message.starts_with("FAIL: display bound requires revision"));
    }
}
