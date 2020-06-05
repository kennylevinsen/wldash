use crate::color::Color;
use crate::widget::WaitContext;
use crate::{
    fonts::FontRef,
    widgets::bar_widget::{BarWidget, BarWidgetImpl},
};

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

pub struct Backlight {
    device_path: PathBuf,
    cur_brightness: u64,
    max_brightness: u64,
}

fn read_file_as_u64(path: &Path) -> Result<u64, Error> {
    let mut s = fs::read_to_string(path)?;
    s.pop();
    s.parse::<u64>()
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
            return 1.0;
        }

        self.cur_brightness as f32 / self.max_brightness as f32
    }

    pub fn set(&mut self, val: f32) -> Result<(), Error> {
        self.cur_brightness = (self.max_brightness as f32 * val) as u64;
        Ok(())
    }

    pub fn add(&mut self, diff: f32) -> Result<(), Error> {
        let inc = (self.max_brightness as f32 * diff) as i64;

        self.cur_brightness = if self.cur_brightness as i64 + inc < 1 {
            1
        } else if self.cur_brightness as i64 + inc > self.max_brightness as i64 {
            self.max_brightness
        } else {
            (self.cur_brightness as i64 + inc) as u64
        };

        Ok(())
    }

    pub fn new(
        path: &str,
        font: FontRef,
        font_size: f32,
        length: u32,
    ) -> Result<Box<BarWidget>, Error> {
        let mut dev = Backlight {
            device_path: Path::new("/sys/class/backlight").to_path_buf().join(path),
            cur_brightness: 0,
            max_brightness: 0,
        };

        dev.update()?;

        Ok(BarWidget::new_simple(
            font,
            font_size,
            length,
            Box::new(dev),
        ))
    }
}

impl BarWidgetImpl for Backlight {
    fn wait(&mut self, _: &mut WaitContext) {}
    fn name(&self) -> &str {
        "backlight"
    }
    fn value(&self) -> f32 {
        self.brightness()
    }
    fn color(&self) -> Color {
        Color::new(1.0, 1.0, 1.0, 1.0)
    }
    fn inc(&mut self, inc: f32) {
        self.add(inc).unwrap();
        match self.sync() {
            Ok(val) => val,
            Err(err) => {
                eprintln!("Error while trying to change brightness: {}", err);
            }
        }
    }
    fn set(&mut self, val: f32) {
        self.set(val).unwrap();
        match self.sync() {
            Ok(val) => val,
            Err(err) => {
                eprintln!("Error while trying to change brightness: {}", err);
            }
        }
    }
    fn toggle(&mut self) {
        self.cur_brightness = if self.cur_brightness == 1 {
            self.max_brightness
        } else {
            1
        };
        match self.sync() {
            Ok(val) => val,
            Err(err) => {
                eprintln!("Error while trying to change brightness: {}", err);
            }
        }
    }
}
