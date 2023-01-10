use std::default::Default;

use crate::{
    buffer::BufferView,
    color::Color,
    fonts::FontMap,
    widgets::{Geometry, Widget},
};

use chrono::{Datelike, Local};

const DATE_FONT: &str = "sans";
const DATE_SIZE: f32 = 40.;

pub struct Date {
    dirty: bool,
    geometry: Geometry,
}

impl Date {
    pub fn new() -> Date {
        Date {
            dirty: true,
            geometry: Default::default(),
        }
    }
}

impl<'a> Widget for Date {
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
        let mut date_line = view.offset((0, 8)).unwrap();
        let font = fonts.get_font(DATE_FONT, DATE_SIZE);
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
        self.geometry
    }

    fn geometry_update(&mut self, _fonts: &mut FontMap, geometry: &Geometry) -> Geometry {
        self.geometry = Geometry {
            x: geometry.x,
            y: geometry.y,
            width: 256,
            height: DATE_SIZE.ceil() as u32,
        };
        self.geometry
    }
}
