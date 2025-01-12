use std::cmp::max;

use crate::{fonts::FontMap, state::State, widgets::Geometry};

pub trait Layout {
    fn geometry_update(
        &self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        state: &mut State,
    ) -> Geometry;

    fn minimum_size(&self, fonts: &mut FontMap, state: &mut State) -> Geometry;
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

    fn minimum_size(&self, fonts: &mut FontMap, state: &mut State) -> Geometry {
        let mut max_width = 0;
        let mut max_height = 0;
        for w in self.widgets.iter() {
            let result = w.minimum_size(fonts, state);
            max_width += result.x + result.width;
            max_height = max(result.y + result.height, max_height);
        }
        Geometry {
            width: max_width,
            height: max_height,
            x: 0,
            y: 0,
        }
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

    fn minimum_size(&self, fonts: &mut FontMap, state: &mut State) -> Geometry {
        let mut max_width = 0;
        let mut max_height = 0;
        for w in self.widgets.iter() {
            let result = w.minimum_size(fonts, state);
            max_width = max(result.x + result.width, max_width);
            max_height += result.y + result.height;
        }
        Geometry {
            width: max_width,
            height: max_height,
            x: 0,
            y: 0,
        }
    }
}

pub trait WidgetUpdater {
    fn geometry_update(&mut self, idx: usize, fonts: &mut FontMap, geometry: &Geometry)
        -> Geometry;
    fn minimum_size(&mut self, idx: usize, fonts: &mut FontMap) -> Geometry;
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

    fn minimum_size(&self, fonts: &mut FontMap, state: &mut State) -> Geometry {
        state.minimum_size(self.widget_idx, fonts)
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

    fn minimum_size(&self, fonts: &mut FontMap, state: &mut State) -> Geometry {
        let mut max_width = 0;
        let mut max_height = 0;
        for w in self.widgets.iter() {
            let result = w.minimum_size(fonts, state);
            max_width += result.x + result.width;
            max_height = max(result.y + result.height, max_height);
        }
        Geometry {
            width: max_width,
            height: max_height,
            x: 0,
            y: 0,
        }
    }
}

pub struct InvertedVerticalLayout {
    pub widgets: Vec<Box<dyn Layout>>,
}

impl InvertedVerticalLayout {
    pub fn new(widgets: Vec<Box<dyn Layout>>) -> Box<dyn Layout> {
        Box::new(InvertedVerticalLayout { widgets })
    }
}

impl Layout for InvertedVerticalLayout {
    fn geometry_update(
        &self,
        fonts: &mut FontMap,
        geometry: &Geometry,
        state: &mut State,
    ) -> Geometry {
        let mut geo = geometry.clone();
        let mut max_width = 0;
        for w in self.widgets.iter() {
            let mut temp_geo = geo.clone();
            let temp_result = w.geometry_update(fonts, &temp_geo, state);
            temp_geo.y = geo.y + (geo.height - temp_result.height);
            let result = w.geometry_update(fonts, &temp_geo, state);
            geo.height -= result.height;
            max_width = max(result.width, max_width);
        }
        geo.width = max_width;
        geo
    }

    fn minimum_size(&self, fonts: &mut FontMap, state: &mut State) -> Geometry {
        let mut max_width = 0;
        let mut max_height = 0;
        for w in self.widgets.iter() {
            let result = w.minimum_size(fonts, state);
            max_width = max(result.x + result.width, max_width);
            max_height += result.y + result.height;
        }
        Geometry {
            width: max_width,
            height: max_height,
            x: 0,
            y: 0,
        }
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

    fn minimum_size(&self, fonts: &mut FontMap, state: &mut State) -> Geometry {
        let size = self.widget.minimum_size(fonts, state);
        Geometry {
            width: size.x + size.width + self.margin.0 + self.margin.2,
            height: size.y + size.height + self.margin.1 + self.margin.3,
            x: 0,
            y: 0,
        }
    }
}
