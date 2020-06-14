use crate::buffer::Buffer;
use crate::color::Color;
use chrono::NaiveDateTime;
use nix::poll::PollFd;
pub use smithay_client_toolkit::keyboard::{KeyState, ModifiersState};

pub struct DrawContext<'a> {
    pub buf: &'a mut Buffer<'a>,
    pub bg: &'a Color,
    pub time: NaiveDateTime,
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

pub struct WaitContext {
    pub fds: Vec<PollFd>,
    pub target_time: Option<NaiveDateTime>,
}

impl WaitContext {
    pub fn set_time(&mut self, new_time: NaiveDateTime) {
        if let Some(ot) = self.target_time {
            if new_time < ot {
                self.target_time = Some(new_time);
            }
        } else {
            self.target_time = Some(new_time);
        }
    }
}

pub trait Widget {
    fn wait(&mut self, ctx: &mut WaitContext);
    fn enter(&mut self);
    fn leave(&mut self);
    fn size(&self) -> (u32, u32);
    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
        expansion: (u32, u32),
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

pub struct VerticalLayout<'a> {
    pub children: Vec<Box<dyn Widget + Send + 'a>>,
}

#[allow(dead_code)]
impl<'a> VerticalLayout<'a> {
    pub fn new(children: Vec<Box<dyn Widget + Send + 'a>>) -> Box<VerticalLayout> {
        Box::new(VerticalLayout { children })
    }
}

impl<'a> Widget for VerticalLayout<'a> {
    fn wait(&mut self, ctx: &mut WaitContext) {
        for child in &mut self.children {
            child.wait(ctx);
        }
    }
    fn enter(&mut self) {
        for child in &mut self.children {
            child.enter();
        }
    }
    fn leave(&mut self) {
        for child in &mut self.children {
            child.leave();
        }
    }
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
        expansion: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let mut offset = 0;
        let mut width = 0;
        let mut damage = Vec::new();
        let mut full_damage = false;
        for child in &mut self.children {
            let mut report = child.draw(
                ctx,
                (pos.0, offset + pos.1),
                (expansion.0, expansion.1 - offset),
            )?;
            if report.width > width {
                width = report.width
            }
            offset += report.height;
            full_damage |= report.full_damage;
            damage.append(&mut report.damage);
        }

        Ok(DrawReport {
            width,
            height: offset,
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

pub struct HorizontalLayout<'a> {
    pub children: Vec<Box<dyn Widget + Send + 'a>>,
}

#[allow(dead_code)]
impl<'a> HorizontalLayout<'a> {
    pub fn new(children: Vec<Box<dyn Widget + Send + 'a>>) -> Box<HorizontalLayout> {
        Box::new(HorizontalLayout { children })
    }

    fn height(&self) -> u32 {
        let mut height = 0;

        for child in &self.children {
            let size = child.size();
            if size.1 > height {
                height = size.1;
            }
        }

        height
    }
}

impl<'a> Widget for HorizontalLayout<'a> {
    fn wait(&mut self, ctx: &mut WaitContext) {
        for child in &mut self.children {
            child.wait(ctx);
        }
    }
    fn enter(&mut self) {
        for child in &mut self.children {
            child.enter();
        }
    }
    fn leave(&mut self) {
        for child in &mut self.children {
            child.leave();
        }
    }
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
        expansion: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let mut offset = 0;
        let mut height = 0;
        let mut damage = Vec::new();
        let mut full_damage = false;
        for child in &mut self.children {
            let mut report = child.draw(
                ctx,
                (offset + pos.0, pos.1),
                (expansion.0 - offset, expansion.1),
            )?;
            if report.height > height {
                height = report.height
            }
            offset += report.width;
            full_damage |= report.full_damage;
            damage.append(&mut report.damage);
        }

        Ok(DrawReport {
            width: offset,
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

pub struct Margin<'a> {
    pub child: Box<dyn Widget + Send + 'a>,
    pub margins: (u32, u32, u32, u32), // left, right, top, bottom
}

#[allow(dead_code)]
impl<'a> Margin<'a> {
    pub fn new(margins: (u32, u32, u32, u32), child: Box<dyn Widget + Send + 'a>) -> Box<Margin> {
        Box::new(Margin { child, margins })
    }
}

impl<'a> Widget for Margin<'a> {
    fn wait(&mut self, ctx: &mut WaitContext) {
        self.child.wait(ctx)
    }
    fn enter(&mut self) {
        self.child.enter()
    }
    fn leave(&mut self) {
        self.child.leave()
    }
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
        expansion: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let expansion = (
            expansion.0 - self.margins.0 - self.margins.1,
            expansion.1 - self.margins.2 - self.margins.3,
        );
        let report = self.child.draw(
            ctx,
            (pos.0 + self.margins.0, pos.1 + self.margins.2),
            expansion,
        )?;
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

pub struct Fixed<'a> {
    pub child: Box<dyn Widget + Send + 'a>,
    pub size: (u32, u32),
}

#[allow(dead_code)]
impl<'a> Fixed<'a> {
    pub fn new(size: (u32, u32), child: Box<dyn Widget + Send + 'a>) -> Box<Fixed> {
        Box::new(Fixed { child, size })
    }
}

impl<'a> Widget for Fixed<'a> {
    fn wait(&mut self, ctx: &mut WaitContext) {
        self.child.wait(ctx)
    }
    fn enter(&mut self) {
        self.child.enter()
    }
    fn leave(&mut self) {
        self.child.leave()
    }
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
        expansion: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let report = self.child.draw(ctx, pos, expansion)?;
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
