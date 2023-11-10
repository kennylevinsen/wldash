use serde::{self, Deserialize, Serialize};

use std::collections::HashMap;
use std::default::Default;
use std::sync::{Arc, Mutex};

use crate::{
    event::Events,
    fonts::{find_font, FontMap},
    widgets::{
        Backlight, Battery, Calendar, Clock, Date, HorizontalLayout, IndexedLayout, Interface,
        InvertedHorizontalLayout, InvertedVerticalLayout, Layout, Line, Margin, PulseAudio,
        VerticalLayout, Widget as RealWidget,
    },
};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum OperationMode {
    LayerSurface((u32, u32)),
    XdgToplevel,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Widget {
    Margin {
        margins: (u32, u32, u32, u32),
        widget: Box<Widget>,
    },
    HorizontalLayout(Vec<Widget>),
    InvertedHorizontalLayout(Vec<Widget>),
    VerticalLayout(Vec<Widget>),
    InvertedVerticalLayout(Vec<Widget>),
    HorizontalLine(u32),
    VerticalLine(u32),
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
        sections_x: i32,
        sections_y: i32,
    },
    Launcher {
        font: Option<String>,
        font_size: f32,
    },
    Battery {
        font: Option<String>,
        font_size: f32,
    },
    Backlight {
        device: Option<String>,
        font: Option<String>,
        font_size: f32,
    },
    PulseAudio {
        font: Option<String>,
        font_size: f32,
    },
}

impl Default for OperationMode {
    fn default() -> OperationMode {
        OperationMode::LayerSurface((1024, 768))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    pub font_paths: Option<HashMap<String, String>>,
    pub widget: Widget,
    pub mode: OperationMode,
    pub background: Option<u32>,
    pub server: Option<bool>,
}

impl Config {
    pub fn generate_v1() -> Config {
        let has_battery = Battery::detect();
        let has_backlight = Backlight::detect("intel_backlight");
        let sans = find_font("sans");
        let monospace = find_font("monospace");

        let mut bars = Vec::new();
        if has_battery {
            bars.push(Widget::Margin {
                margins: (0, 0, 0, 8),
                widget: Box::new(Widget::Battery {
                    font: None,
                    font_size: 24.,
                }),
            });
        }
        if has_backlight {
            bars.push(Widget::Margin {
                margins: (0, 0, 0, 8),
                widget: Box::new(Widget::Backlight {
                    device: Some("intel_backlight".to_string()),
                    font: None,
                    font_size: 24.,
                }),
            });
        }
        bars.push(Widget::Margin {
            margins: (0, 0, 0, 8),
            widget: Box::new(Widget::PulseAudio {
                font: None,
                font_size: 24.,
            }),
        });

        let widget = Widget::Margin {
            margins: (20, 20, 20, 20),
            widget: Box::new(Widget::VerticalLayout(vec![
                Widget::HorizontalLayout(vec![
                    Widget::VerticalLayout(vec![
                        Widget::Date {
                            font: None,
                            font_size: 48.,
                        },
                        Widget::Clock {
                            font: None,
                            font_size: 256.,
                        },
                    ]),
                    Widget::Margin {
                        margins: (88, 0, 0, 0),
                        widget: Box::new(Widget::VerticalLayout(bars)),
                    },
                ]),
                Widget::Calendar {
                    font_primary: None,
                    font_secondary: None,
                    font_size: 36.,
                    sections_x: 3,
                    sections_y: 1,
                },
                Widget::Launcher {
                    font: None,
                    font_size: 32.,
                },
            ])),
        };

        let map = HashMap::from([
            ("sans".to_string(), sans),
            ("monospace".to_string(), monospace),
        ]);

        Config {
            font_paths: Some(map),
            widget: widget,
            mode: OperationMode::LayerSurface((1024, 768)),
            background: Some(0xFAFAFAFA),
            server: None,
        }
    }

    pub fn generate_v2(clay: bool) -> Config {
        let has_battery = Battery::detect();
        let has_backlight = Backlight::detect("intel_backlight");
        let sans = find_font("sans");
        let monospace = find_font("monospace");

        let mut bars = Vec::new();
        if has_battery {
            bars.push(Widget::Margin {
                margins: (16, 8, 8, 0),
                widget: Box::new(Widget::Battery {
                    font: None,
                    font_size: 24.,
                }),
            });
        }
        if has_backlight {
            bars.push(Widget::Margin {
                margins: (16, 8, 8, 0),
                widget: Box::new(Widget::Backlight {
                    device: Some("intel_backlight".to_string()),
                    font: None,
                    font_size: 24.,
                }),
            });
        }
        bars.push(Widget::Margin {
            margins: (16, 8, 8, 0),
            widget: Box::new(Widget::PulseAudio {
                font: None,
                font_size: 24.,
            }),
        });

        let widget = Widget::VerticalLayout(vec![
            Widget::HorizontalLayout(vec![
                Widget::Margin {
                    margins: (8, 0, 0, 0),
                    widget: Box::new(Widget::Clock {
                        font: None,
                        font_size: 128.,
                    }),
                },
                Widget::Margin {
                    margins: (16, 64, 0, 0),
                    widget: Box::new(Widget::Date {
                        font: None,
                        font_size: 48.,
                    }),
                },
                Widget::VerticalLayout(bars),
            ]),
            Widget::HorizontalLine(1),
            Widget::InvertedHorizontalLayout(vec![
                Widget::Margin {
                    margins: (16, 8, 16, 0),
                    widget: Box::new(Widget::Calendar {
                        font_primary: None,
                        font_secondary: None,
                        font_size: 24.,
                        sections_x: 1,
                        sections_y: if clay { -1 } else { 3 },
                    }),
                },
                Widget::VerticalLine(1),
                Widget::Margin {
                    margins: (8, 0, 0, 0),
                    widget: Box::new(Widget::Launcher {
                        font: None,
                        font_size: 32.,
                    }),
                },
            ]),
        ]);

        let map = HashMap::from([
            ("sans".to_string(), sans),
            ("monospace".to_string(), monospace),
        ]);

        Config {
            font_paths: Some(map),
            widget: widget,
            mode: if clay {
                OperationMode::XdgToplevel
            } else {
                OperationMode::LayerSurface((1024, 1))
            },
            background: Some(0xFAFAFAFA),
            server: None,
        }
    }
}

fn leak_or_default(v: Option<String>, d: &'static str) -> &'static str {
    match v {
        Some(v) => Box::leak(v.into_boxed_str()),
        None => d,
    }
}

impl Widget {
    pub fn construct_widgets(
        self,
        v: &mut Vec<Box<dyn RealWidget>>,
        mut fm: &mut FontMap,
        events: &Arc<Mutex<Events>>,
    ) {
        match self {
            Widget::Margin { widget, .. } => widget.construct_widgets(v, fm, events),
            Widget::HorizontalLayout(widgets) => widgets
                .into_iter()
                .for_each(|w| w.construct_widgets(v, fm, events)),
            Widget::InvertedHorizontalLayout(widgets) => widgets
                .into_iter()
                .for_each(|w| w.construct_widgets(v, fm, events)),
            Widget::VerticalLayout(widgets) => widgets
                .into_iter()
                .for_each(|w| w.construct_widgets(v, fm, events)),
            Widget::InvertedVerticalLayout(widgets) => widgets
                .into_iter()
                .for_each(|w| w.construct_widgets(v, fm, events)),
            Widget::HorizontalLine(width) => v.push(Box::new(Line::new(width, false))),
            Widget::VerticalLine(width) => v.push(Box::new(Line::new(width, true))),
            Widget::Clock { font, font_size } => v.push(Box::new(Clock::new(
                &mut fm,
                leak_or_default(font, "sans"),
                font_size,
            ))),
            Widget::Date { font, font_size } => v.push(Box::new(Date::new(
                &mut fm,
                leak_or_default(font, "sans"),
                font_size,
            ))),
            Widget::Calendar {
                font_primary,
                font_secondary,
                font_size,
                sections_x,
                sections_y,
            } => v.push(Box::new(Calendar::new(
                &mut fm,
                leak_or_default(font_primary, "monospace"),
                leak_or_default(font_secondary, "sans"),
                font_size,
                sections_x,
                sections_y,
            ))),
            Widget::Launcher { font, font_size } => v.push(Box::new(Interface::new(
                events.clone(),
                &mut fm,
                leak_or_default(font, "sans"),
                font_size,
            ))),
            Widget::Battery { font, font_size } => v.push(Box::new(Battery::new(
                events.clone(),
                &mut fm,
                leak_or_default(font, "sans"),
                font_size,
            ))),
            Widget::Backlight {
                device,
                font,
                font_size,
            } => v.push(Box::new(Backlight::new(
                leak_or_default(device, "intel_backlight"),
                &mut fm,
                leak_or_default(font, "sans"),
                font_size,
            ))),
            Widget::PulseAudio { font, font_size } => v.push(Box::new(PulseAudio::new(
                events.clone(),
                &mut fm,
                Box::leak(font.unwrap_or_else(|| "sans".to_string()).into_boxed_str()),
                font_size,
            ))),
        }
    }

    pub fn construct_layout(&self, idx: &mut usize) -> Box<dyn Layout> {
        match self {
            Widget::Margin { margins, widget } => {
                Margin::new(widget.construct_layout(idx), *margins)
            }
            Widget::HorizontalLayout(widgets) => {
                HorizontalLayout::new(widgets.iter().map(|w| w.construct_layout(idx)).collect())
            }
            Widget::InvertedHorizontalLayout(widgets) => InvertedHorizontalLayout::new(
                widgets.iter().map(|w| w.construct_layout(idx)).collect(),
            ),
            Widget::VerticalLayout(widgets) => {
                VerticalLayout::new(widgets.iter().map(|w| w.construct_layout(idx)).collect())
            }
            Widget::InvertedVerticalLayout(widgets) => InvertedVerticalLayout::new(
                widgets.iter().map(|w| w.construct_layout(idx)).collect(),
            ),
            _ => IndexedLayout::new({
                let i = *idx;
                *idx += 1;
                i
            }),
        }
    }
}
