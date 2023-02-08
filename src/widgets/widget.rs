use std::{
    cmp::{max, min},
    fmt,
};

use crate::{buffer::BufferView, event::Event, fonts::FontMap};

#[derive(Debug, Default, Clone, Copy)]
pub struct Geometry {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl fmt::Display for Geometry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "(x: {}, y: {}, width: {}, height: {})",
            self.x, self.y, self.width, self.height
        )
    }
}

impl Geometry {
    pub fn new() -> Geometry {
        Geometry {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    }

    pub fn expand(&self, other: Geometry) -> Geometry {
        let x = min(self.x, other.x);
        let y = min(self.y, other.y);
        Geometry {
            x,
            y,
            width: max(self.width + self.x, other.width + other.x) - x,
            height: max(self.height + self.y, other.height + other.height) - y,
        }
    }

    pub fn contains(&self, position: (u32, u32)) -> bool {
        position.0 >= self.x
            && position.0 < self.x + self.width
            && position.1 >= self.y
            && position.1 < self.y + self.height
    }
}

pub trait Widget {
    fn get_dirty(&self) -> bool {
        false
    }
    fn geometry(&self) -> Geometry;
    fn geometry_update(&mut self, fonts: &mut FontMap, geometry: &Geometry) -> Geometry;
    fn minimum_size(&mut self, _fonts: &mut FontMap) -> Geometry {
        Default::default()
    }
    fn draw(&mut self, fonts: &mut FontMap, view: &mut BufferView) -> Geometry;
    fn event(&mut self, _event: &Event) {}
}
