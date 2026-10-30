//! v0.51 star projection with the accepted north/+30-degree default and a runtime view camera.
pub const CAMERA_AZIMUTH_DEG: f64 = 0.0;
pub const CAMERA_ALTITUDE_DEG: f64 = 30.0;
pub const HORIZONTAL_FOV_DEG: f64 = 90.0;
pub const CAMERA_STEP_DEG: f64 = 1.0;
/// Accepted v0.54.07 peak travel was 1.25 degrees per 30 Hz tick. v0.54.08
/// expresses the same speed per second so visible motion is independent of
/// render cadence and short frame stalls.
pub const CAMERA_MAX_SPEED_DEG_PER_SEC: f64 = 37.5;
pub const CAMERA_MIN_ALTITUDE_DEG: f64 = 0.0;
pub const CAMERA_MAX_ALTITUDE_DEG: f64 = 90.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub altitude_deg: f64,
    pub azimuth_deg: f64,
}

impl Default for Camera {
    fn default() -> Self { Self { altitude_deg: CAMERA_ALTITUDE_DEG, azimuth_deg: CAMERA_AZIMUTH_DEG } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewControl { Up, Down, Left, Right, Reset }

/// Level-triggered view intent used by windowed input. Axes are normalized to
/// -1, 0, or +1 so horizontal and vertical motion share one controller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewInput {
    pub altitude_axis: i8,
    pub azimuth_axis: i8,
}
impl ViewInput {
    pub fn new(altitude_axis:i8,azimuth_axis:i8)->Self {
        Self {altitude_axis:altitude_axis.clamp(-1,1),azimuth_axis:azimuth_axis.clamp(-1,1)}
    }
}

impl Camera {
    pub fn apply(&mut self, control: ViewControl) -> bool {
        let before=*self;
        match control {
            ViewControl::Up => self.altitude_deg=(self.altitude_deg+CAMERA_STEP_DEG).min(CAMERA_MAX_ALTITUDE_DEG),
            ViewControl::Down => self.altitude_deg=(self.altitude_deg-CAMERA_STEP_DEG).max(CAMERA_MIN_ALTITUDE_DEG),
            ViewControl::Left => self.azimuth_deg=(self.azimuth_deg-CAMERA_STEP_DEG).rem_euclid(360.0),
            ViewControl::Right => self.azimuth_deg=(self.azimuth_deg+CAMERA_STEP_DEG).rem_euclid(360.0),
            ViewControl::Reset => *self=Self::default(),
        }
        *self!=before
    }

    /// Move toward a target without overshoot, using the shortest azimuth path.
    /// Altitude and azimuth are independently bounded so diagonal WASD input
    /// remains responsive instead of one axis starving the other.
    pub fn approach(&mut self, target: Camera, max_step_deg: f64) -> bool {
        if !max_step_deg.is_finite() || max_step_deg <= 0.0 { return false; }
        let before=*self;
        let altitude_delta=target.altitude_deg-self.altitude_deg;
        self.altitude_deg += altitude_delta.clamp(-max_step_deg,max_step_deg);
        if (target.altitude_deg-self.altitude_deg).abs() < 1e-9 {
            self.altitude_deg=target.altitude_deg;
        }
        let azimuth_delta=signed_azimuth_delta(self.azimuth_deg,target.azimuth_deg);
        self.azimuth_deg=(self.azimuth_deg+azimuth_delta.clamp(-max_step_deg,max_step_deg)).rem_euclid(360.0);
        if signed_azimuth_delta(self.azimuth_deg,target.azimuth_deg).abs() < 1e-9 {
            self.azimuth_deg=target.azimuth_deg.rem_euclid(360.0);
        }
        *self!=before
    }
}

/// Signed shortest-path difference `target-current` in [-180, 180).
pub fn signed_azimuth_delta(current_deg:f64,target_deg:f64)->f64 {
    (target_deg-current_deg+180.0).rem_euclid(360.0)-180.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StarPixel {
    pub x: i32,
    pub y: i32,
}

pub fn vertical_fov_deg(width: usize, height: usize) -> f64 {
    let horizontal = HORIZONTAL_FOV_DEG.to_radians();
    (2.0 * ((horizontal * 0.5).tan() * height as f64 / width as f64).atan()).to_degrees()
}

/// Rectilinear camera. Engine azimuth convention is navigation/astronomical:
/// 0=N, 90=E, 180=S, 270=W. At zero roll camera right follows increasing
/// azimuth; the legacy wrapper below uses the accepted north/+30-degree default.
pub fn project_alt_az(
    altitude_rad: f64,
    azimuth_rad: f64,
    width: usize,
    height: usize,
) -> Option<StarPixel> {
    project_alt_az_with_camera(altitude_rad,azimuth_rad,width,height,Camera::default())
}

pub fn project_alt_az_with_camera(
    altitude_rad: f64,
    azimuth_rad: f64,
    width: usize,
    height: usize,
    camera: Camera,
) -> Option<StarPixel> {
    if width == 0 || height == 0 || !altitude_rad.is_finite() || !azimuth_rad.is_finite() ||
        !camera.altitude_deg.is_finite() || !camera.azimuth_deg.is_finite() {
        return None;
    }
    if altitude_rad < 0.0 { return None; }
    let (sa, ca) = altitude_rad.sin_cos();
    // Work in angular offset from the runtime camera using the same navigation
    // azimuth convention as the engine; a zero camera-relative angle lands on center.
    let mut delta_az = (azimuth_rad - camera.azimuth_deg.to_radians())
        .rem_euclid(std::f64::consts::TAU);
    if delta_az > std::f64::consts::PI { delta_az -= std::f64::consts::TAU; }
    let (sd, cd) = delta_az.sin_cos();

    let camera_alt = camera.altitude_deg.to_radians();
    let (sc, cc) = camera_alt.sin_cos();
    // Zero-roll camera basis: right follows increasing azimuth, up is zenithward.
    let x_camera = ca * sd;
    let y_camera = -ca * cd * sc + sa * cc;
    let z_camera = ca * cd * cc + sa * sc;
    if ![x_camera, y_camera, z_camera].iter().all(|v| v.is_finite()) || z_camera <= 0.0 {
        return None;
    }
    let focal = (width as f64 * 0.5) / (HORIZONTAL_FOV_DEG.to_radians() * 0.5).tan();
    let x = width as f64 * 0.5 + focal * x_camera / z_camera;
    let y = height as f64 * 0.5 - focal * y_camera / z_camera;
    if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 || x >= width as f64 || y >= height as f64 {
        return None;
    }
    Some(StarPixel { x: x.floor() as i32, y: y.floor() as i32 })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(alt_deg: f64, az_deg: f64) -> Option<StarPixel> {
        project_alt_az(alt_deg.to_radians(), az_deg.to_radians(), 960, 600)
    }
    #[test]
    fn viewport_vertical_fov_matches_square_pixel_geometry() {
        assert!((vertical_fov_deg(960, 600) - 64.0107664).abs() < 1e-6);
    }
    #[test]
    fn camera_center_projects_to_viewport_center() {
        assert_eq!(p(30.0, 0.0), Some(StarPixel { x: 480, y: 300 }));
    }
    #[test]
    fn north_horizon_is_below_center_at_expected_perspective_position() {
        let actual = p(0.0, 0.0).unwrap();
        let expected_y = 300.0 + 480.0 * 30_f64.to_radians().tan();
        assert_eq!(actual.x, 480);
        assert_eq!(actual.y, expected_y.floor() as i32);
    }
    #[test]
    fn east_is_right_and_west_is_left() {
        let east = p(30.0, 10.0).unwrap();
        let west = p(30.0, 350.0).unwrap();
        assert!(east.x > 480, "east of north must appear right: {east:?}");
        assert!(west.x < 480, "west of north must appear left: {west:?}");
    }
    #[test]
    fn preserved_zero_roll_keeps_east_west_symmetric_in_y() {
        let east = p(30.0, 10.0).unwrap();
        let west = p(30.0, 350.0).unwrap();
        assert_eq!(east.y, west.y, "zero roll must not tilt equal-altitude east/west points");
    }
    #[test]
    fn north_wraparound_is_continuous_and_centered() {
        let wrapped = p(30.0, 360.0).unwrap();
        assert!((wrapped.x - 480).abs() <= 1 && wrapped.y == 300, "360 degrees must land on camera center: {wrapped:?}");
        let east = p(30.0, 0.001).unwrap();
        let west = p(30.0, 359.999).unwrap();
        assert!(east.x >= 480, "east side of wrap must stay on/right of center: {east:?}");
        assert!(west.x <= 480, "west side of wrap must stay on/left of center: {west:?}");
        assert!((east.x - west.x).abs() <= 2, "wrap must be continuous: east={east:?} west={west:?}");
    }
    #[test]
    fn runtime_camera_moves_and_resets_without_changing_default_projection() {
        let mut camera=Camera::default();
        assert_eq!(project_alt_az_with_camera(30f64.to_radians(),0.0,960,600,camera),
            project_alt_az(30f64.to_radians(),0.0,960,600));
        assert!(camera.apply(ViewControl::Right));
        assert_eq!(camera.azimuth_deg,1.0);
        assert!(camera.apply(ViewControl::Up));
        assert_eq!(camera.altitude_deg,31.0);
        assert!(camera.apply(ViewControl::Reset));
        assert_eq!(camera,Camera::default());
    }
    #[test]
    fn camera_altitude_is_bounded_and_azimuth_wraps() {
        let mut camera=Camera{altitude_deg:90.0,azimuth_deg:0.0};
        assert!(!camera.apply(ViewControl::Up));
        assert!(camera.apply(ViewControl::Left));
        assert_eq!(camera.azimuth_deg,359.0);
        camera.altitude_deg=0.0;
        assert!(!camera.apply(ViewControl::Down));
    }
    #[test]
    fn camera_approach_glides_without_overshoot_and_uses_short_wrap_path() {
        let mut camera=Camera{altitude_deg:30.0,azimuth_deg:358.0};
        let target=Camera{altitude_deg:35.0,azimuth_deg:3.0};
        assert!(camera.approach(target,1.25));
        assert_eq!(camera.altitude_deg,31.25);
        assert!((camera.azimuth_deg-359.25).abs()<1e-9);
        for _ in 0..3 { assert!(camera.approach(target,1.25)); }
        assert_eq!(camera,target);
        assert!(!camera.approach(target,1.25));
    }
    #[test]
    fn signed_azimuth_delta_is_continuous_at_north() {
        assert!((signed_azimuth_delta(358.0,3.0)-5.0).abs()<1e-9);
        assert!((signed_azimuth_delta(3.0,358.0)+5.0).abs()<1e-9);
    }

    #[test]
    fn clipping_rejects_below_horizon_behind_camera_outside_and_nonfinite() {
        assert!(p(-0.001, 0.0).is_none());
        assert!(p(20.0, 180.0).is_none());
        assert!(p(20.0, 90.0).is_none());
        assert!(project_alt_az(f64::NAN, 0.0, 960, 600).is_none());
        assert!(project_alt_az(0.0, f64::INFINITY, 960, 600).is_none());
    }
}
