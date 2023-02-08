use std::sync::{Arc, Mutex};

use calloop::ping::Ping;

use crate::keyboard::KeyEvent;

#[derive(Copy, Clone, Debug)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
    ScrollVertical(f64),
    ScrollHorizontal(f64),
}

#[derive(Debug)]
pub struct PointerEvent {
    pub button: PointerButton,
    pub pos: (u32, u32),
}

#[derive(Debug)]
pub enum Event {
    NewMinute,
    PowerUpdate,
    LauncherUpdate,
    AudioUpdate,
    KeyEvent(KeyEvent),
    PointerEvent(PointerEvent),
    TokenUpdate(String),
}

pub struct Events {
    dirty: bool,
    events: Vec<Event>,
    ping: Ping,
}

impl Events {
    pub fn new(ping: Ping) -> Arc<Mutex<Events>> {
        Arc::new(Mutex::new(Events {
            dirty: false,
            events: Vec::new(),
            ping,
        }))
    }

    pub fn add_event(&mut self, ev: Event) {
        self.events.push(ev);
        if !self.dirty {
            self.dirty = true;
            self.ping.ping();
        }
    }

    pub fn flush(&mut self) -> Vec<Event> {
        self.dirty = false;
        self.events.drain(..).collect()
    }
}
