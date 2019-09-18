
#[cfg(feature="alsa-widget")]
mod alsa_sound;
#[cfg(feature="alsa-widget")]
pub use alsa_sound::Alsa;

#[cfg(feature="pulseaudio-widget")]
mod pulse_sound;
#[cfg(feature="pulseaudio-widget")]
pub use pulse_sound::PulseAudio;

