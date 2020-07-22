use crate::color::Color;
use crate::draw::Font;
use crate::{
    fonts::FontRef,
    widget::{DrawContext, DrawReport, KeyState, ModifiersState, WaitContext, Widget},
};

use chrono::{Datelike, NaiveDateTime};

pub struct Date<'a> {
    cur_time: NaiveDateTime,
    date_cache: Font<'a>,
    size: f32,
    ch_width: u32,
    digit_width: u32,
    spacing_width: u32,
}

impl<'a> Date<'a> {
    pub fn new(time: NaiveDateTime, font: FontRef, size: f32) -> ::std::io::Result<Box<Date>> {
        let mut date_cache = Font::new(font, size);
        let chs = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let digits = "0123456789,/ ";
        let spacings = ",/ ";

        date_cache.add_str_to_cache(chs);
        date_cache.add_str_to_cache(digits);
        date_cache.add_str_to_cache(spacings);
        let ch_width = date_cache.auto_widest(chs)?;
        let digit_width = date_cache.auto_widest(digits)?;
        let spacing_width = date_cache.auto_widest(spacings)?;

        Ok(Box::new(Date {
            cur_time: time,
            date_cache,
            size,
            ch_width,
            digit_width,
            spacing_width,
        }))
    }
}

impl<'a> Widget for Date<'a> {
    fn wait(&mut self, _: &mut WaitContext) {}
    fn enter(&mut self) {}
    fn leave(&mut self) {}
    fn size(&self) -> (u32, u32) {
        (
            (3 * self.ch_width + 8 * self.digit_width + 4 * self.spacing_width) as u32,
            self.size.ceil() as u32,
        )
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
        _expansion: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let (width, height) = self.size();

        if !(ctx.time.date() != self.cur_time.date() || ctx.force) {
            return Ok(DrawReport::empty(width, height));
        }

        self.cur_time = ctx.time;

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
            width,
            height,
            damage: vec![(pos.0 as i32, pos.1 as i32, width as i32, height as i32)],
            full_damage: false,
        })
    }

    fn keyboard_input(&mut self, _: u32, _: ModifiersState, _: KeyState, _: Option<String>) {}
    fn mouse_click(&mut self, _: u32, _: (u32, u32)) {}
    fn mouse_scroll(&mut self, _: (f64, f64), _: (u32, u32)) {}
}
