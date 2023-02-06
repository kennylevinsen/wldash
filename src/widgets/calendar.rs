use std::default::Default;

use crate::{
    buffer::BufferView,
    color::Color,
    fonts::FontMap,
    event::Event,
    widgets::{Geometry, Widget},
};

use chrono::{Datelike, Local, NaiveDate};

pub struct Calendar {
    font: &'static str,
    sections_x: i32,
    sections_y: i32,
    shown_sections_x: u32,
    shown_sections_y: u32,
    size: f32,
    dirty: bool,
    geometry: Geometry,
}

const DAY_FACTOR: f32 = 1.;
const YEAR_FACTOR: f32 = 1.5;
const MONTH_FACTOR: f32 = 2.;
const DATE_FACTOR: f32 = 2.;
const LINE_HEIGHT: f32 = 1.8;

impl Calendar {
    pub fn new(fm: &mut FontMap, font: &'static str, size: f32, sections_x: i32, sections_y: i32) -> Calendar {
        fm.queue_font(font, size, "ADEFHIMNORSTUW");
        fm.queue_font(font, size * 1.5, "0123456789");
        fm.queue_font(font, size * 2.0, "0123456789 ADFJMOSabcehgilmnoprstuvy");

        Calendar {
            font,
            size,
            sections_x,
            sections_y,
            shown_sections_x: 0,
            shown_sections_y: 0,
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
        let white = Color::WHITE;
        let dim = Color::GREY80;
        let dimmer = Color::GREY75;

        let mut y_off = 1;
        // TODO: Fix offsets:
        // - Headline font is not correctly accounted for

        //
        // Draw the week day
        //
        for idx in 1..8 {
            let day_font = fonts.get_font(self.font, self.size * DAY_FACTOR);
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
                .draw_text(
                    &mut view
                        .offset((
                            idx * (self.size * 2.5).ceil() as u32 + (self.size / 5.).ceil() as u32,
                            y_off * (self.size * LINE_HEIGHT).ceil() as u32
                                + (self.size * 1.).ceil() as u32,
                        )),
                    white,
                    &wk_chr,
                )
                .unwrap();
        }

        //
        // Draw the month
        //
        if time.year() != orig.year() {
            let year_font = fonts.get_font(self.font, self.size * YEAR_FACTOR);
            year_font
                .draw_text(
                    &mut view.offset(((self.size * 17.).ceil() as u32, 0)),
                    dim,
                    &format!("{:}", time.year()),
                )
                .unwrap();
        }

        let cal_font = fonts.get_font(self.font, self.size * DATE_FACTOR);
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
        cal_font
            .draw_text(&mut view, white, month_str)
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
                .draw_text(
                    &mut view
                        .offset((
                            0,
                            y_off * (self.size * LINE_HEIGHT).ceil() as u32
                                + (self.size * MONTH_FACTOR).ceil() as u32,
                        )),
                    dimmer,
                    &format!("{:02}", wk.week()),
                )
                .unwrap();
            x_pos += 1;

            //
            // Draw the dates
            //
            while x_pos < 8 {
                let c = if time.day() == orig.day() && time.month() == orig.month() {
                    Color::WHITE
                } else {
                    Color::GREY50
                };

                cal_font
                    .draw_text(
                        &mut view
                            .offset((
                                x_pos * (self.size * 2.5).ceil() as u32,
                                y_off * (self.size * LINE_HEIGHT).ceil() as u32
                                    + (self.size * MONTH_FACTOR).ceil() as u32,
                            )),
                        c,
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
        if self.shown_sections_y == 0 || self.shown_sections_x == 0 {
            return Geometry {
                x: self.geometry.x,
                y: self.geometry.y,
                width: 0,
                height: 0,
            };
        }

        let time = Local::now().naive_local().date();

        let cal_height = (self.size * 16.25).ceil() as u32;
        let cal_width = (7. * self.size * 2.5 + self.size * 2.).ceil() as u32;
        let cal_pad = (3. * self.size).ceil() as u32;

        let mut t = time.with_day(1).unwrap();
        let cals = self.shown_sections_y * self.shown_sections_x;
        if cals >= 3 {
            t = t.pred_opt().unwrap().with_day(1).unwrap();
        }

        for ydx in 0..self.shown_sections_y {
            for idx in 0..self.shown_sections_x {
                self.draw_month(
                    fonts,
                    &mut view
                        .offset(((cal_width + cal_pad) * idx, (cal_height) * ydx)),
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
        let cal_width = 7 * (self.size * 2.5).ceil() as u32 + (self.size * 2.).ceil() as u32;
        let cal_height = (self.size * MONTH_FACTOR).ceil() as u32
            + (self.size * LINE_HEIGHT * 8.).ceil() as u32;
        let cal_pad = (self.size * 3.).ceil() as u32;

        let possible_sections_x = geometry.width / (cal_width + cal_pad);
        let possible_sections_y = geometry.height / cal_height;

        let width = if self.sections_x > 0 && self.sections_x < possible_sections_x as i32 {
            self.shown_sections_x = self.sections_x as u32;
            cal_width * self.sections_x as u32 + cal_pad * (self.sections_x as u32 - 1)
        } else {
            self.shown_sections_x = possible_sections_x;
            (geometry.width / (cal_width + cal_pad)) * (cal_width + cal_pad)
        };
        let height = if self.sections_y > 0 && self.sections_y < possible_sections_y as i32 {
            self.shown_sections_y = self.sections_y as u32;
            cal_height * self.sections_y as u32
        } else {
            self.shown_sections_y = possible_sections_y;
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
