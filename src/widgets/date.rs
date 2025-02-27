use std::default::Default;

use crate::{
    buffer::BufferView,
    color::Color,
    event::Event,
    fonts::FontMap,
    widgets::{Geometry, Widget},
};

use chrono::{Datelike, Local};

pub struct Date {
    font: &'static str,
    size: f32,
    dirty: bool,
    geometry: Geometry,
}

impl Date {
    pub fn new(fm: &mut FontMap, font: &'static str, size: f32) -> Date {
        fm.queue_font(font, size, "0123456789/, adehinortuFMSTW");
        Date {
            font,
            size,
            dirty: true,
            geometry: Default::default(),
        }
    }
}

impl<'a> Widget for Date {
    fn get_dirty(&self) -> bool {
        self.dirty
    }

    fn geometry(&self) -> Geometry {
        self.geometry
    }

    fn event(&mut self, event: &Event) {
        match event {
            Event::NewMinute => {
                self.dirty = true;
            }
            _ => (),
        }
    }

    fn draw(&mut self, fonts: &mut FontMap, view: &mut BufferView) -> Geometry {
        let time = Local::now().naive_local();
        let fg = Color::WHITE;
        let font = fonts.get_font(self.font, self.size);
        font.draw_text(
            view,
            fg,
            &format!(
                "{}, {:02}/{:02}/{:4}",
                time.weekday(),
                time.day(),
                time.month(),
                time.year()
            ),
        )
        .unwrap();
        self.dirty = false;
        self.geometry
    }

    fn geometry_update(&mut self, fonts: &mut FontMap, geometry: &Geometry) -> Geometry {
        let width = (3. + 2. + 2. + 2. + 4.) * self.size / 2.;
        let font = fonts.get_font(self.font, self.size);
        self.geometry = Geometry {
            x: geometry.x,
            y: geometry.y,
            width: width.ceil() as u32,
            height: font.height().ceil() as u32,
        };
        self.geometry
    }

    fn minimum_size(&mut self, fonts: &mut FontMap) -> Geometry {
        let width = (3. + 2. + 2. + 2. + 4.) * self.size / 2.;
        let font = fonts.get_font(self.font, self.size);
        Geometry {
            x: 0,
            y: 0,
            width: width.ceil() as u32,
            height: font.height().ceil() as u32,
        }
    }
}
