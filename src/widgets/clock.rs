use crate::cmd::Cmd;
use crate::color::Color;
use crate::draw::{Font, ROBOTO_REGULAR};
use crate::widget::{DrawContext, DrawReport, KeyState, ModifiersState, Widget};

use std::sync::mpsc::Sender;

use chrono::{DateTime, Datelike, Duration, Local, Timelike};

pub struct Clock {
    cur_time: DateTime<Local>,
    clock_cache: Font,
    size: f32,
}

impl Clock {
    pub fn new(size: f32, ch: Sender<Cmd>) -> Box<Clock> {
        let _ = std::thread::Builder::new()
            .name("clock_ticker".to_string())
            .spawn(move || loop {
                let n = Local::now();
                let target = (n + Duration::seconds(60))
                    .with_second(0)
                    .unwrap()
                    .with_nanosecond(0)
                    .unwrap();

                let d = target - n;

                std::thread::sleep(d.to_std().unwrap());
                ch.send(Cmd::Draw).unwrap();
            });

        let mut clock_cache = Font::new(&ROBOTO_REGULAR, size);
        clock_cache.add_str_to_cache("0123456789:");

        let time = Local::now();

        Box::new(Clock {
            cur_time: time.with_year(time.year().saturating_sub(1)).unwrap(),
            clock_cache: clock_cache,
            size: size,
        })
    }
}

impl Widget for Clock {
    fn size(&self) -> (u32, u32) {
        let digit = (self.size * 0.45).ceil() as u32;
        let colon = (self.size * 0.20).ceil() as u32;
        (digit * 4 + colon, self.size.ceil() as u32)
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let (width, height) = self.size();
        if !(ctx.time.date() != self.cur_time.date()
            || ctx.time.hour() != self.cur_time.hour()
            || ctx.time.minute() != self.cur_time.minute()
            || ctx.force)
        {
            return Ok(DrawReport::empty(width, height));
        }

        self.cur_time = ctx.time.clone();

        let buf = &mut ctx.buf.subdimensions((pos.0, pos.1, width, height))?;
        buf.memset(ctx.bg);

        let digit = (self.size * 0.45).ceil() as u32;
        let colon = (self.size * 0.20).ceil() as u32;
        let dim = self.clock_cache.draw_text_fixed_width(
            buf,
            ctx.bg,
            &Color::new(1.0, 1.0, 1.0, 1.0),
            &[digit, digit, colon, digit, digit],
            &format!("{:02}:{:02}", ctx.time.hour(), ctx.time.minute()),
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
