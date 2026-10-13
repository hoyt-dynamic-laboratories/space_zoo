//! Gentle, bounded reactions. More agency, never more visual intensity.
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crate::world::FPS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit, Helicopter, Dog(i32, i32), Greet, Notice, Space, ReleaseChickens, Chupacabra, Sasquatch,
}

pub struct InputController {
    last_notice: Option<u64>,
    last_greet: Option<u64>,
    last_dog: Option<u64>,
}

impl InputController {
    pub fn new() -> Self {
        Self { last_notice: None, last_greet: None, last_dog: None }
    }

    pub fn handle(&mut self, key: KeyEvent, tick: u64) -> Option<Action> {
        // Raw terminal mode prevents the OS from generating SIGINT itself.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(Action::Quit);
        }
        if key.kind == KeyEventKind::Release { return None; }
        let action = match key.code {
            KeyCode::Esc => Action::Helicopter,
            KeyCode::Left => Action::Dog(-1, 0),
            KeyCode::Right => Action::Dog(1, 0),
            KeyCode::Up => Action::Dog(0, -1),
            KeyCode::Down => Action::Dog(0, 1),
            KeyCode::Enter => Action::Greet,
            KeyCode::Char('1') => Action::Chupacabra,
            KeyCode::Char('2') => Action::Sasquatch,
            KeyCode::Char('c'|'C') if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                Action::ReleaseChickens,
            KeyCode::Char(' ') => Action::Space,
            // An unrecognized numpad/F-key is still a harmless animal key.
            // Ctrl+C remains the only explicit keyboard exit.
            _ => Action::Notice,
        };
        // No holding a key for jackpots. All feedback is capped to a single action.
        let (last, cooldown) = match action {
            Action::Notice => (&mut self.last_notice, FPS/6), // ~167 ms, no escalating reward
            Action::Space => (&mut self.last_notice, FPS/4), // small response
            Action::Greet => (&mut self.last_greet, FPS/2), // bird director handles long Heron visit
            Action::Dog(..) => (&mut self.last_dog, 2),
            Action::ReleaseChickens => (&mut self.last_notice,FPS/2),
            Action::Helicopter | Action::Quit | Action::Chupacabra | Action::Sasquatch => return Some(action),
        };
        if last.is_some_and(|previous| tick.saturating_sub(previous) < cooldown) { return None; }
        *last = Some(tick);
        Some(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ctrl_c_quits_and_escape_summons() {
        let mut input = InputController::new();
        assert_eq!(input.handle(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), 0), Some(Action::Helicopter));
        assert_eq!(input.handle(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL), 0), Some(Action::Quit));
    }
    #[test]
    fn daytime_release_key_is_not_control_c() {
        let mut input=InputController::new();
        assert_eq!(input.handle(KeyEvent::new(KeyCode::Char('c'),KeyModifiers::NONE),0),
            Some(Action::ReleaseChickens));
        assert_eq!(input.handle(KeyEvent::new(KeyCode::Char('C'),KeyModifiers::NONE),1),None);
        assert_eq!(input.handle(KeyEvent::new(KeyCode::Char('c'),KeyModifiers::CONTROL),1),
            Some(Action::Quit));
    }
    #[test]
    fn mash_does_not_stack() {
        let mut input = InputController::new();
        let k = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(input.handle(k, 0), Some(Action::Notice));
        assert_eq!(input.handle(k, 1), None);
        assert_eq!(input.handle(k, FPS/6), Some(Action::Notice));
    }
}
