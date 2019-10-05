pub mod backlight;
pub mod bar_widget;
pub mod battery;
pub mod calendar;
pub mod clock;
pub mod date;
pub mod launcher;
pub mod networkmanager;

#[cfg(any(feature = "alsa-widget", feature = "pulseaudio-widget"))]
pub mod audio;
