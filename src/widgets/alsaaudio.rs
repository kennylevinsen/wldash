use std::{
    thread,
    sync::{Arc, Mutex},
};

use crate::{
    color::Color,
    fonts::FontMap,
    event::{Event, Events, PointerButton},
    widgets::bar_widget::{BarWidget, BarWidgetImpl},
};

use alsa::mixer::{
    Mixer, Selem, SelemChannelId, SelemId,
};

#[inline]
fn alsa_volume_to_f32(volume: i64, min: i64, max: i64) -> f32 {
    (volume - min) as f32 / (max - min) as f32
}

#[inline]
fn f32_to_alsa_volume(volume: f32, min: i64, max: i64) -> i64 {
    min + ((max - min) as f32 * volume) as i64
}

fn get_master_volume(selem: &mut Selem) -> f32 {
    let (min, max) = selem.get_playback_volume_range();
    selem
        .get_playback_volume(SelemChannelId::mono())
        .map(|volume| alsa_volume_to_f32(volume, min, max))
        .unwrap()
}

fn get_mute(selem: &mut Selem) -> bool {
    let res = selem.get_playback_switch(SelemChannelId::FrontLeft);
    false
}

fn set_master_volume(selem: &mut Selem, volume: f32) {
    let (min, max) = selem.get_playback_volume_range();
    let volume = f32_to_alsa_volume(volume, min, max);
    selem.set_playback_volume_all(volume).unwrap();
}

fn start_monitor(inner: Arc<Mutex<Inner>>) {
    thread::Builder::new()
        .name("audiomon".to_string())
        .spawn(move || {
            let mixer = Mixer::new("default", true).unwrap();
            let id = SelemId::new("Master", 0);
            let mut selem = mixer.find_selem(&id).unwrap();

            let mut inner = inner.lock().unwrap();
            inner.volume = get_master_volume(&mut selem);
            inner.mute = get_mute(&mut selem);
        })
        .unwrap();
}

struct Inner {
    volume: f32,
    mute: bool,
}

pub struct AlsaAudio {
    inner: Arc<Mutex<Inner>>,
}

impl AlsaAudio {
    pub fn new(fm: &mut FontMap, font: &'static str, size: f32) -> BarWidget {
        let inner = Arc::new(Mutex::new(Inner{
            mute: false,
            volume: 0.,
        }));
        start_monitor(inner.clone());
        let dev = AlsaAudio{ inner };
        BarWidget::new(Box::new(dev), fm, font, size)
    }
}

impl BarWidgetImpl for AlsaAudio {
    fn name(&self) -> &'static str {
        "audio"
    }
    fn get_dirty(&self) -> bool {
        true
    }
    fn value(&mut self) -> f32 {
        let inner = self.inner.lock().unwrap();
        inner.volume
    }
    fn color(&self) -> Color {
        let inner = self.inner.lock().unwrap();
        if inner.mute {
            Color::DARKORANGE
        } else {
            Color::WHITE
        }
    }
    fn click(&mut self, pos: f32, btn: PointerButton) {
        let inner = self.inner.lock().unwrap();
        match btn {
//            PointerButton::Left =>  inner.set_master_volume(pos),
            //PointerButton::Right => Some(Change::ToggleMute),
            //PointerButton::ScrollVertical(val) => Some(Change::VolumeInc((val / 512.) as f32)),
            //PointerButton::ScrollHorizontal(val) => Some(Change::VolumeInc((val / 100.) as f32)),
            _ => (),
        };
    }

}
