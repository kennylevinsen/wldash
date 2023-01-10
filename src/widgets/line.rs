use std::default::Default;

use crate::{
    buffer::BufferView,
    color::Color,
    fonts::FontMap,
    widgets::{Geometry, Widget},
};

#[derive(Default)]
pub struct Line {
    geometry: Geometry,
}

impl Line {
    pub fn new() -> Line {
        Default::default()
    }
}

impl Widget for Line {
    fn geometry(&self) -> Geometry {
        self.geometry
    }
    fn draw(&mut self, _fonts: &mut FontMap, view: &mut BufferView) -> Geometry {
        view.memset(&Color::new(1., 1., 1., 1.));
        self.geometry
    }

    fn geometry_update(&mut self, _fonts: &mut FontMap, geometry: &Geometry) -> Geometry {
        self.geometry = Geometry {
            x: geometry.x,
            y: geometry.y,
            width: geometry.width,
            height: 1,
        };
        self.geometry
    }
}
