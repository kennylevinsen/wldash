use crate::buffer::Buffer;
use crate::color::Color;
use crate::draw::{Font, ROBOTO_REGULAR};
use crate::module::{Input, ModuleImpl};

use std::cmp::Ordering;
use std::io::Read;
use std::cell::RefCell;

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
            Err(::std::io::Error::new(
                ::std::io::ErrorKind::Other,
                "input is a tty",
            ))
        }
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

        self.font.borrow_mut().draw_text(
            &mut buf.subdimensions((0, 0, 128, 32))?,
            bg,
            &Color::new(0.75, 0.75, 0.75, 1.0),
            "Run: ",
        )?;

        let mut x_off: i32 = 0;
        let mut width_remaining: i32 = 1280 - 64;
        if self.matches.len() == 0 && self.cur.len() > 0 {
            self.font.borrow_mut().draw_text(
                &mut buf
                    .subdimensions((64, 0, width_remaining as u32, 32))
                    .unwrap(),
                bg,
                &Color::new(1.0, 0.5, 0.5, 1.0),
                &self.cur,
            )?;
        } else {
            for (idx, m) in self.matches.iter().enumerate() {
                let mut b =
                    match buf.subdimensions((64 + x_off as u32, 0, width_remaining as u32, 32)) {
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
                    self.font.borrow_mut().draw_text_individual_colors(&mut b, bg, &colors, &m)?
                } else {
                    self.font.borrow_mut().draw_text(
                        &mut b,
                        bg,
                        &Color::new(0.5, 0.5, 0.5, 1.0),
                        m,
                    )?
                };

                x_off += (size.0 + 8) as i32;
                width_remaining -= (size.0 + 8) as i32;

                if width_remaining < 0 {
                    break;
                }
            }
        }

        Ok(vec![buf.get_signed_bounds()])
    }

    fn update(&mut self, _time: &DateTime<Local>, force: bool) -> Result<bool, ::std::io::Error> {
        if self.dirty || force {
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
                    if self.matches.len() > self.offset {
                        println!("{}", self.matches[self.offset]);
                        std::process::exit(0);
                    } else {
                        println!("{}", self.cur);
                        std::process::exit(0);
                    }
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
