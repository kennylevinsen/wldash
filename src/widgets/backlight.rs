use std::{
    fs::{self, OpenOptions},
    io::{Write, Error, ErrorKind},
    path::{Path, PathBuf},
};

use crate::{
    color::Color,
    event::PointerButton,
    fonts::FontMap,
    widgets::bar_widget::{BarWidget, BarWidgetImpl},
};

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

pub struct Backlight {
    device_path: PathBuf,
    cur: u64,
    max: u64,
    dirty: bool
}

impl Backlight {
    fn update(&mut self) {
        self.cur = read_file_as_u64(self.device_path.join("brightness").as_path()).unwrap();
        self.max = read_file_as_u64(self.device_path.join("max_brightness").as_path()).unwrap();
    }

    pub fn new(path: &str, fm: &mut FontMap, font: &'static str, size: f32) -> BarWidget {
        let mut dev = Backlight {
            device_path: Path::new("/sys/class/backlight").to_path_buf().join(path),
            cur: 0,
            max: 0,
            dirty: true,
        };

        dev.update();
        BarWidget::new(Box::new(dev), fm, font, size)
    }

    pub fn detect(path: &str) -> bool {
        let path = Path::new("/sys/class/backlight").to_path_buf().join(path);
        match read_file_as_u64(path.join("brightness").as_path()) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    pub fn set(&mut self, brightness: f32) {
        let val = (self.max as f32 * brightness) as u64;
        write_file_as_u64(self.device_path.join("brightness").as_path(), val);
        self.cur = read_file_as_u64(self.device_path.join("brightness").as_path()).unwrap();
    }
}

impl BarWidgetImpl for Backlight {
    fn name(&self) -> &'static str {
        "backlight"
    }
    fn get_dirty(&self) -> bool {
        self.dirty
    }
    fn value(&mut self) -> f32 {
        if self.cur > self.max {
            // what.
            return 1.0;
        }

        self.dirty = false;
        self.cur as f32 / self.max as f32
    }
    fn color(&self) -> Color {
        Color::WHITE
    }
    fn click(&mut self, pos: f32, btn: PointerButton) {
        self.dirty = true;
        match btn {
            PointerButton::Left => self.set(pos),
            _ => (),
        };
    }
}
