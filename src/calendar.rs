use crate::buffer::Buffer;
use crate::color::Color;
use crate::module::{ModuleImpl, Input};
use crate::draw::{draw_text, ROBOTO_REGULAR, DEJAVUSANS_MONO};
use chrono::{DateTime, Date, Datelike, Local};

pub struct Calendar {
    cur_date: Date<Local>,
    first_draw: bool,
    offset: i32,
}

fn draw_month(
    buf: &mut Buffer,
    background_color: &Color,
    orig: &Date<Local>,
    time: &Date<Local>,
) -> Result<(i32, i32, i32, i32), ::std::io::Error> {
    buf.memset(background_color);

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
    draw_text(
        ROBOTO_REGULAR,
        &mut buf.subdimensions((0, 0, 304, 72)),
        background_color,
        &Color::new(1.0, 1.0, 1.0, 1.0),
        64.0,
        month_str,
    )?;

    if time.year() != orig.year() {
        draw_text(
            ROBOTO_REGULAR,
            &mut buf.subdimensions((320, 0, 64, 32)),
            background_color,
            &Color::new(0.8, 0.8, 0.8, 1.0),
            24.0,
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
            6 => "SUN",
            7 => "SAT",
            _ => panic!("impossible value"),
        };

        draw_text(
            DEJAVUSANS_MONO,
            &mut buf.subdimensions((idx * 48 + 4, (y_off * 32) + 64, 32, 16)),
            background_color,
            &Color::new(1.0, 1.0, 1.0, 1.0),
            16.0,
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
        draw_text(
            DEJAVUSANS_MONO,
            &mut buf.subdimensions((0 * 48, (y_off * 32) + 64, 38, 32)),
            background_color,
            &Color::new(0.75, 0.75, 0.75, 1.0),
            32.0,
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
            draw_text(
                DEJAVUSANS_MONO,
                &mut buf.subdimensions((x_pos * 48, (y_off * 32) + 64, 38, 32)),
                background_color,
                &c,
                32.0,
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

impl Calendar {
    pub fn new() -> Calendar {
        Calendar{
            cur_date: Local::now().date(),
            first_draw: true,
            offset: 0,
        }
    }
}

impl ModuleImpl for Calendar {
    fn draw(
        &self,
        buf: &mut Buffer,
        background_color: &Color,
        time: &DateTime<Local>,
    ) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error> {
        let time = time.date();
        let mut t = time.with_day(1).unwrap();
        if self.offset != 0 {
            let mut new_month = t.month() as i32 + self.offset;
            let mut new_year = t.year();
            while new_month > 12 {
                new_year += 1;
                new_month -= 12;
            }
            while new_month < 0 {
                new_year -= 1;
                new_month += 12;
            }
            t = t.with_year(new_year).unwrap().with_month(new_month as u32).unwrap();
        }
        let mut damage: Vec<(i32, i32, i32, i32)> = Vec::new();
        damage.push(draw_month(
            &mut buf.subdimensions((0, 0, 384, 344)),
            background_color,
            &time,
            &t.pred().with_day(1).unwrap(),
        )?);
        damage.push(draw_month(
            &mut buf.subdimensions((448, 0, 384, 344)),
            background_color,
            &time,
            &t,
        )?);
        let n = if t.month() == 12 {
            t.with_year(t.year() + 1).unwrap().with_month(1).unwrap()
        } else {
            t.with_month(t.month() + 1).unwrap()
        };
        damage.push(draw_month(
            &mut buf.subdimensions((896, 0, 384, 344)),
            background_color,
            &time,
            &n,
        )?);
        Ok(damage)
    }

    fn update(&mut self, time: &DateTime<Local>, force: bool) -> Result<bool, ::std::io::Error>{
        if self.first_draw {
            self.first_draw = false;
            Ok(true)
        } else if time.date() != self.cur_date || force{
            self.cur_date = time.date();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn input(&mut self, input: Input) {
        match input {
            Input::Scroll{pos: _, x: _, y} => {
                if y < 0.0 {
                    self.offset += 1;
                } else {
                    self.offset -= 1;
                }
                self.first_draw = true;
            },
            Input::Click{pos: _, button: _} => {
                self.offset = 0;
                self.first_draw = true;
            }
        }
    }
}
