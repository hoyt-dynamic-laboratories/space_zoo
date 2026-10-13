//! Isolate terminal escape-sequence decoding from the simulation clock.
//! On some Linux consoles, an unusual numpad sequence can leave crossterm's
//! decoder waiting for a terminator. Animals must keep moving regardless.
use std::sync::mpsc::{self,Receiver,TryRecvError};
use crossterm::event::{self,Event};

pub struct Keyboard {rx:Receiver<Event>}
impl Keyboard {
    pub fn new()->Self {
        let (tx,rx)=mpsc::sync_channel(64);
        std::thread::spawn(move || loop {
            match event::read() {
                Ok(event)=>{if tx.send(event).is_err(){break;}},
                Err(error) if error.kind()==std::io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind()==std::io::ErrorKind::UnexpectedEof => break,
                Err(_) => {
                    // An invalid/partial escape sequence must not permanently
                    // kill input. Avoid busy-looping on a sick terminal device.
                    std::thread::sleep(std::time::Duration::from_millis(10));
                },
            }
        });
        Self{rx}
    }
    pub fn try_next(&self)->Option<Event> {
        match self.rx.try_recv() {Ok(e)=>Some(e),Err(TryRecvError::Empty|TryRecvError::Disconnected)=>None}
    }
}
