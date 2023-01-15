use std::default::Default;

use crate::{
    buffer::BufferView,
    color::Color,
    fonts::FontMap,
    state::Event,
    widgets::{Geometry, Widget},
};

use chrono::{Datelike, Local, NaiveDate};

pub struct Calendar {
    font: &'static str,
    sections_x: i32,
    sections_y: i32,
    size: f32,
    dirty: bool,
    geometry: Geometry,
}

impl Calendar {
    pub fn new(font: &'static str, size: f32, sections_x: i32, sections_y: i32) -> Calendar {
        Calendar {
            font,
            size,
            sections_x,
            sections_y,
            dirty: true,
            geometry: Default::default(),
        }
    }

    fn draw_month(
        &mut self,
        fonts: &mut FontMap,
        mut view: &mut BufferView,
        orig: NaiveDate,
        mut time: NaiveDate,
    ) {
        let black = Color::new(0., 0., 0., 1.);
        let white = Color::new(1., 1., 1., 1.);
        let dim = Color::new(0.8, 0.8, 0.8, 1.);
        let dimmer = Color::new(0.75, 0.75, 0.75, 1.);
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


        let mut y_off = 1;
        // TODO: Fix offsets:
        // - Headline font is not correctly accounted for

        //
        // Draw the week day
        //
        for idx in 1..8 {
            let day_font = fonts.get_font(self.font, self.size);
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

            day_font
                .auto_draw_text(
                    &mut view
                        .offset((
                            idx * (self.size * 2.5).ceil() as u32 + (self.size / 5.).ceil() as u32,
                            y_off * (self.size * 1.8).ceil() as u32
                                + (self.size * 1.).ceil() as u32,
                        ))
                        .unwrap(),
                    &black,
                    &white,
                    &wk_chr,
                )
                .unwrap();
        }

        //
        // Draw the month
        //
        if time.year() != orig.year() {
            let year_font = fonts.get_font(self.font, self.size * 1.5);
            year_font
                .auto_draw_text(
                    &mut view.offset(((self.size * 17.).ceil() as u32, 0)).unwrap(),
                    &black,
                    &dim,
                    &format!("{:}", time.year()),
                )
                .unwrap();
        }

        let cal_font = fonts.get_font(self.font, self.size * 2.);
        cal_font
            .auto_draw_text(&mut view, &black, &white, month_str)
            .unwrap();

        let mut done = false;
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
            cal_font
                .auto_draw_text(
                    &mut view
                        .offset((
                            0,
                            y_off * (self.size * 1.8).ceil() as u32
                                + (self.size * 2.).ceil() as u32,
                        ))
                        .unwrap(),
                    &black,
                    &dimmer,
                    &format!("{:02}", wk.week()),
                )
                .unwrap();
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

                cal_font
                    .auto_draw_text(
                        &mut view
                            .offset((
                                x_pos * (self.size * 2.5).ceil() as u32,
                                y_off * (self.size * 1.8).ceil() as u32 + (self.size * 2.) as u32,
                            ))
                            .unwrap(),
                        &black,
                        &c,
                        &format!("{:02}", time.day()),
                    )
                    .unwrap();

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
    }
}

impl<'a> Widget for Calendar {
    fn get_dirty(&self) -> bool {
        self.dirty
    }

    fn geometry(&self) -> Geometry {
        self.geometry
    }

    fn event(&mut self, event: &Event) {
        match event {
            Event::NewMinute => {
                self.dirty = true;
            }
            _ => (),
        }
    }

    fn draw(&mut self, fonts: &mut FontMap, view: &mut BufferView) -> Geometry {
        let time = Local::now().naive_local().date();

        let cal_height = (self.size * 16.25).ceil() as u32;
        let cal_width = (7. * self.size * 3. + self.size * 2.).ceil() as u32;
        let cal_pad = (3. * self.size).ceil() as u32;
        let sections_y = if self.sections_y > 0 {
            self.sections_y as u32
        } else {
            self.geometry.height / cal_height
        };
        let sections_x = if self.sections_x > 0 {
            self.sections_x as u32
        } else {
            self.geometry.width / (cal_height + cal_pad)
        };

        let mut t = time.with_day(1).unwrap();
        let cals = sections_y * sections_x - 1;
        if cals >=3 {
            t = t.pred_opt().unwrap().with_day(1).unwrap();
        }

        for ydx in 0..sections_y {
            for idx in 0..sections_x {
                self.draw_month(
                    fonts,
                    &mut view
                        .offset(((cal_width + cal_pad) * idx, (cal_height) * ydx))
                        .unwrap(),
                    time,
                    t,
                );
                t = if t.month() == 12 {
                    t.with_year(t.year() + 1).unwrap().with_month(1).unwrap()
                } else {
                    t.with_month(t.month() + 1).unwrap()
                };
            }
        }
        self.dirty = false;
        self.geometry
    }

    fn geometry_update(&mut self, _fonts: &mut FontMap, geometry: &Geometry) -> Geometry {
        let cal_width = (7. * self.size * 3. + self.size * 2.).ceil() as u32;
        let cal_height = (self.size * 16.25).ceil() as u32; // TODO: Calculate from font sizes
        let cal_pad = (self.size * 3.).ceil() as u32;
        let width = if self.sections_x > 0 {
            cal_width * self.sections_x as u32 + cal_pad * (self.sections_x as u32 - 1)
        } else {
            (geometry.width / (cal_width + cal_pad)) *  (cal_width + cal_pad)
        };
        let height = if self.sections_y > 0 {
            cal_height * self.sections_y as u32
        } else {
            (geometry.height / cal_height) * cal_height
        };
        self.geometry = Geometry {
            x: geometry.x,
            y: geometry.y,
            width: width,
            height: height,
        };
        self.geometry
    }
}
