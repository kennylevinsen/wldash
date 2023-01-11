use std::cmp::{max, min};
use std::fmt;

use crate::{buffer::BufferView, fonts::FontMap, keyboard::KeyEvent};

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
}

pub trait Widget {
    fn get_dirty(&self) -> bool {
        false
    }
    fn set_dirty(&mut self, _dirty: bool) {}
    fn geometry(&self) -> Geometry;
    fn geometry_update(&mut self, fonts: &mut FontMap, geometry: &Geometry) -> Geometry;
    fn draw(&mut self, fonts: &mut FontMap, view: &mut BufferView) -> Geometry;
    fn keyboard_input(&mut self, _event: &KeyEvent) {}
    fn token_update(&mut self, _token: &str) {}
}

pub trait Layout<U> {
    fn geometry_update(
        &mut self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        user_data: &mut U,
    ) -> Geometry;
}
pub struct HorizontalLayout<U> {
    pub widgets: Vec<Box<dyn Layout<U>>>,
}

impl<U> Layout<U> for HorizontalLayout<U> {
    fn geometry_update(
        &mut self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        user_data: &mut U,
    ) -> Geometry {
        let mut geo = geometry.clone();
        let mut max_width = 0;
        let mut max_height = 0;
        for w in self.widgets.iter_mut() {
            let result = w.geometry_update(fonts, &geo, user_data);
            geo.x = result.x + result.width;
            geo.width -= result.width;
            max_width += result.width;
            max_height = max(result.height, max_height);
        }
        geo.width = max_width;
        geo.height = max_height;
        geo
    }
}

pub struct VerticalLayout<U> {
    pub widgets: Vec<Box<dyn Layout<U>>>,
}

impl<U> Layout<U> for VerticalLayout<U> {
    fn geometry_update(
        &mut self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        user_data: &mut U,
    ) -> Geometry {
        let mut geo = geometry.clone();
        let mut max_width = 0;
        let mut max_height = 0;
        for w in self.widgets.iter_mut() {
            let result = w.geometry_update(fonts, &geo, user_data);
            geo.y = result.y + result.height;
            geo.height -= result.height;
            max_width = max(result.width, max_width);
            max_height += result.height;
        }
        geo.width = max_width;
        geo.height = max_height;
        geo
    }
}

pub trait WidgetUpdater {
    fn geometry_update(&mut self, idx: usize, fonts: &mut FontMap, geometry: &Geometry)
        -> Geometry;
}

pub struct IndexedLayout {
    pub widget_idx: usize,
}

impl<U: WidgetUpdater> Layout<U> for IndexedLayout {
    fn geometry_update(
        &mut self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        user_data: &mut U,
    ) -> Geometry {
        user_data.geometry_update(self.widget_idx, fonts, geometry)
    }
}

pub struct Margin<U> {
    pub widget: Box<dyn Layout<U>>,
    pub margin: (u32, u32, u32, u32),
}

impl<U> Layout<U> for Margin<U> {
    fn geometry_update(
        &mut self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        user_data: &mut U,
    ) -> Geometry {
        let geo = Geometry {
            x: geometry.x + self.margin.0,
            y: geometry.y + self.margin.1,
            width: geometry.width - self.margin.0 - self.margin.2,
            height: geometry.height - self.margin.1 - self.margin.3,
        };

        let out = self.widget.geometry_update(fonts, &geo, user_data);
        Geometry {
            x: out.x - self.margin.0,
            y: out.y - self.margin.1,
            width: out.width + self.margin.0 + self.margin.2,
            height: out.height + self.margin.1 + self.margin.3,
        }
    }
}
