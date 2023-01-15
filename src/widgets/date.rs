use std::default::Default;

use crate::{
    buffer::BufferView,
    color::Color,
    fonts::FontMap,
    state::Event,
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
    pub fn new(font: &'static str, size: f32) -> Date {
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
        let fg = Color::new(1., 1., 1., 1.);
        let bg = Color::new(0., 0., 0., 1.);
        let mut date_line = view.offset((0, 8)).unwrap();
        let font = fonts.get_font(self.font, self.size);
        font.auto_draw_text(
            &mut date_line,
            &bg,
            &fg,
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

    fn geometry_update(&mut self, _fonts: &mut FontMap, geometry: &Geometry) -> Geometry {
        let width = (3. + 2. + 2. + 2. + 4.) * self.size / 2.;
        self.geometry = Geometry {
            x: geometry.x,
            y: geometry.y,
            width: width.ceil() as u32,
            height: self.size.ceil() as u32,
        };
        self.geometry
    }
}
