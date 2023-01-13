use std::default::Default;

use crate::{
    buffer::BufferView,
    color::Color,
    fonts::FontMap,
    widgets::{Geometry, Widget},
};

use chrono::{Local, Timelike};

pub struct Clock {
    font: &'static str,
    size: f32,
    dirty: bool,
    geometry: Geometry,
    digit_width: u32,
    colon_width: u32,
}

impl<'a> Clock {
    pub fn new(font: &'static str, size: f32) -> Clock {
        Clock {
            font, size,
            dirty: true,
            geometry: Default::default(),
            digit_width: 0,
            colon_width: 0,
        }
    }
}

impl<'a> Widget for Clock {
    fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }
    fn get_dirty(&self) -> bool {
        self.dirty
    }

    fn geometry(&self) -> Geometry {
        self.geometry
    }

    fn draw(&mut self, fonts: &mut FontMap, view: &mut BufferView) -> Geometry {
        let time = Local::now().naive_local();
        let fg = Color::new(1., 1., 1., 1.);
        let bg = Color::new(0., 0., 0., 1.);

        let digit = self.digit_width;
        let colon = self.colon_width;
        let font = fonts.get_font(self.font, self.size);
        font.draw_text_fixed_width(
            view,
            &bg,
            &fg,
            &[digit, digit, colon, digit, digit],
            &format!("{:02}:{:02}", time.hour(), time.minute()),
        )
        .unwrap();
        self.geometry
    }

    fn geometry_update(&mut self, fonts: &mut FontMap, geometry: &Geometry) -> Geometry {
        let font = fonts.get_font(self.font, self.size);
        self.digit_width = font.auto_widest("0123456789").unwrap();
        self.colon_width = font.auto_widest(":").unwrap();
        self.geometry = Geometry {
            x: geometry.x,
            y: geometry.y,
            width: self.digit_width * 4 + self.colon_width,
            height: self.size.ceil() as u32,
        };
        self.geometry
    }
}
