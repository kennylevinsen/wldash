use std::{
    fs,
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
};

use crate::{
    color::Color,
    widgets::bar_widget::{BarWidget, BarWidgetImpl},
};

fn read_file_as_u64(path: &Path) -> Result<u64, Error> {
    let mut s = fs::read_to_string(path)?;
    s.pop();
    s.parse::<u64>()
        .map_err(|_e| Error::new(ErrorKind::Other, "unable to parse value"))
}

pub struct Backlight {
    device_path: PathBuf,
    cur: u64,
    max: u64,
}

impl Backlight {
    fn update(&mut self) {
        self.cur = read_file_as_u64(self.device_path.join("brightness").as_path()).unwrap();
        self.max = read_file_as_u64(self.device_path.join("max_brightness").as_path()).unwrap();
    }

    pub fn new(path: &str, font: &'static str, size: f32) -> BarWidget {
        let mut dev = Backlight {
            device_path: Path::new("/sys/class/backlight").to_path_buf().join(path),
            cur: 0,
            max: 0,
        };

        dev.update();
        BarWidget::new(Box::new(dev), font, size)
    }
}

impl BarWidgetImpl for Backlight {
    fn name(&self) -> &'static str {
        "backlight"
    }
    fn value(&self) -> f32 {
        if self.cur > self.max {
            // what.
            return 1.0;
        }

        self.cur as f32 / self.max as f32
    }
    fn color(&self) -> Color {
        Color::new(1.0, 1.0, 1.0, 1.0)
    }
}
