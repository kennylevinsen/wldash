
use crate::color::Color;
use crate::widgets::bar_widget::{BarWidget, BarWidgetImpl};

use alsa::mixer::{ Mixer, Selem, SelemChannelId, SelemId };

const CARD_NAME: &'static str = "default";
const SELEM_NAME: &'static str = "Master";
const SELEM_ID: u32 = 0u32;

#[inline]
fn alsa_error_to_io_error(fail: &'static str, err: &alsa::Error) -> ::std::io::Error {
    let kind = ::std::io::ErrorKind::Other;
    let func = err.func();
    let errno = err.errno().map(|errno| format!("; errno = {}", errno)).unwrap_or(String::new());
    return ::std::io::Error::new(kind, format!("{}: {}{}", fail, func, errno));
}

#[inline]
fn alsa_volume_to_f32(volume: i64, min: i64, max: i64) -> f32 {
    (volume - min) as f32 / (max - min) as f32
}

#[inline]
fn f32_to_alsa_volume(volume: f32, min: i64, max: i64) -> i64 {
    min + ((max - min) as f32 * volume) as i64
}

pub struct Alsa {
    pub mixer: Mixer,
}

impl Alsa {
    pub fn new(font_size: f32, length: u32) -> ::std::io::Result<Box<BarWidget>> {
        Mixer::new(CARD_NAME, true)
            .map(|mixer| BarWidget::new_simple(font_size, length, Box::new(Self { mixer })))
            .map_err(|err| alsa_error_to_io_error("Failed to create ALSA mixer", &err))
    }
    pub fn get_master<'a>(&'a self) -> ::std::io::Result<Selem<'a>> {
        let master_id = SelemId::new(SELEM_NAME, SELEM_ID);
        match self.mixer.find_selem(&master_id) {
            Some(master) => Ok(master),
            None => {
                let kind = ::std::io::ErrorKind::NotFound;
                let desc = "`Master` not found";
                Err(::std::io::Error::new(kind, desc))
            }
        }
    }
    pub fn get_master_volume(&self) -> ::std::io::Result<f32> {
        self.get_master()
            .and_then(|master| {
                let (min, max) = master.get_playback_volume_range();
                master.get_playback_volume(SelemChannelId::mono())
                    .map(|volume| alsa_volume_to_f32(volume, min, max))
                    .map_err(|e| alsa_error_to_io_error("Failed to get `Master` volume", &e))
            })
    }
    pub fn set_master_volume(&self, volume: f32) -> ::std::io::Result<()> {
        self.get_master()
            .and_then(|master| {
                let (min, max) = master.get_playback_volume_range();
                let volume = f32_to_alsa_volume(volume, min, max);
                master.set_playback_volume_all(volume)
                    .map_err(|e| alsa_error_to_io_error("Failed to set `Master` volume", &e))
            })
    }
    pub fn inc_master_volume(&self, diff: f32) -> ::std::io::Result<()> {
        self.get_master()
            .and_then(|master| {
                let (min, max) = master.get_playback_volume_range();
                master.get_playback_volume(SelemChannelId::mono())
                    .map(|volume| alsa_volume_to_f32(volume, min, max))
                    .and_then(|volume| {
                        let volume = f32_to_alsa_volume(volume + diff, min, max);
                        master.set_playback_volume_all(volume)
                    })
                    .map_err(|e| alsa_error_to_io_error("Failed to inc/dec `Master` volume", &e))
            })
    }
}

impl BarWidgetImpl for Alsa {
    fn name(&self) -> &str {
        "volume"
    }
    fn value(&self) -> f32 {
        self.get_master_volume()
            .unwrap_or_else(|e| { eprintln!("{}", e); 0.0f32 })
    }
    fn color(&self) -> Color {
        Color::new(1.0, 1.0, 1.0, 1.0)
    }
    fn inc(&mut self, diff: f32) {
        if let Err(e) = self.inc_master_volume(diff) {
            eprintln!("<AlsaVolume as BarWidgetImpl>::inc() failed: {}", e);
        }
    }
    fn set(&mut self, abs: f32) {
        if let Err(e) = self.set_master_volume(abs) {
            eprintln!("<AlsaVolume as BarWidgetImpl>::set() failed: {}", e);
        }
    }
    fn toggle(&mut self) {}
}
