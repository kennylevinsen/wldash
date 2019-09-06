use crate::buffer::Buffer;
use crate::color::Color;
use chrono::{DateTime, Local};
pub use smithay_client_toolkit::keyboard::{KeyState, ModifiersState};

pub struct DrawContext<'a> {
    pub buf: &'a mut Buffer<'a>,
    pub bg: &'a Color,
    pub time: &'a DateTime<Local>,
    pub force: bool,
}

#[derive(Debug)]
pub struct DrawReport {
    pub width: u32,
    pub height: u32,
    pub damage: Vec<(i32, i32, i32, i32)>,
    pub full_damage: bool,
}

impl DrawReport {
    pub fn empty(width: u32, height: u32) -> DrawReport {
        DrawReport {
            width,
            height,
            damage: Vec::new(),
            full_damage: false,
        }
    }
}

pub trait Widget {
    fn size(&self) -> (u32, u32);
    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error>;

    fn keyboard_input(
        &mut self,
        keysym: u32,
        modifier_state: ModifiersState,
        key_state: KeyState,
        interpreted: Option<String>,
    );
    fn mouse_click(&mut self, button: u32, pos: (u32, u32));
    fn mouse_scroll(&mut self, scroll: (f64, f64), pos: (u32, u32));
}

pub struct VerticalLayout {
    pub children: Vec<Box<dyn Widget + Send>>,
}

#[allow(dead_code)]
impl VerticalLayout {
    pub fn new(children: Vec<Box<dyn Widget + Send>>) -> Box<VerticalLayout> {
        Box::new(VerticalLayout { children: children })
    }
}

impl Widget for VerticalLayout {
    fn size(&self) -> (u32, u32) {
        let mut width = 0;
        let mut height = 0;

        for child in &self.children {
            let size = child.size();
            if size.0 > width {
                width = size.0;
            }
            height += size.1;
        }

        (width, height)
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let mut offset = pos.1;
        let mut width = 0;
        let mut damage = Vec::new();
        let mut full_damage = false;

        for child in &mut self.children {
            let mut report = child.draw(ctx, (pos.0, offset))?;
            if report.width > width {
                width = report.width;
            }
            offset += report.height;
            full_damage |= report.full_damage;
            damage.append(&mut report.damage);
        }

        Ok(DrawReport {
            width,
            height: offset - pos.1,
            damage,
            full_damage,
        })
    }

    fn keyboard_input(
        &mut self,
        keysym: u32,
        modifier_state: ModifiersState,
        key_state: KeyState,
        interpreted: Option<String>,
    ) {
        for child in &mut self.children {
            child.keyboard_input(keysym, modifier_state, key_state, interpreted.clone());
        }
    }

    fn mouse_click(&mut self, button: u32, pos: (u32, u32)) {
        let mut height = 0;

        for child in &mut self.children {
            let size = child.size();
            if pos.1 >= height && pos.1 < height + size.1 {
                let pos = (pos.0, pos.1 - height);
                child.mouse_click(button, pos);
                return;
            }
            height += size.1;
        }
    }

    fn mouse_scroll(&mut self, scroll: (f64, f64), pos: (u32, u32)) {
        let mut height = 0;

        for child in &mut self.children {
            let size = child.size();
            if pos.1 >= height && pos.1 < height + size.1 {
                let pos = (pos.0, pos.1 - height);
                child.mouse_scroll(scroll, pos);
                return;
            }
            height += size.1;
        }
    }
}

pub struct HorizontalLayout {
    pub children: Vec<Box<dyn Widget + Send>>,
}

#[allow(dead_code)]
impl HorizontalLayout {
    pub fn new(children: Vec<Box<dyn Widget + Send>>) -> Box<HorizontalLayout> {
        Box::new(HorizontalLayout { children: children })
    }
}

impl Widget for HorizontalLayout {
    fn size(&self) -> (u32, u32) {
        let mut width = 0;
        let mut height = 0;

        for child in &self.children {
            let size = child.size();
            if size.1 > height {
                height = size.1;
            }
            width += size.0;
        }

        (width, height)
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let mut offset = pos.0;
        let mut height = 0;
        let mut damage = Vec::new();
        let mut full_damage = false;

        for child in &mut self.children {
            let mut report = child.draw(ctx, (offset, pos.1))?;
            if report.height > height {
                height = report.height;
            }
            offset += report.width;
            full_damage |= report.full_damage;
            damage.append(&mut report.damage);
        }

        Ok(DrawReport {
            width: offset - pos.0,
            height,
            damage,
            full_damage,
        })
    }

    fn keyboard_input(
        &mut self,
        keysym: u32,
        modifier_state: ModifiersState,
        key_state: KeyState,
        interpreted: Option<String>,
    ) {
        for child in &mut self.children {
            child.keyboard_input(keysym, modifier_state, key_state, interpreted.clone());
        }
    }

    fn mouse_click(&mut self, button: u32, pos: (u32, u32)) {
        let mut width = 0;

        for child in &mut self.children {
            let size = child.size();
            if pos.0 >= width && pos.0 < width + size.0 {
                let pos = (pos.0 - width, pos.1);
                child.mouse_click(button, pos);
                return;
            }

            width += size.0;
        }
    }

    fn mouse_scroll(&mut self, scroll: (f64, f64), pos: (u32, u32)) {
        let mut width = 0;

        for child in &mut self.children {
            let size = child.size();
            if pos.0 >= width && pos.0 < width + size.0 {
                let pos = (pos.0 - width, pos.1);
                child.mouse_scroll(scroll, pos);
                return;
            }

            width += size.0;
        }
    }
}

pub struct Margin {
    pub child: Box<dyn Widget + Send>,
    pub margins: (u32, u32, u32, u32), // left, right, top, bottom
}

#[allow(dead_code)]
impl Margin {
    pub fn new(margins: (u32, u32, u32, u32), child: Box<dyn Widget + Send>) -> Box<Margin> {
        Box::new(Margin {
            child: child,
            margins: margins,
        })
    }
}

impl Widget for Margin {
    fn size(&self) -> (u32, u32) {
        let size = self.child.size();
        (
            size.0 + self.margins.0 + self.margins.1,
            size.1 + self.margins.2 + self.margins.3,
        )
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let report = self
            .child
            .draw(ctx, (pos.0 + self.margins.0, pos.1 + self.margins.2))?;
        Ok(DrawReport {
            width: report.width + self.margins.0 + self.margins.1,
            height: report.height + self.margins.2 + self.margins.3,
            damage: report.damage,
            full_damage: report.full_damage,
        })
    }

    fn keyboard_input(
        &mut self,
        keysym: u32,
        modifier_state: ModifiersState,
        key_state: KeyState,
        interpreted: Option<String>,
    ) {
        self.child
            .keyboard_input(keysym, modifier_state, key_state, interpreted);
    }

    fn mouse_click(&mut self, button: u32, pos: (u32, u32)) {
        let pos = (
            pos.0.saturating_sub(self.margins.0),
            pos.1.saturating_sub(self.margins.2),
        );
        self.child.mouse_click(button, pos);
    }

    fn mouse_scroll(&mut self, scroll: (f64, f64), pos: (u32, u32)) {
        let pos = (
            pos.0.saturating_sub(self.margins.0),
            pos.1.saturating_sub(self.margins.2),
        );
        self.child.mouse_scroll(scroll, pos);
    }
}

pub struct Fixed {
    pub child: Box<dyn Widget + Send>,
    pub size: (u32, u32),
}

#[allow(dead_code)]
impl Fixed {
    pub fn new(size: (u32, u32), child: Box<dyn Widget + Send>) -> Box<Fixed> {
        Box::new(Fixed {
            child: child,
            size: size,
        })
    }
}

impl Widget for Fixed {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let report = self.child.draw(ctx, pos)?;
        Ok(DrawReport {
            width: self.size.0,
            height: self.size.1,
            damage: report.damage,
            full_damage: report.full_damage,
        })
    }

    fn keyboard_input(
        &mut self,
        keysym: u32,
        modifier_state: ModifiersState,
        key_state: KeyState,
        interpreted: Option<String>,
    ) {
        self.child
            .keyboard_input(keysym, modifier_state, key_state, interpreted);
    }

    fn mouse_click(&mut self, button: u32, pos: (u32, u32)) {
        self.child.mouse_click(button, pos);
    }

    fn mouse_scroll(&mut self, scroll: (f64, f64), pos: (u32, u32)) {
        self.child.mouse_scroll(scroll, pos);
    }
}
