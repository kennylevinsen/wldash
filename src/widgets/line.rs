use std::default::Default;

use crate::{
    buffer::BufferView,
    color::Color,
    fonts::FontMap,
    widgets::{Geometry, Widget},
};

pub struct Line {
    geometry: Geometry,
    thickness: u32,
    vertical: bool,
}

impl Line {
    pub fn new(thickness: u32, vertical: bool) -> Line {
        Line {
            geometry: Default::default(),
            thickness,
            vertical,
        }
    }
}

impl Widget for Line {
    fn geometry(&self) -> Geometry {
        self.geometry
    }

    fn draw(&mut self, _fonts: &mut FontMap, view: &mut BufferView) -> Geometry {
        view.memset(Color::WHITE);
        self.geometry
    }

    fn geometry_update(&mut self, _fonts: &mut FontMap, geometry: &Geometry) -> Geometry {
        self.geometry = Geometry {
            x: geometry.x,
            y: geometry.y,
            width: if self.vertical {
                self.thickness
            } else {
                geometry.width
            },
            height: if self.vertical {
                geometry.height
            } else {
                self.thickness
            },
        };
        self.geometry
    }

    fn minimum_size(&mut self, _fonts: &mut FontMap) -> Geometry {
        Geometry {
            x: 0,
            y: 0,
            width: if self.vertical { self.thickness } else { 1 },
            height: if self.vertical { 1 } else { self.thickness },
        }
    }
}
