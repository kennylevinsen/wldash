use crate::color::Color;
use crate::draw::{Font, ROBOTO_REGULAR};
use crate::widget::{DrawContext, DrawReport, KeyState, ModifiersState, Widget, WaitContext};

use chrono::{DateTime, Datelike, Local};

pub struct Date {
    cur_time: DateTime<Local>,
    date_cache: Font,
    size: f32,
}

impl Date {
    pub fn new(size: f32) -> Box<Date> {
        let mut date_cache = Font::new(&ROBOTO_REGULAR, size);
        date_cache
            .add_str_to_cache("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789,/ ");

        let time = Local::now();

        Box::new(Date {
            cur_time: time.with_year(time.year().saturating_sub(1)).unwrap(),
            date_cache: date_cache,
            size: size,
        })
    }
}

impl Widget for Date {
    fn wait(&self, _: &mut WaitContext) {}
    fn enter(&mut self) {}
    fn leave(&mut self) {}
    fn size(&self) -> (u32, u32) {
        ((6.5 * self.size).ceil() as u32, self.size.ceil() as u32)
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let (width, height) = self.size();

        if !(ctx.time.date() != self.cur_time.date() || ctx.force) {
            return Ok(DrawReport::empty(width, height));
        }

        self.cur_time = ctx.time.clone();

        let buf = &mut ctx.buf.subdimensions((pos.0, pos.1, width, height))?;
        buf.memset(ctx.bg);
        self.date_cache.draw_text(
            buf,
            ctx.bg,
            &Color::new(1.0, 1.0, 1.0, 1.0),
            &format!(
                "{:?}, {:02}/{:02}/{:4}",
                ctx.time.weekday(),
                ctx.time.day(),
                ctx.time.month(),
                ctx.time.year()
            ),
        )?;

        Ok(DrawReport {
            width: width,
            height: height,
            damage: vec![(pos.0 as i32, pos.1 as i32, width as i32, height as i32)],
            full_damage: false,
        })
    }

    fn keyboard_input(&mut self, _: u32, _: ModifiersState, _: KeyState, _: Option<String>) {}
    fn mouse_click(&mut self, _: u32, _: (u32, u32)) {}
    fn mouse_scroll(&mut self, _: (f64, f64), _: (u32, u32)) {}
}
