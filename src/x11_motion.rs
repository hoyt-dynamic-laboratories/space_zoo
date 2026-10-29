//! Current X11 movement state, independent of minifb's frame-delayed event cache.
//! Command text still belongs to minifb/the launcher; this connection selects no
//! events, grabs no keys, and never queues movement or integrates missed frames.
use minifb::{Key, Window};
use x11_dl::xlib;

const KEYS: [Key; 9] = [Key::W, Key::S, Key::A, Key::D, Key::Up, Key::Down,
    Key::Left, Key::Right, Key::R];
// Unshifted symbols, matching the existing minifb controls, then Ctrl and Esc.
const SYMBOLS: [u64; 12] = [0x77, 0x73, 0x61, 0x64, 0xff52, 0xff54,
    0xff51, 0xff53, 0x72, 0xffe3, 0xffe4, 0xff1b];

#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub available: bool,
    down: [bool; 9],
    pressed: [bool; 9],
}
impl Snapshot {
    pub fn down(&self, key: Key) -> bool {
        KEYS.iter().position(|&k| k == key).is_some_and(|i| self.down[i])
    }
    pub fn pressed(&self, key: Key) -> bool {
        KEYS.iter().position(|&k| k == key).is_some_and(|i| self.pressed[i])
    }
}

#[derive(Default)]
struct Edges {
    previous: [bool; 9],
    inhibited: [bool; 9],
}
impl Edges {
    fn sample(&mut self, down: [bool; 9], available: bool) -> Snapshot {
        if !available {
            // A key held/typed in the menu must be released before it can steer.
            self.inhibited = down;
            self.previous = [false; 9];
            return Snapshot::default();
        }
        let mut result = Snapshot { available: true, ..Snapshot::default() };
        for i in 0..9 {
            self.inhibited[i] &= down[i];
            result.down[i] = down[i] && !self.inhibited[i];
            result.pressed[i] = result.down[i] && !self.previous[i];
        }
        self.previous = result.down;
        result
    }
}

pub struct X11Motion {
    x: xlib::Xlib,
    display: *mut xlib::Display,
    window: xlib::Window,
    codes: [u8; 12],
    edges: Edges,
}
impl X11Motion {
    pub fn new(window: &Window) -> Result<Self, String> {
        let x = xlib::Xlib::open().map_err(|e| format!("X11 movement: {e}"))?;
        let display = unsafe { (x.XOpenDisplay)(std::ptr::null()) };
        if display.is_null() { return Err("X11 movement: cannot open display".into()); }
        let codes = SYMBOLS.map(|symbol| unsafe { (x.XKeysymToKeycode)(display, symbol as xlib::KeySym) });
        Ok(Self { x, display, window: window.get_window_handle() as usize as xlib::Window,
            codes, edges: Edges::default() })
    }
    pub fn sample(&mut self, enabled: bool) -> Snapshot {
        let mut map = [0 as std::ffi::c_char; 32];
        let mut focus: xlib::Window = 0;
        let mut revert = 0;
        unsafe {
            (self.x.XQueryKeymap)(self.display, map.as_mut_ptr());
            (self.x.XGetInputFocus)(self.display, &mut focus, &mut revert);
        }
        let down = |i: usize| {
            let code = self.codes[i] as usize;
            code != 0 && (map[code / 8] as u8 & (1 << (code % 8))) != 0
        };
        let available = enabled && focus == self.window && !down(9) && !down(10) && !down(11);
        self.edges.sample(std::array::from_fn(|i| down(i)), available)
    }
}
impl Drop for X11Motion {
    fn drop(&mut self) { unsafe { (self.x.XCloseDisplay)(self.display); } }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(i: usize) -> [bool; 9] { let mut keys = [false; 9]; keys[i] = true; keys }
    #[test]
    fn long_holds_do_not_queue_steps_and_release_is_zero() {
        for i in 0..8 {
            let mut edges = Edges::default();
            assert!(edges.sample(key(i), true).pressed[i]);
            for _ in 0..10_000 {
                let s = edges.sample(key(i), true);
                assert!(s.down[i]); assert!(!s.pressed[i]);
            }
            let released = edges.sample([false; 9], true);
            assert_eq!(released.down, [false; 9]);
            assert_eq!(released.pressed, [false; 9]);
        }
    }
    #[test]
    fn menu_and_focus_loss_stop_then_require_release() {
        for i in 0..9 {
            let mut edges = Edges::default();
            edges.sample(key(i), true);
            assert!(!edges.sample(key(i), false).available);
            assert!(!edges.sample(key(i), true).down[i]);
            edges.sample([false; 9], true);
            assert!(edges.sample(key(i), true).pressed[i]);
        }
    }
    #[test]
    fn release_elsewhere_cannot_resume_after_focus_return() {
        let mut edges = Edges::default();
        edges.sample(key(3), true);
        edges.sample(key(3), false);
        let returned = edges.sample([false; 9], true);
        assert_eq!(returned.down, [false; 9]);
        assert_eq!(returned.pressed, [false; 9]);
    }
    #[test]
    fn diagonal_keys_release_independently_and_taps_rearm() {
        let mut edges = Edges::default();
        let mut diagonal = key(0); diagonal[3] = true;
        let s = edges.sample(diagonal, true);
        assert!(s.pressed[0] && s.pressed[3]);
        let s = edges.sample(key(3), true);
        assert!(!s.down[0] && s.down[3] && !s.pressed[3]);
        edges.sample([false; 9], true);
        assert!(edges.sample(key(3), true).pressed[3]);
    }
}
