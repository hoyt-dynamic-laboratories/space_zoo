//! Isolate terminal escape-sequence decoding from the simulation clock.
//! On some Linux consoles, an unusual numpad sequence can leave crossterm's
//! decoder waiting for a terminator. Animals must keep moving regardless.
//! Terminal failure, however, must not leave an unquittable VT_PROCESS session.
use std::io;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use crossterm::event::{self, Event};

const MAX_CONSECUTIVE_ERRORS: usize = 8;

type KeyboardMessage = io::Result<Event>;

pub struct Keyboard { rx: Receiver<KeyboardMessage> }
impl Keyboard {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::sync_channel(64);
        std::thread::spawn(move || {
            let mut failures = 0;
            loop {
                match event::read() {
                    Ok(event) => {
                        failures = 0;
                        if tx.send(Ok(event)).is_err() { break; }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) if matches!(error.kind(),
                        io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe | io::ErrorKind::NotConnected) => {
                        let _ = tx.send(Err(error));
                        break;
                    }
                    Err(error) => {
                        failures += 1;
                        if failures >= MAX_CONSECUTIVE_ERRORS {
                            let _ = tx.send(Err(io::Error::new(error.kind(),
                                format!("keyboard input failed {failures} consecutive times: {error}"))));
                            break;
                        }
                        // Ignore an occasional malformed sequence, but never
                        // spin indefinitely on a persistently broken device.
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
            }
        });
        Self { rx }
    }

    /// Ok(None) means no event *yet*. EOF / disconnection are errors, not idle.
    pub fn try_next(&self) -> io::Result<Option<Event>> {
        match self.rx.try_recv() {
            Ok(Ok(event)) => Ok(Some(event)),
            Ok(Err(error)) => Err(error),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe, "keyboard reader disconnected")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_queue_is_not_a_disconnected_reader() {
        let (_tx, rx) = mpsc::sync_channel(1);
        let keyboard = Keyboard { rx };
        assert!(matches!(keyboard.try_next(), Ok(None)));
    }

    #[test]
    fn disconnected_reader_is_an_error() {
        let (tx, rx) = mpsc::sync_channel(1);
        drop(tx);
        let keyboard = Keyboard { rx };
        assert_eq!(keyboard.try_next().unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn reader_error_survives_channel() {
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send(Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EOF"))).unwrap();
        let keyboard = Keyboard { rx };
        assert_eq!(keyboard.try_next().unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }
}
