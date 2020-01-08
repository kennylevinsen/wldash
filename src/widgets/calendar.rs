use crate::buffer::Buffer;
use crate::color::Color;
use crate::draw::{Font, DEJAVUSANS_MONO, ROBOTO_REGULAR};
use crate::widget::{DrawContext, DrawReport, KeyState, ModifiersState, WaitContext, Widget};

use chrono::{Date, Datelike, Local};

pub struct Calendar {
    cur_date: Date<Local>,
    dirty: bool,
    offset: f64,
    sections: u32,
    font_size: u32,
    calendar_cache: Font,
    month_cache: Font,
    year_cache: Font,
    day_cache: Font,
}

impl Calendar {
    fn draw_month(
        &self,
        buf: &mut Buffer,
        background_color: &Color,
        orig: &Date<Local>,
        time: &Date<Local>,
    ) -> Result<(i32, i32, i32, i32), ::std::io::Error> {
        let mut time = time.clone();
        let mut y_off = 1;
        let mut done = false;

        let month_str = match time.month() {
            1 => "January",
            2 => "February",
            3 => "March",
            4 => "April",
            5 => "May",
            6 => "June",
            7 => "July",
            8 => "August",
            9 => "September",
            10 => "October",
            11 => "November",
            12 => "December",
            _ => panic!("impossible value"),
        };

        //
        // Draw the of the month
        //

        self.month_cache.draw_text(
            buf,
            background_color,
            &Color::new(1.0, 1.0, 1.0, 1.0),
            month_str,
        )?;
        if time.year() != orig.year() {
            self.year_cache.draw_text(
                &mut buf.offset((self.font_size * 20, 0))?,
                background_color,
                &Color::new(0.8, 0.8, 0.8, 1.0),
                &format!("{:}", time.year()),
            )?;
        }

        //
        // Draw the week day
        //

        for idx in 1..8 {
            let wk_chr = match idx {
                1 => "MON",
                2 => "TUE",
                3 => "WED",
                4 => "THU",
                5 => "FRI",
                6 => "SAT",
                7 => "SUN",
                _ => panic!("impossible value"),
            };

            self.day_cache.draw_text(
                &mut buf.offset((
                    idx * self.font_size * 3 + self.font_size / 5,
                    y_off * self.font_size * 2 + self.font_size * 4,
                ))?,
                background_color,
                &Color::new(1.0, 1.0, 1.0, 1.0),
                &wk_chr,
            )?;
        }

        y_off += 1;

        while !done {
            let mut x_pos = 0;

            //
            // Find the start of this week
            //
            let mut wkday = time.weekday();
            while wkday != chrono::Weekday::Mon {
                x_pos += 1;
                wkday = wkday.pred();
            }

            //
            // Draw the week number
            //
            let wk = time.iso_week();
            self.calendar_cache.draw_text(
                &mut buf.offset((0, y_off * self.font_size * 2 + self.font_size * 4))?,
                background_color,
                &Color::new(0.75, 0.75, 0.75, 1.0),
                &format!("{:02}", wk.week()),
            )?;
            x_pos += 1;

            //
            // Draw the dates
            //
            while x_pos < 8 {
                let c = if time.day() == orig.day() && time.month() == orig.month() {
                    Color::new(1.0, 1.0, 1.0, 1.0)
                } else {
                    Color::new(0.5, 0.5, 0.5, 1.0)
                };

                self.calendar_cache.draw_text(
                    &mut buf.offset((
                        x_pos * self.font_size * 3,
                        y_off * self.font_size * 2 + self.font_size * 4,
                    ))?,
                    background_color,
                    &c,
                    &format!("{:02}", time.day()),
                )?;

                let t = time.with_day(time.day() + 1);
                if t.is_none() {
                    done = true;
                    break;
                }
                time = t.unwrap();
                x_pos += 1;
            }

            y_off += 1;
        }
        Ok(buf.get_signed_bounds())
    }
}

impl Calendar {
    pub fn new(font_size: f32, sections: u32) -> Box<Calendar> {
        let mut calendar_cache = Font::new(&DEJAVUSANS_MONO, font_size * 2.0);
        calendar_cache.add_str_to_cache("0123456789");
        let mut month_cache = Font::new(&ROBOTO_REGULAR, font_size * 4.0);
        month_cache.add_str_to_cache("JanuryFebMchApilJgstSmOoNvD");
        let mut year_cache = Font::new(&DEJAVUSANS_MONO, font_size * 1.5);
        year_cache.add_str_to_cache("-0123456789");
        let mut day_cache = Font::new(&DEJAVUSANS_MONO, font_size);
        day_cache.add_str_to_cache("MONTUEWDHFRISA");
        Box::new(Calendar {
            cur_date: Local::now().date(),
            dirty: true,
            offset: 0.0,
            sections: sections,
            font_size: font_size as u32,
            calendar_cache: calendar_cache,
            month_cache: month_cache,
            year_cache: year_cache,
            day_cache: day_cache,
        })
    }
}

impl Widget for Calendar {
    fn wait(&mut self, _: &mut WaitContext) {}
    fn enter(&mut self) {}
    fn leave(&mut self) {}

    fn size(&self) -> (u32, u32) {
        let cal_width = 7 * self.font_size * 3 + self.font_size * 2;
        let cal_pad = self.font_size * 3;
        (
            cal_width * self.sections + cal_pad * (self.sections - 1),
            (self.font_size as f32 * 21.5) as u32,
        )
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let (width, height) = self.size();
        if ctx.time.date() == self.cur_date && !ctx.force && !self.dirty {
            return Ok(DrawReport::empty(width, height));
        }
        self.dirty = false;
        self.cur_date = ctx.time.date();

        let buf = &mut ctx.buf.subdimensions((pos.0, pos.1, width, height))?;
        buf.memset(ctx.bg);
        let time = ctx.time.date();
        let mut t = time.with_day(1).unwrap();
        let o = (self.offset / 100.0) as i32;

        let cals = self.sections - 1;
        let pre_cals = cals / 2;
        for _ in 0..pre_cals {
            t = t.pred().with_day(1).unwrap();
        }

        if o != 0 {
            let mut month = (t.month() - 1) as i32 + o;
            let mut year = t.year();
            while month > 11 {
                year += 1;
                month -= 12;
            }
            while month < 0 {
                year -= 1;
                month += 12;
            }
            t = t
                .with_year(year)
                .unwrap()
                .with_month((month + 1) as u32)
                .unwrap();
        }
        let cal_pad = 7 * self.font_size * 3 + self.font_size * 5;
        for idx in 0..self.sections {
            self.draw_month(&mut buf.offset((cal_pad * idx, 0))?, ctx.bg, &time, &t)?;

            t = if t.month() == 12 {
                t.with_year(t.year() + 1).unwrap().with_month(1).unwrap()
            } else {
                t.with_month(t.month() + 1).unwrap()
            };
        }
        Ok(DrawReport {
            width: width,
            height: height,
            damage: vec![buf.get_signed_bounds()],
            full_damage: false,
        })
    }

    fn keyboard_input(&mut self, _: u32, _: ModifiersState, _: KeyState, _: Option<String>) {}
    fn mouse_click(&mut self, _: u32, (x, _): (u32, u32)) {
        let cal_pad = 7 * self.font_size * 3 + self.font_size * 5;
        let cals = self.sections - 1;
        let pre_cals = cals / 2;
        if x < pre_cals * cal_pad {
            self.offset -= 100.0;
        } else if x >= (pre_cals + 1) * cal_pad {
            self.offset += 100.0;
        } else {
            self.offset = 0.0;
        }
        self.dirty = true;
    }
    fn mouse_scroll(&mut self, (_, y): (f64, f64), _: (u32, u32)) {
        self.offset += y;
        self.dirty = true;
    }
}
