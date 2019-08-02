use crate::buffer::Buffer;
use crate::color::Color;
use crate::draw::{draw_bar, draw_text, draw_box, ROBOTO_REGULAR};
use crate::module::{Input, ModuleImpl};

use std::fs::OpenOptions;
use std::io::{Error, ErrorKind};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

pub struct Backlight {
    device_path: PathBuf,
    cur_brightness: u64,
    max_brightness: u64,
    dirty: bool,
}

fn read_file_as_u64(path: &Path) -> Result<u64, Error> {
    let mut file = OpenOptions::new().read(true).open(path)?;
    let mut str = String::new();
    file.read_to_string(&mut str)?;
    str.pop();
    str.parse::<u64>()
        .map_err(|_e| Error::new(ErrorKind::Other, "unable to parse value"))
}

fn write_file_as_u64(path: &Path, value: u64) -> Result<(), Error> {
    let mut file = OpenOptions::new().write(true).open(path)?;
    file.write_fmt(format_args!("{}", value))
}

impl Backlight {
    pub fn update(&mut self) -> Result<(), Error> {
        self.cur_brightness = read_file_as_u64(self.device_path.join("brightness").as_path())?;
        self.max_brightness = read_file_as_u64(self.device_path.join("max_brightness").as_path())?;
        self.dirty = true;
        Ok(())
    }

    pub fn sync(&mut self) -> Result<(), Error> {
        write_file_as_u64(
            self.device_path.join("brightness").as_path(),
            self.cur_brightness,
        )?;
        self.update()?;
        Ok(())
    }

    pub fn brightness(&self) -> f32 {
        if self.cur_brightness > self.max_brightness {
            // what.
            return 100.0;
        }

        (self.cur_brightness as f32 / self.max_brightness as f32) * 100.0
    }

    pub fn add(&mut self, diff: f32) -> Result<(), Error> {
        let inc = ((self.max_brightness as f32) / 100.0 * diff) as i64;

        self.cur_brightness = if self.cur_brightness as i64 + inc < 1 {
            1
        } else if self.cur_brightness as i64 + inc > self.max_brightness as i64 {
            self.max_brightness
        } else {
            (self.cur_brightness as i64 + inc) as u64
        };

        Ok(())
    }

    pub fn new() -> Result<Self, Error> {
        let devices = Path::new("/sys/class/backlight").read_dir()?;

        let first_device = match devices.take(1).next() {
            Some(v) => match v {
                Ok(v) => v,
                Err(_) => return Err(Error::new(ErrorKind::Other, "no backlight device")),
            },
            None => return Err(Error::new(ErrorKind::Other, "no backlight device")),
        };
        let mut dev = Backlight {
            device_path: first_device.path(),
            cur_brightness: 0,
            max_brightness: 0,
            dirty: true,
        };

        dev.update()?;

        Ok(dev)
    }
}

impl ModuleImpl for Backlight {
    fn draw(
        &self,
        buf: &mut Buffer,
        bg: &Color,
        _time: &DateTime<Local>,
    ) -> Result<Vec<(i32, i32, i32, i32)>, Error> {
        buf.memset(bg);
        let c = Color::new(1.0, 1.0, 1.0, 1.0);
        draw_text(
            ROBOTO_REGULAR,
            &mut buf.subdimensions((0, 0, 128, 24)),
            bg,
            &c,
            24.0,
            "backlight",
        )?;
        draw_bar(
            &mut buf.subdimensions((128, 0, 432, 24)),
            &c,
            432,
            24,
            self.brightness() / 100.0,
        )?;
        draw_box(
            &mut buf.subdimensions((128, 0, 432, 24)),
            &c,
            (432, 24),
        )?;
        Ok(vec![buf.get_signed_bounds()])
    }

    fn update(&mut self, _time: &DateTime<Local>, force: bool) -> Result<bool, ::std::io::Error> {
        if self.dirty || force {
            self.dirty = false;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn input(&mut self, input: Input) {
        match input {
            Input::Scroll {
                pos: _pos,
                x: _x,
                y,
            } => {
                self.add(y as f32 / 8.0).unwrap();
                self.sync().unwrap();
            }
            Input::Click { pos: _pos, button } => {
                match button {
                    273 => {
                        // Right click
                        self.cur_brightness = if self.cur_brightness == 1 {
                            self.max_brightness
                        } else {
                            1
                        };
                        self.sync().unwrap();
                    }
                    _ => {}
                }
            }
        }
    }
}
