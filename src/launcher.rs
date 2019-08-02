use crate::buffer::Buffer;
use crate::color::Color;
use crate::draw::{draw_text, ROBOTO_REGULAR};
use crate::module::{Input, ModuleImpl};

use std::io::Read;

use atty::Stream;

use chrono::{DateTime, Local};

use smithay_client_toolkit::keyboard::keysyms;

pub struct Launcher {
    options: Vec<String>,
    matches: Vec<String>,
    cur: String,
    offset: usize,
    dirty: bool
}

impl Launcher {
    pub fn new() -> Result<Launcher, ::std::io::Error> {
        if !atty::is(Stream::Stdin) {
            let mut inbuf = String::new();
            std::io::stdin().read_to_string(&mut inbuf).unwrap();
            let options = inbuf.split("\n").into_iter().map(|s| s.to_string()).collect();

            Ok(Launcher {
                options: options,
                matches: vec![],
                cur: "".to_string(),
                offset: 0,
                dirty: true,
            })
        } else {
            Err(::std::io::Error::new(::std::io::ErrorKind::Other, "input is a tty"))
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

        draw_text(
            ROBOTO_REGULAR,
            &mut buf.subdimensions((0, 0, 128, 32))?,
            bg,
                &Color::new(0.5, 0.5, 0.5, 1.0),
            32.0,
            "Run: ",
        )?;


        let mut x_off: i32 = 0;
        let mut width_remaining: i32 = 1280 - 64;
        for (idx, m) in self.matches.iter().enumerate() {
            let mut b = match buf.subdimensions((64 + x_off as u32, 0, width_remaining as u32, 32)) {
                Ok(b) => b,
                Err(_) => break,
            };
            let size = if idx == self.offset && self.cur.len() > 0 {
                let l = self.cur.len();
                let off = m.to_lowercase().find(&self.cur.to_lowercase()).unwrap();
                let s1 = draw_text(
                    ROBOTO_REGULAR,
                    &mut b,
                    bg,
                    &Color::new(0.75, 0.75, 0.75, 1.0),
                    32.0,
                    &m[0..off],
                )?;
                let mut b2 = b.subdimensions((s1.0+1, 0, width_remaining as u32 - s1.0-1, 32))?;
                let s2 = draw_text(
                    ROBOTO_REGULAR,
                    &mut b2,
                    bg,
                    &Color::new(1.0, 1.0, 1.0, 1.0),
                    32.0,
                    &m[off..off+l],
                )?;
                let mut b3 = b.subdimensions((s1.0+s2.0+2, 0, width_remaining as u32 - s1.0- s2.0 - 2, 32))?;
                let s3 = draw_text(
                    ROBOTO_REGULAR,
                    &mut b3,
                    bg,
                    &Color::new(0.75, 0.75, 0.75, 1.0),
                    32.0,
                    &m[off+l..],
                )?;

                (s1.0 + s2.0 + s3.0 + 3, s1.1 + s2.1 + s3.1)
            } else {
                draw_text(
                    ROBOTO_REGULAR,
                    &mut b,
                    bg,
                    &Color::new(0.5, 0.5, 0.5, 1.0),
                    32.0,
                    m,
                )?

            };

            x_off += (size.0 + 8) as i32;
            width_remaining -= (size.0 + 8) as i32;

            if width_remaining < 0 {
                break;
            }
        }


        // draw_text_fixed_width(
        //     ROBOTO_REGULAR,
        //     &mut buf.subdimensions((0, 64, 288 * 2 + 64, 256))?,
        //     bg,
        //     &Color::new(1.0, 1.0, 1.0, 1.0),
        //     256.0,
        //     vec![120, 120, 64, 120, 120],
        //     &format!("{:02}:{:02}", time.hour(), time.minute()),
        // )?;

        Ok(vec![buf.get_signed_bounds()])
    }

    fn update(&mut self, _time: &DateTime<Local>, force: bool) -> Result<bool, ::std::io::Error> {
        if self.dirty || force {
            self.matches = self
                .options
                .iter()
                .filter(|x| x.to_lowercase().find(&self.cur.to_lowercase()).is_some())
                .map(|x| x.to_string())
                .collect();

            self.matches.sort();
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
                        self.cur = self.cur[..self.cur.len()-1].to_string();
                        self.offset = 0;
                        self.dirty = true
                    }
                },
                keysyms::XKB_KEY_Return => {
                    if self.matches.len() > self.offset {
                        println!("{}", self.matches[self.offset]);
                        std::process::exit(0);
                    }
                },
                keysyms::XKB_KEY_Right => {
                    if self.matches.len() > 0 && self.offset < self.matches.len() - 1{
                        self.offset += 1;
                        self.dirty = true;
                    }
                },
                keysyms::XKB_KEY_Left => {
                    if self.matches.len() > 0 && self.offset > 0 {
                        self.offset -= 1;
                        self.dirty = true;
                    }
                },
                _ => match interpreted {
                    Some(v) => {
                        self.cur += &v;
                        self.offset = 0;
                        self.dirty = true
                    },
                    None => {}
                }
            }
            _ => {}
        }
    }
}
