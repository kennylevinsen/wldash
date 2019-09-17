use crate::cmd::Cmd;
use crate::color::Color;
use crate::widget;
use crate::widgets;
use serde::{Deserialize, Serialize};
use std::default::Default;
use std::sync::mpsc::Sender;

#[derive(Serialize, Deserialize, Debug)]
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
    HorizontalLayout(Vec<Box<Widget>>),
    VerticalLayout(Vec<Box<Widget>>),
    Clock {
        font_size: f32,
    },
    Date {
        font_size: f32,
    },
    Calendar {
        font_size: f32,
        sections: u32,
    },
    Launcher {
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
        font_size: f32,
        length: u32,
    },
    Backlight {
        font_size: f32,
        length: u32,
    },
    #[cfg(feature="pulseaudio")]
    PulseAudio {
        font_size: f32,
        length: u32,
    },
    #[cfg(feature="alsasound")]
    AlsaSound {
        font_size: f32,
        length: u32
    }
}

impl Widget {
    pub fn construct(self, tx: Sender<Cmd>) -> Option<Box<dyn widget::Widget + Send>> {
        match self {
            Widget::Margin { margins, widget } => match widget.construct(tx.clone()) {
                Some(w) => Some(widget::Margin::new(margins, w)),
                None => None,
            },
            Widget::Fixed {
                width,
                height,
                widget,
            } => match widget.construct(tx.clone()) {
                Some(w) => Some(widget::Fixed::new((width, height), w)),
                None => None,
            },
            Widget::HorizontalLayout(widgets) => Some(widget::HorizontalLayout::new(
                widgets
                    .into_iter()
                    .map(|x| x.construct(tx.clone()))
                    .filter(|x| x.is_some())
                    .map(|x| x.unwrap())
                    .collect(),
            )),
            Widget::VerticalLayout(widgets) => Some(widget::VerticalLayout::new(
                widgets
                    .into_iter()
                    .map(|x| x.construct(tx.clone()))
                    .filter(|x| x.is_some())
                    .map(|x| x.unwrap())
                    .collect(),
            )),
            Widget::Clock { font_size } => Some(widgets::clock::Clock::new(font_size, tx.clone())),
            Widget::Date { font_size } => Some(widgets::date::Date::new(font_size)),
            Widget::Calendar {
                font_size,
                sections,
            } => Some(widgets::calendar::Calendar::new(font_size, sections)),
            Widget::Launcher {
                font_size,
                length,
                app_opener,
                term_opener,
                url_opener,
            } => Some(widgets::launcher::Launcher::new(
                font_size,
                length,
                tx.clone(),
                app_opener,
                term_opener,
                if url_opener.len() == 0 {
                    "xdg_open ".to_string()
                } else {
                    url_opener
                },
            )),
            Widget::Battery { font_size, length } => {
                match widgets::battery::UpowerBattery::new(font_size, length, tx.clone()) {
                    Ok(w) => Some(w),
                    Err(_) => None,
                }
            }
            Widget::Backlight { font_size, length } => {
                match widgets::backlight::Backlight::new(font_size, length) {
                    Ok(w) => Some(w),
                    Err(_) => None,
                }
            }
            #[cfg(feature="pulseaudio")]
            Widget::PulseAudio { font_size, length } => {
                match widgets::audio::PulseAudio::new(font_size, length, tx.clone()) {
                    Ok(w) => Some(w),
                    Err(_) => None,
                }
            }
            #[cfg(feature="alsasound")]
            Widget::AlsaSound { font_size, length } => {
                match widgets::audio::Alsa::new(font_size, length) {
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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            widget: Widget::Margin {
                margins: (20, 20, 20, 20),
                widget: Box::new(Widget::VerticalLayout(vec![
                    Box::new(Widget::HorizontalLayout(vec![
                        Box::new(Widget::Margin {
                            margins: (0, 88, 0, 32),
                            widget: Box::new(Widget::VerticalLayout(vec![
                                Box::new(Widget::Date { font_size: 64.0 }),
                                Box::new(Widget::Clock { font_size: 256.0 }),
                            ])),
                        }),
                        Box::new(Widget::VerticalLayout(vec![
                            Box::new(Widget::Margin {
                                margins: (0, 0, 0, 8),
                                widget: Box::new(Widget::Battery {
                                    font_size: 24.0,
                                    length: 600,
                                }),
                            }),
                            Box::new(Widget::Margin {
                                margins: (0, 0, 0, 8),
                                widget: Box::new(Widget::Backlight {
                                    font_size: 24.0,
                                    length: 600,
                                }),
                            }),
                            Box::new(Widget::Margin {
                                margins: (0, 0, 0, 8),
                                widget: Box::new(Widget::PulseAudio {
                                    font_size: 24.0,
                                    length: 600,
                                }),
                            }),
                        ])),
                    ])),
                    Box::new(Widget::Calendar {
                        font_size: 16.0,
                        sections: 3,
                    }),
                    Box::new(Widget::Launcher {
                        font_size: 32.0,
                        length: 1200,
                        app_opener: "".to_string(),
                        term_opener: "".to_string(),
                        url_opener: "".to_string(),
                    }),
                ])),
            },
            output_mode: Default::default(),
            scale: 1,
            background: Color::new(0.0, 0.0, 0.0, 0.9),
        }
    }
}
