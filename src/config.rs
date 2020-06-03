use crate::cmd::Cmd;
use crate::color::Color;
use crate::widget;
use crate::{
    fonts::{FontMap, FontRef},
    widgets,
};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::default::Default;
use std::{collections::HashMap, sync::mpsc::Sender};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Widget {
    Margin {
        margins: (u32, u32, u32, u32),
        widget: Box<Widget>,
    },
    Fixed {
        width: u32,
        height: u32,
        widget: Box<Widget>,
    },
    HorizontalLayout(Vec<Widget>),
    VerticalLayout(Vec<Widget>),
    Clock {
        font: Option<String>,
        font_size: f32,
    },
    Date {
        font: Option<String>,
        font_size: f32,
    },
    Calendar {
        font_primary: Option<String>,
        font_secondary: Option<String>,
        font_size: f32,
        sections: u32,
    },
    Launcher {
        font: Option<String>,
        font_size: f32,
        length: u32,
        #[serde(default)]
        app_opener: String,
        #[serde(default)]
        term_opener: String,
        #[serde(default)]
        url_opener: String,
    },
    Battery {
        font: Option<String>,
        font_size: f32,
        length: u32,
    },
    Backlight {
        #[serde(default)]
        device: String,
        font: Option<String>,
        font_size: f32,
        length: u32,
    },
    #[cfg(feature = "pulseaudio-widget")]
    PulseAudio {
        font: Option<String>,
        font_size: f32,
        length: u32,
    },
    #[cfg(feature = "alsa-widget")]
    AlsaSound {
        font: Option<String>,
        font_size: f32,
        length: u32,
    },
}

impl Widget {
    pub fn construct<'a>(
        self,
        time: NaiveDateTime,
        tx: Sender<Cmd>,
        fonts: &'a FontMap,
    ) -> Option<Box<dyn widget::Widget + Send + 'a>> {
        match self {
            Widget::Margin { margins, widget } => match widget.construct(time, tx, fonts) {
                Some(w) => Some(widget::Margin::new(margins, w)),
                None => None,
            },
            Widget::Fixed {
                width,
                height,
                widget,
            } => match widget.construct(time, tx, fonts) {
                Some(w) => Some(widget::Fixed::new((width, height), w)),
                None => None,
            },
            Widget::HorizontalLayout(widgets) => Some(widget::HorizontalLayout::new(
                widgets
                    .into_iter()
                    .map(|x| x.construct(time, tx.clone(), fonts))
                    .filter(|x| x.is_some())
                    .map(|x| x.unwrap())
                    .collect(),
            )),
            Widget::VerticalLayout(widgets) => Some(widget::VerticalLayout::new(
                widgets
                    .into_iter()
                    .map(|x| x.construct(time, tx.clone(), fonts))
                    .filter(|x| x.is_some())
                    .map(|x| x.unwrap())
                    .collect(),
            )),
            Widget::Clock { font, font_size } => Some(widgets::clock::Clock::new(
                time,
                get_font(&font.or_else(|| Some("sans".to_string())).unwrap(), &fonts),
                font_size,
            )),
            Widget::Date { font, font_size } => Some(widgets::date::Date::new(
                time,
                get_font(&font.or_else(|| Some("sans".to_string())).unwrap(), &fonts),
                font_size,
            )),
            Widget::Calendar {
                font_primary,
                font_secondary,
                font_size,
                sections,
            } => Some(widgets::calendar::Calendar::new(
                time,
                get_font(
                    &font_primary.or_else(|| Some("sans".to_string())).unwrap(),
                    &fonts,
                ),
                get_font(
                    &font_secondary.or_else(|| Some("mono".to_string())).unwrap(),
                    &fonts,
                ),
                font_size,
                sections,
            )),
            Widget::Launcher {
                font,
                font_size,
                length,
                app_opener,
                term_opener,
                url_opener,
            } => Some(widgets::launcher::Launcher::new(
                get_font(&font.or_else(|| Some("sans".to_string())).unwrap(), &fonts),
                font_size,
                length,
                tx,
                app_opener,
                term_opener,
                if url_opener.is_empty() {
                    "xdg_open ".to_string()
                } else {
                    url_opener
                },
            )),
            Widget::Battery {
                font,
                font_size,
                length,
            } => {
                match widgets::battery::UpowerBattery::new(
                    get_font(&font.or_else(|| Some("sans".to_string())).unwrap(), &fonts),
                    font_size,
                    length,
                    tx,
                ) {
                    Ok(w) => Some(w),
                    Err(_) => None,
                }
            }
            Widget::Backlight {
                device,
                font,
                font_size,
                length,
            } => {
                let d = if device == "" {
                    "intel_backlight"
                } else {
                    &device
                };
                match widgets::backlight::Backlight::new(
                    d,
                    get_font(&font.or_else(|| Some("sans".to_string())).unwrap(), &fonts),
                    font_size,
                    length,
                ) {
                    Ok(w) => Some(w),
                    Err(_) => None,
                }
            }
            #[cfg(feature = "pulseaudio-widget")]
            Widget::PulseAudio {
                font,
                font_size,
                length,
            } => {
                match widgets::audio::PulseAudio::new(
                    get_font(&font.or_else(|| Some("sans".to_string())).unwrap(), &fonts),
                    font_size,
                    length,
                    tx,
                ) {
                    Ok(w) => Some(w),
                    Err(_) => None,
                }
            }
            #[cfg(feature = "alsa-widget")]
            Widget::AlsaSound {
                font,
                font_size,
                length,
            } => {
                match widgets::audio::Alsa::new(
                    get_font(&font.or_else(|| Some("sans".to_string())).unwrap(), &fonts),
                    font_size,
                    length,
                ) {
                    Ok(w) => Some(w),
                    Err(_) => None,
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum OutputMode {
    All,
    Active,
}

impl Default for OutputMode {
    fn default() -> Self {
        OutputMode::Active
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub output_mode: OutputMode,
    pub scale: u32,
    pub background: Color,
    pub widget: Widget,
    pub fonts: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            widget: Widget::Margin {
                margins: (20, 20, 20, 20),
                widget: Box::new(Widget::VerticalLayout(vec![
                    Widget::HorizontalLayout(vec![
                        Widget::Margin {
                            margins: (0, 88, 0, 32),
                            widget: Box::new(Widget::VerticalLayout(vec![
                                Widget::Date {
                                    font: None,
                                    font_size: 64.0,
                                },
                                Widget::Clock {
                                    font: None,
                                    font_size: 256.0,
                                },
                            ])),
                        },
                        Widget::VerticalLayout(vec![
                            Widget::Margin {
                                margins: (0, 0, 0, 8),
                                widget: Box::new(Widget::Battery {
                                    font: None,
                                    font_size: 24.0,
                                    length: 600,
                                }),
                            },
                            Widget::Margin {
                                margins: (0, 0, 0, 8),
                                widget: Box::new(Widget::Backlight {
                                    device: "intel_backlight".to_string(),
                                    font: None,
                                    font_size: 24.0,
                                    length: 600,
                                }),
                            },
                            #[cfg(feature = "pulseaudio-widget")]
                            Widget::Margin {
                                margins: (0, 0, 0, 8),
                                widget: Box::new(Widget::PulseAudio {
                                    font: None,
                                    font_size: 24.0,
                                    length: 600,
                                }),
                            },
                        ]),
                    ]),
                    Widget::Calendar {
                        font_primary: None,
                        font_secondary: None,
                        font_size: 16.0,
                        sections: 3,
                    },
                    Widget::Launcher {
                        font: None,
                        font_size: 32.0,
                        length: 1200,
                        app_opener: "".to_string(),
                        term_opener: "".to_string(),
                        url_opener: "".to_string(),
                    },
                ])),
            },
            output_mode: Default::default(),
            scale: 1,
            background: Color::new(0.0, 0.0, 0.0, 0.9),
            fonts: {
                let mut map = HashMap::with_capacity(2);
                map.insert("mono".to_string(), "mono".to_string());
                map.insert("sans".to_string(), "sans".to_string());
                map
            },
        }
    }
}

#[inline]
fn get_font<'a>(name: &str, map: &'a FontMap) -> FontRef<'a> {
    match map.get(name) {
        Some(f) => f,
        None => panic!(format!("Font {} is missing from the config", name)),
    }
}
