
use crate::color::Color;
use crate::widgets::bar_widget::{BarWidget, BarWidgetImpl};

use std::sync::{Arc, Mutex};

use alsa::mixer::{ Mixer, Selem, SelemChannelId, SelemId };

const CARD_NAME: &'static str = "default";
const SELEM_NAME: &'static str = "Master";
const SELEM_ID: u32 = 0u32;

fn alsa_error_to_io_error(fail: &'static str, err: &alsa::Error) -> ::std::io::Error {
    let kind = ::std::io::ErrorKind::Other;
    let func = err.func();
    let errno = err.errno().map(|errno| format!("; errno = {}", errno)).unwrap_or(String::new());
    return ::std::io::Error::new(kind, format!("{}: {}{}", fail, func, errno));
}

pub struct AlsaMixer {
    pub mixer: Mixer,
}

impl AlsaMixer {
    pub fn new() -> ::std::io::Result<Self> {
        Mixer::new(CARD_NAME, true)
            .map(|mixer| Self { mixer })
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
                let max_range = (max - min) as f32;
                master.get_playback_volume(SelemChannelId::mono())
                    .map(|volume| (volume - min) as f32 / max_range)
                    .map_err(|e| alsa_error_to_io_error("Failed to get `Master` volume", &e))
            })
    }
    pub fn set_master_volume(&self, volume: f32) -> ::std::io::Result<()> {
        self.get_master()
            .and_then(|master| {
                let (min, max) = master.get_playback_volume_range();
                let max_range = max - min;
                let volume = min + ((max_range as f32) * volume) as i64;
                master.set_playback_volume_all(volume)
                    .map_err(|e| alsa_error_to_io_error("Failed to set `Master` volume", &e))
            })
    }
}

pub struct AlsaVolume {
    inner: Arc<Mutex<AlsaMixer>>
}

impl AlsaVolume {
    pub fn initialize() -> ::std::io::Result<Self> {
        AlsaMixer::new()
            .map(|mixer| Self{ inner: Arc::new(Mutex::new(mixer)) })
    }
    pub fn get_master_volume(&self) -> ::std::io::Result<f32> {
        self.inner
            .try_lock()
            .map_err(|_| {
                let kind = ::std::io::ErrorKind::Other;
                let msg = "Failed to get `Master` volume: locking failed";
                ::std::io::Error::new(kind, msg)
            })
            .and_then(|ref alsa_mixer| alsa_mixer.get_master_volume())
    }
    
    pub fn set_master_volume(&self, volume: f32) -> ::std::io::Result<()> {
        self.inner
            .try_lock()
            .map_err(|_| {
                let kind = ::std::io::ErrorKind::Other;
                let msg = "Failed to set `Master` volume: locking failed";
                ::std::io::Error::new(kind, msg)
            })
            .and_then(|ref alsa_mixer| alsa_mixer.set_master_volume(volume))
    }
    pub fn new(font_size: f32, length: u32) -> ::std::io::Result<Box<BarWidget>> {
        Self::initialize().map(|alsa| BarWidget::new_simple(font_size, length, Box::new(alsa)))
    }
}


// Note: ALSA returns values from 0 to 65536
impl BarWidgetImpl for AlsaVolume {
    fn name(&self) -> &str {
        "volume"
    }
    fn value(&self) -> f32 {
        // TODO: use min/max instead of hardcoded constants
        self.get_master_volume()
            .unwrap_or_else(|e| { eprintln!("{}", e); 0.0f32 })
    }
    fn color(&self) -> Color {
        // TODO: Custom color from config
        Color::new(1.0, 1.0, 1.0, 1.0)
    }
    fn inc(&mut self, _diff: f32) { }
    fn set(&mut self, abs: f32) {
        let r = self.set_master_volume(abs);
        if let Err(e) = r {
            eprintln!("<AlsaVolume as BarWidgetImpl>::set() failed: {}", e);
        }
    }
    fn toggle(&mut self) {}
}
