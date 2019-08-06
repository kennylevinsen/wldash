use crate::buffer::Buffer;
use crate::color::Color;
use crate::draw::{Font, ROBOTO_REGULAR};
use crate::module::{Input, ModuleImpl};

use std::cell::RefCell;
use std::cmp::Ordering;
use std::io::Read;
use std::process::Command;
use std::str;

use atty::Stream;
use chrono::{DateTime, Local};
use fuzzy_matcher::skim::{fuzzy_indices, fuzzy_match};
use smithay_client_toolkit::keyboard::keysyms;

pub struct Launcher {
    options: Vec<String>,
    matches: Vec<String>,
    cur: String,
    offset: usize,
    font: RefCell<Font>,
    dirty: bool,
}

impl Launcher {
    pub fn new() -> Result<Launcher, ::std::io::Error> {
        if !atty::is(Stream::Stdin) {
            let mut inbuf = String::new();
            std::io::stdin().read_to_string(&mut inbuf).unwrap();
            let options = inbuf
                .split("\n")
                .into_iter()
                .filter(|s| s.len() > 0)
                .map(|s| s.to_string())
                .collect();

            Ok(Launcher {
                options: options,
                matches: vec![],
                cur: "".to_string(),
                offset: 0,
                font: RefCell::new(Font::new(&ROBOTO_REGULAR, 32.0)),
                dirty: true,
            })
        } else {
            Ok(Launcher {
                options: vec![],
                matches: vec![],
                cur: "".to_string(),
                offset: 0,
                font: RefCell::new(Font::new(&ROBOTO_REGULAR, 32.0)),
                dirty: true,
            })
        }
    }

    fn draw_launcher(
        &self,
        buf: &mut Buffer,
        bg: &Color,
    ) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error> {
        buf.memset(bg);

        let mut x_off = if self.cur.len() > 0 {
            let c = if self.matches.len() == 0 { Color::new(1.0, 0.5, 0.5, 1.0) } else { Color::new(1.0, 1.0, 1.0, 1.0) };
            let dim = self.font.borrow_mut().auto_draw_text(
                &mut buf.subdimensions((0, 0, 1232, 32))?,
                bg,
                &c,
                &self.cur,
            )?;

            dim.0 + 8
        } else {
            0
        };


        let mut width_remaining: i32 = 1232 - x_off as i32;
        for (idx, m) in self.matches.iter().enumerate() {
            let mut b =
                match buf.subdimensions((x_off, 0, width_remaining as u32, 32)) {
                    Ok(b) => b,
                    Err(_) => break,
                };
            let size = if idx == self.offset && self.cur.len() > 0 {
                let (_, indices) =
                    fuzzy_indices(&m.to_lowercase(), &self.cur.to_lowercase()).unwrap();
                let mut colors = Vec::with_capacity(m.len());
                for pos in 0..m.len() {
                    if indices.contains(&pos) {
                        colors.push(Color::new(1.0, 1.0, 1.0, 1.0));
                    } else {
                        colors.push(Color::new(0.75, 0.75, 0.75, 1.0));
                    }
                }
                self.font
                    .borrow_mut()
                    .auto_draw_text_individual_colors(&mut b, bg, &colors, &m)?
            } else {
                self.font.borrow_mut().auto_draw_text(
                    &mut b,
                    bg,
                    &Color::new(0.5, 0.5, 0.5, 1.0),
                    m,
                )?
            };

            x_off += size.0 + 8;
            width_remaining -= (size.0 + 8) as i32;

            if width_remaining < 0 {
                break;
            }
        }

        Ok(vec![buf.get_signed_bounds()])
    }

    fn draw_calc(
        &self,
        buf: &mut Buffer,
        bg: &Color,
    ) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error> {
        buf.memset(bg);

        let x_off = if self.cur.len() > 0 {
            let dim = self.font.borrow_mut().auto_draw_text(
                &mut buf.subdimensions((0, 0, 1232, 32))?,
                bg,
                &Color::new(1.0, 1.0, 1.0, 1.0),
                &self.cur,
            )?;

            dim.0 + 16
        } else {
            0
        };

        if let Ok(output) = Command::new("ivy")
                    .arg("-e")
                    .arg(&self.cur.chars().skip(1).collect::<String>())
                    .output() {
            if let Ok(stdout) = str::from_utf8(&output.stdout) {
                let stdout = stdout.trim();
                if stdout.len() > 0 {
                    self.font.borrow_mut().auto_draw_text(
                        &mut buf.subdimensions((x_off, 0, 1232-x_off, 32))?,
                        bg,
                        &Color::new(0.75, 0.75, 0.75, 1.0),
                        &format!(" = {:}", stdout),
                    )?;
                }
            }
        }

        Ok(vec![buf.get_signed_bounds()])
    }

    fn draw_shell(
        &self,
        buf: &mut Buffer,
        bg: &Color,
    ) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error> {
        buf.memset(bg);

        self.font.borrow_mut().auto_draw_text(
            &mut buf.subdimensions((0, 0, 1232, 32))?,
            bg,
            &Color::new(1.0, 1.0, 1.0, 1.0),
            &self.cur,
        )?;

        Ok(vec![buf.get_signed_bounds()])
    }
}

impl ModuleImpl for Launcher {
    fn draw(
        &self,
        buf: &mut Buffer,
        bg: &Color,
        _time: &DateTime<Local>,
    ) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error> {
        buf.memset(bg);

        match self.cur.chars().next() {
            Some('=') => self.draw_calc(buf, bg),
            Some('!') => self.draw_shell(buf, bg),
            _ => self.draw_launcher(buf, bg),
        }?;

        Ok(vec![buf.get_signed_bounds()])
    }

    fn update(&mut self, _time: &DateTime<Local>, force: bool) -> Result<bool, ::std::io::Error> {
        if self.dirty || force {
            match self.cur.chars().next() {
                Some('=') => (),
                Some('!') => (),
                _ => {
                    let mut m = self
                        .options
                        .iter()
                        .map(|x| (fuzzy_match(&x.to_lowercase(), &self.cur.to_lowercase()), x))
                        .filter(|(x, _)| x.is_some())
                        .map(|(x, y)| (x.unwrap(), y.to_string()))
                        .collect::<Vec<(i64, String)>>();

                    m.sort_by(|(x1, y1), (x2, y2)| {
                        if x1 > x2 {
                            Ordering::Less
                        } else if x1 < x2 {
                            Ordering::Greater
                        } else if y1.len() < y2.len() {
                            Ordering::Less
                        } else if y1.len() > y2.len() {
                            Ordering::Greater
                        } else {
                            y1.cmp(y2)
                        }
                    });

                    self.matches = m.into_iter().map(|(_, x)| x).collect();
                }
            };

            self.dirty = false;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn input(&mut self, input: Input) {
        match input {
            Input::Keypress { key, interpreted } => match key {
                keysyms::XKB_KEY_BackSpace => {
                    if self.cur.len() > 0 {
                        self.cur = self.cur[..self.cur.len() - 1].to_string();
                        self.offset = 0;
                        self.dirty = true
                    }
                }
                keysyms::XKB_KEY_Return => {
                    match self.cur.chars().next() {
                        Some('=') => (),
                        Some('!') => {
                            println!("{}", self.cur.chars().skip(1).collect::<String>());
                            std::process::exit(0);
                        }
                        _ => if self.matches.len() > self.offset {
                            println!("{}", self.matches[self.offset]);
                            std::process::exit(0);
                        }
                    };
                }
                keysyms::XKB_KEY_Right => {
                    if self.matches.len() > 0 && self.offset < self.matches.len() - 1 {
                        self.offset += 1;
                        self.dirty = true;
                    }
                }
                keysyms::XKB_KEY_Left => {
                    if self.matches.len() > 0 && self.offset > 0 {
                        self.offset -= 1;
                        self.dirty = true;
                    }
                }
                _ => match interpreted {
                    Some(v) => {
                        self.cur += &v;
                        self.offset = 0;
                        self.dirty = true
                    }
                    None => {}
                },
            },
            _ => {}
        }
    }
}
