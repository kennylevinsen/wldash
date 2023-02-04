use std::cmp::max;

use crate::{
    fonts::FontMap,
    state::State,
    widgets::{Geometry},
};

pub trait Layout {
    fn geometry_update(
        &self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        state: &mut State,
    ) -> Geometry;
}

pub struct HorizontalLayout {
    pub widgets: Vec<Box<dyn Layout>>,
}

impl HorizontalLayout {
    pub fn new(widgets: Vec<Box<dyn Layout>>) -> Box<dyn Layout> {
        Box::new(HorizontalLayout { widgets })
    }
}

impl Layout for HorizontalLayout {
    fn geometry_update(
        &self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        state: &mut State,
    ) -> Geometry {
        let mut geo = geometry.clone();
        let mut max_width = 0;
        let mut max_height = 0;
        for w in self.widgets.iter() {
            let result = w.geometry_update(fonts, &geo, state);
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

pub struct VerticalLayout {
    pub widgets: Vec<Box<dyn Layout>>,
}

impl VerticalLayout {
    pub fn new(widgets: Vec<Box<dyn Layout>>) -> Box<dyn Layout> {
        Box::new(VerticalLayout { widgets })
    }
}

impl Layout for VerticalLayout {
    fn geometry_update(
        &self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        state: &mut State,
    ) -> Geometry {
        let mut geo = geometry.clone();
        let mut max_width = 0;
        let mut max_height = 0;
        for w in self.widgets.iter() {
            let result = w.geometry_update(fonts, &geo, state);
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

impl IndexedLayout {
    pub fn new(widget_idx: usize) -> Box<dyn Layout> {
        Box::new(IndexedLayout { widget_idx })
    }
}

impl Layout for IndexedLayout {
    fn geometry_update(
        &self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        state: &mut State,
    ) -> Geometry {
        state.geometry_update(self.widget_idx, fonts, geometry)
    }
}

pub struct Margin {
    pub widget: Box<dyn Layout>,
    pub margin: (u32, u32, u32, u32),
}

impl Margin {
    pub fn new(widget: Box<dyn Layout>, margin: (u32, u32, u32, u32)) -> Box<dyn Layout> {
        Box::new(Margin { widget, margin })
    }
}

impl Layout for Margin {
    fn geometry_update(
        &self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        state: &mut State,
    ) -> Geometry {
        let geo = Geometry {
            x: geometry.x + self.margin.0,
            y: geometry.y + self.margin.1,
            width: geometry.width - self.margin.0 - self.margin.2,
            height: geometry.height - self.margin.1 - self.margin.3,
        };
        let out = self.widget.geometry_update(fonts, &geo, state);
        Geometry {
            x: out.x - self.margin.0,
            y: out.y - self.margin.1,
            width: out.width + self.margin.0 + self.margin.2,
            height: out.height + self.margin.1 + self.margin.3,
        }
    }
}

pub struct InvertedHorizontalLayout {
    pub widgets: Vec<Box<dyn Layout>>,
}

impl InvertedHorizontalLayout {
    pub fn new(widgets: Vec<Box<dyn Layout>>) -> Box<dyn Layout> {
        Box::new(InvertedHorizontalLayout { widgets })
    }
}

impl Layout for InvertedHorizontalLayout {
    fn geometry_update(
        &self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        state: &mut State,
    ) -> Geometry {
        let mut geo = geometry.clone();
        let mut max_height = 0;
        for w in self.widgets.iter() {
            let mut temp_geo = geo.clone();
            let temp_result = w.geometry_update(fonts, &temp_geo, state);
            temp_geo.x = geo.x + (geo.width - temp_result.width);
            let result = w.geometry_update(fonts, &temp_geo, state);
            geo.width -= result.width;
            max_height = max(result.height, max_height);
        }
        geo.height = max_height;
        geo
    }
}