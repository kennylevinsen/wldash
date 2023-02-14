use std::default::Default;

use crate::{
    buffer::BufferView,
    color::Color,
    event::Event,
    fonts::FontMap,
    widgets::{Geometry, Widget},
};

use chrono::{Datelike, Local, NaiveDate};

pub struct Calendar {
    font: &'static str,
    month_font: &'static str,
    sections_x: i32,
    sections_y: i32,
    shown_sections_x: u32,
    shown_sections_y: u32,
    size: f32,
    dirty: bool,
    geometry: Geometry,
    year_width: u32,
    date_width: u32,
    day_width: u32,
}

const DAY_FACTOR: f32 = 0.6;
const YEAR_FACTOR: f32 = 0.75;
const MONTH_FACTOR: f32 = 1.25;
const DATE_FACTOR: f32 = 1.;

const LINE_HEIGHT: f32 = 0.85;

impl Calendar {
    pub fn new(
        fm: &mut FontMap,
        font: &'static str,
        month_font: &'static str,
        size: f32,
        sections_x: i32,
        sections_y: i32,
    ) -> Calendar {
        fm.queue_font(font, size * DAY_FACTOR, "ADEFHIMNORSTUW");
        fm.queue_font(font, size * YEAR_FACTOR, "0123456789");
        fm.queue_font(font, size * DATE_FACTOR, "0123456789");
        fm.queue_font(month_font, size * MONTH_FACTOR, "ADFJMOSabcehgilmnoprstuvy");

        Calendar {
            font,
            month_font,
            size,
            sections_x,
            sections_y,
            shown_sections_x: 0,
            shown_sections_y: 0,
            dirty: true,
            geometry: Default::default(),
            date_width: 0,
            day_width: 0,
            year_width: 0,
        }
    }

    fn draw_month(
        &mut self,
        fonts: &mut FontMap,
        mut view: &mut BufferView,
        orig: NaiveDate,
        mut time: NaiveDate,
    ) {
        let cal_width = 8 * self.date_width + 7 * (self.date_width / 2);
        //
        // Draw the week day
        //
        let day_offset = (self.date_width - self.day_width) / 2;
        let date_offset = self.date_width + self.date_width / 2;
        for idx in 1..8 {
            let day_font = fonts.get_font(self.font, self.size * DAY_FACTOR);
            let (wk_chr, c) = match idx {
                1 => ("MON", Color::WHITE),
                2 => ("TUE", Color::WHITE),
                3 => ("WED", Color::WHITE),
                4 => ("THU", Color::WHITE),
                5 => ("FRI", Color::WHITE),
                6 => ("SAT", Color::GREY80),
                7 => ("SUN", Color::GREY80),
                _ => panic!("impossible value"),
            };

            day_font
                .draw_text(
                    &mut view.offset((
                        idx * date_offset + day_offset,
                        (self.size * MONTH_FACTOR).ceil() as u32
                            + (self.size * DAY_FACTOR * 0.2).ceil() as u32,
                    )),
                    c,
                    &wk_chr,
                )
                .unwrap();
        }

        //
        // Draw the month
        //
        let month_font = fonts.get_font(self.month_font, self.size * MONTH_FACTOR);
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
        month_font
            .draw_text(&mut view, Color::WHITE, month_str)
            .unwrap();

        //
        // Draw the year
        //
        if time.year() != orig.year() {
            let year_font = fonts.get_font(self.font, self.size * YEAR_FACTOR);
            year_font
                .draw_text(
                    &mut view.offset((cal_width - self.year_width, 0)),
                    Color::GREY80,
                    &format!("{:}", time.year()),
                )
                .unwrap();
        }

        let cal_font = fonts.get_font(self.font, self.size * DATE_FACTOR);
        'row: for y_off in 1.. {
            //
            // Draw the week number
            //
            let wk = time.iso_week();
            cal_font
                .draw_text(
                    &mut view.offset((
                        0,
                        y_off * (self.size * LINE_HEIGHT).ceil() as u32
                            + (self.size * MONTH_FACTOR).ceil() as u32,
                    )),
                    if wk == orig.iso_week() {
                        Color::WHITE
                    } else {
                        Color::GREY75
                    },
                    &format!("{:02}", wk.week()),
                )
                .unwrap();

            //
            // Draw the dates
            //
            let x_off = time.weekday().number_from_monday();
            for x_pos in x_off..8 {
                let c = if time.day() == orig.day() && time.month() == orig.month() {
                    Color::WHITE
                } else if x_pos < 6 {
                    Color::GREY50
                } else {
                    Color::GREY35
                };

                cal_font
                    .draw_text(
                        &mut view.offset((
                            x_pos * date_offset,
                            y_off * (self.size * LINE_HEIGHT).ceil() as u32
                                + (self.size * MONTH_FACTOR).ceil() as u32,
                        )),
                        c,
                        &format!("{:02}", time.day()),
                    )
                    .unwrap();

                let t = time.with_day(time.day() + 1);
                if t.is_none() {
                    break 'row;
                }
                time = t.unwrap();
            }
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

        let cal_height =
            (self.size * MONTH_FACTOR).ceil() as u32 + (self.size * LINE_HEIGHT * 8.).ceil() as u32;
        let cal_width = 8 * self.date_width + 7 * (self.date_width / 2);
        let cal_pad = self.date_width * 2;

        let mut t = time.with_day(1).unwrap();
        let cals = self.shown_sections_y * self.shown_sections_x;
        if cals >= 3 {
            t = t.pred_opt().unwrap().with_day(1).unwrap();
        }

        for ydx in 0..self.shown_sections_y {
            for idx in 0..self.shown_sections_x {
                self.draw_month(
                    fonts,
                    &mut view.offset(((cal_width + cal_pad) * idx, cal_height * ydx)),
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

    fn geometry_update(&mut self, fonts: &mut FontMap, geometry: &Geometry) -> Geometry {
        let year_font = fonts.get_font(self.font, self.size * YEAR_FACTOR);
        self.year_width = year_font.auto_widest("0123456789").unwrap() * 4;
        let cal_font = fonts.get_font(self.font, self.size * DATE_FACTOR);
        self.date_width = cal_font.auto_widest("0123456789").unwrap() * 2;
        let day_font = fonts.get_font(self.font, self.size * DAY_FACTOR);
        self.day_width = day_font.auto_widest("ADEFHIMNORSTUW").unwrap() * 3;

        let cal_width = 8 * self.date_width + 7 * (self.date_width / 2);
        let cal_height =
            (self.size * MONTH_FACTOR).ceil() as u32 + (self.size * LINE_HEIGHT * 8.).ceil() as u32;

        let cal_pad = self.date_width * 2;

        if geometry.width < cal_width || geometry.height < cal_height {
            self.shown_sections_x = 0;
            self.shown_sections_y = 0;
            self.geometry = Default::default();
            return self.geometry;
        }

        let possible_sections_x = 1 + (geometry.width - cal_width) / (cal_width + cal_pad);
        let possible_sections_y = 1 + (geometry.height - cal_height) / cal_height;

        self.shown_sections_x =
            if self.sections_x > 0 && self.sections_x < possible_sections_x as i32 {
                self.sections_x as u32
            } else {
                possible_sections_x
            };
        self.shown_sections_y =
            if self.sections_y > 0 && self.sections_y < possible_sections_y as i32 {
                self.sections_y as u32
            } else {
                possible_sections_y
            };

        self.geometry = Geometry {
            x: geometry.x,
            y: geometry.y,
            width: cal_width * self.shown_sections_x as u32
                + cal_pad * (self.shown_sections_x as u32 - 1),
            height: cal_height * self.shown_sections_y as u32,
        };
        self.geometry
    }

    fn minimum_size(&mut self, fonts: &mut FontMap) -> Geometry {
        let cal_font = fonts.get_font(self.font, self.size * DATE_FACTOR);
        let date_width = cal_font.auto_widest("0123456789").unwrap() * 2;

        let cal_width = 8 * date_width + 7 * (date_width / 2);
        let cal_height =
            (self.size * MONTH_FACTOR).ceil() as u32 + (self.size * LINE_HEIGHT * 8.).ceil() as u32;
        let cal_pad = date_width * 2;

        let sections_x = match self.sections_x {
            -1 => 1,
            0 => {
                return Geometry {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                }
            }
            x => x,
        };
        let sections_y = match self.sections_y {
            -1 => 1,
            0 => {
                return Geometry {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                }
            }
            y => y,
        };
        let width = cal_width * sections_x as u32 + cal_pad * (sections_x as u32 - 1);
        let height = cal_height * sections_y as u32;
        Geometry {
            x: 0,
            y: 0,
            width: width,
            height: height,
        }
    }
}
