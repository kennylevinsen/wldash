use crate::buffer::Buffer;
use crate::color::Color;

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local};

#[derive(Clone)]
pub enum Input {
    Click { pos: (u32, u32), button: u32 },
    Scroll { pos: (u32, u32), x: f64, y: f64 },
    Keypress { key: u32, interpreted: Option<String> },
}

impl Input {
    pub fn offset(&self, offset: (u32, u32)) -> Input {
        match self {
            Input::Click { pos, button } => Input::Click {
                pos: (pos.0 - offset.0, pos.1 - offset.1),
                button: *button,
            },
            Input::Scroll { pos, x, y } => Input::Scroll {
                pos: (pos.0 - offset.0, pos.1 - offset.1),
                x: *x,
                y: *y,
            },
            Input::Keypress { key, interpreted } => Input::Keypress { key: *key, interpreted: interpreted.clone() }, 
        }
    }
}

pub trait ModuleImpl {
    fn draw(
        &self,
        buf: &mut Buffer,
        bg: &Color,
        time: &DateTime<Local>,
    ) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error>;
    fn update(&mut self, time: &DateTime<Local>, force: bool) -> Result<bool, ::std::io::Error>;
    fn input(&mut self, input: Input);
}

pub struct Module {
    m: Arc<Mutex<Box<dyn ModuleImpl>>>,
    pos: (u32, u32, u32, u32),
}

impl Module {
    pub fn new(m: Box<dyn ModuleImpl>, pos: (u32, u32, u32, u32)) -> Module {
        Module {
            m: Arc::new(Mutex::new(m)),
            pos: pos,
        }
    }

    pub fn get_bounds(&self) -> (u32, u32, u32, u32) {
        self.pos
    }

    pub fn update(&self, time: &DateTime<Local>, force: bool) -> Result<bool, ::std::io::Error> {
        self.m.lock().unwrap().update(time, force)
    }

    pub fn draw(
        &self,
        buf: &mut Buffer,
        bg: &Color,
        time: &DateTime<Local>,
    ) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error> {
        self.m.lock().unwrap().draw(buf, bg, time)
    }

    pub fn intersect(&self, pos: (u32, u32)) -> bool {
        pos.0 >= self.pos.0
            && pos.0 < (self.pos.0 + self.pos.2)
            && pos.1 >= self.pos.1
            && pos.1 < (self.pos.1 + self.pos.3)
    }

    pub fn input(&self, input: Input) {
        self.m.lock().unwrap().input(input)
    }
}
