use std::{cmp::min, default::Default};

use crate::{
    buffer::BufferView,
    color::Color,
    draw::{draw_bar, draw_box},
    event::{Event, PointerButton},
    fonts::FontMap,
    widgets::{Geometry, Widget},
};

pub trait BarWidgetImpl {
    fn get_dirty(&self) -> bool {
        false
    }
    fn name(&self) -> &'static str;
    fn value(&mut self) -> f32;
    fn color(&self) -> Color;
    fn click(&mut self, _pos: f32, _btn: PointerButton) {}
}

pub struct BarWidget {
    font: &'static str,
    size: f32,
    geometry: Geometry,
    inner_widget: Box<dyn BarWidgetImpl>,
}

impl BarWidget {
    pub fn new(
        inner_widget: Box<dyn BarWidgetImpl>,
        fm: &mut FontMap,
        font: &'static str,
        size: f32,
    ) -> BarWidget {
        fm.queue_font(font, size, inner_widget.name());
        BarWidget {
            geometry: Default::default(),
            inner_widget,
            font,
            size,
        }
    }
}

impl Widget for BarWidget {
    fn geometry(&self) -> Geometry {
        self.geometry
    }

    fn get_dirty(&self) -> bool {
        self.inner_widget.get_dirty()
    }

    fn draw(&mut self, fonts: &mut FontMap, view: &mut BufferView) -> Geometry {
        let font = fonts.get_font(self.font, self.size);
        let fg = Color::WHITE;

        font.draw_text(view, fg, self.inner_widget.name()).unwrap();
        let size = self.size.ceil() as u32;
        let bar_offset = 4 * size;
        let val = self.inner_widget.value();

        let c = self.inner_widget.color();
        draw_bar(
            &mut view.offset((bar_offset, 0)),
            c,
            self.geometry.width - bar_offset,
            size,
            val,
        )
        .unwrap();

        draw_box(
            &mut view.offset((bar_offset, 0)),
            c,
            (self.geometry.width - bar_offset, size),
        )
        .unwrap();
        self.geometry
    }

    fn geometry_update(&mut self, _fonts: &mut FontMap, geometry: &Geometry) -> Geometry {
        let w = min(geometry.width, 768);
        let mut x = geometry.x;
        if geometry.width > w {
            x += geometry.width - w;
        }
        self.geometry = Geometry {
            x: x,
            y: geometry.y,
            width: w,
            height: self.size.ceil() as u32,
        };
        self.geometry
    }

    fn minimum_size(&mut self, _fonts: &mut FontMap) -> Geometry {
        Geometry {
            x: 0,
            y: 0,
            width: self.size.ceil() as u32 * 6,
            height: self.size.ceil() as u32,
        }
    }

    fn event(&mut self, event: &Event) {
        match event {
            Event::PointerEvent(ev) => {
                let offset = 4 * self.size.ceil() as u32;
                if ev.pos.0 >= offset {
                    let val = (ev.pos.0 - offset) as f32 / (self.geometry.width - offset) as f32;
                    self.inner_widget.click(val, ev.button);
                }
            }
            _ => (),
        }
    }
}
