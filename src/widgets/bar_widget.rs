use std::{cmp::min, default::Default};

use crate::{
    buffer::BufferView,
    color::Color,
    draw::{draw_bar, draw_box},
    fonts::FontMap,
    widgets::{Geometry, Widget},
};

pub trait BarWidgetImpl {
    fn get_dirty(&self) -> bool;
    fn set_dirty(&mut self, dirty: bool);
    fn name(&self) -> &'static str;
    fn value(&self) -> f32;
    fn color(&self) -> Color;
}

pub struct BarWidget {
    font: &'static str,
    size: f32,
    geometry: Geometry,
    inner_widget: Box<dyn BarWidgetImpl>,
}

impl BarWidget {
    pub fn new(inner_widget: Box<dyn BarWidgetImpl>, font: &'static str, size: f32) -> BarWidget {
        BarWidget {
            geometry: Default::default(),
            inner_widget, font, size,
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

    fn set_dirty(&mut self, dirty: bool) {
        self.inner_widget.set_dirty(dirty)
    }

    fn draw(&mut self, fonts: &mut FontMap, view: &mut BufferView) -> Geometry {
        let font = fonts.get_font(self.font, self.size);
        let fg = Color::new(1., 1., 1., 1.);
        let bg = Color::new(0., 0., 0., 1.);

        font.auto_draw_text(view, &bg, &fg, self.inner_widget.name())
            .unwrap();
        let size = self.size.ceil() as u32;
        let bar_offset = 4 * size;
        let val = self.inner_widget.value();

        let c = self.inner_widget.color();
        draw_bar(
            &mut view.offset((bar_offset, 0)).unwrap(),
            &c,
            self.geometry.width - bar_offset,
            size,
            val,
        )
        .unwrap();

        draw_box(
            &mut view.offset((bar_offset, 0)).unwrap(),
            &c,
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
}
