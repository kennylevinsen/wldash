use crate::color::Color;
use crate::draw::Font;
use crate::{
    fonts::FontRef,
    widget::{DrawContext, DrawReport, KeyState, ModifiersState, WaitContext, Widget},
};

use chrono::{Duration, NaiveDateTime, Timelike};

pub struct Clock<'a> {
    cur_time: NaiveDateTime,
    clock_cache: Font<'a>,
    size: f32,
    digit: u32,
    colon: u32,
}

impl<'a> Clock<'a> {
    pub fn new(time: NaiveDateTime, font: FontRef, size: f32) -> ::std::io::Result<Box<Clock>> {
        let mut clock_cache = Font::new(font, size);
        clock_cache.add_str_to_cache("0123456789:");

        let digit = clock_cache.auto_widest("123456789")?;
        let colon = (clock_cache.auto_widest(":")? as f32 * 1.25) as u32;

        Ok(Box::new(Clock {
            cur_time: time,
            clock_cache,
            size,
            digit,
            colon,
        }))
    }
}

impl<'a> Widget for Clock<'a> {
    fn wait(&mut self, ctx: &mut WaitContext) {
        let target = (self.cur_time + Duration::seconds(60))
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();
        ctx.set_time(target);
    }
    fn enter(&mut self) {}
    fn leave(&mut self) {}
    fn size(&self) -> (u32, u32) {
        (self.digit * 4 + self.colon, self.size.ceil() as u32)
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
        _expansion: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let (width, height) = self.size();
        if !(ctx.time.date() != self.cur_time.date()
            || ctx.time.hour() != self.cur_time.hour()
            || ctx.time.minute() != self.cur_time.minute()
            || ctx.force)
        {
            return Ok(DrawReport::empty(width, height));
        }

        self.cur_time = ctx.time;

        let buf = &mut ctx.buf.subdimensions((pos.0, pos.1, width, height))?;
        buf.memset(ctx.bg);

        let digit = self.digit;
        let colon = self.colon;
        self.clock_cache.draw_text_fixed_width(
            buf,
            ctx.bg,
            &Color::new(1.0, 1.0, 1.0, 1.0),
            &[digit, digit, colon, digit, digit],
            &format!("{:02}:{:02}", ctx.time.hour(), ctx.time.minute()),
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
