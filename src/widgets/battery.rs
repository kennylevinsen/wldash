use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use dbus::blocking::{
    stdintf::org_freedesktop_dbus::{Properties, PropertiesPropertiesChanged},
    LocalConnection,
};

use crate::{
    color::Color,
    fonts::FontMap,
    event::{Event, Events},
    widgets::bar_widget::{BarWidget, BarWidgetImpl},
};

enum UpowerBatteryState {
    Charging,
    Discharging,
    Empty,
    Full,
    NotCharging,
    Unknown,
}

struct InnerBattery {
    value: f32,
    state: UpowerBatteryState,
    dirty: bool,
}

fn start_monitor(inner: Arc<Mutex<InnerBattery>>, events: Arc<Mutex<Events>>) {
    thread::Builder::new()
        .name("battmon".to_string())
        .spawn(move || {
            let conn = LocalConnection::new_system().unwrap();
            let device_path = "/org/freedesktop/UPower/devices/DisplayDevice";
            let proxy = conn.with_proxy(
                "org.freedesktop.UPower",
                device_path,
                Duration::from_millis(500),
            );

            let capacity: f64 = proxy
                .get("org.freedesktop.UPower.Device", "Percentage")
                .expect("unable to get property");
            let state: u32 = proxy
                .get("org.freedesktop.UPower.Device", "State")
                .expect("unable to get state");

            {
                let mut inner = inner.lock().unwrap();
                inner.value = capacity as f32 / 100.;
                inner.state = match state {
                    1 => UpowerBatteryState::Charging,
                    2 => UpowerBatteryState::Discharging,
                    3 => UpowerBatteryState::Empty,
                    4 => UpowerBatteryState::Full,
                    5 => UpowerBatteryState::NotCharging,
                    6 => UpowerBatteryState::Discharging,
                    _ => UpowerBatteryState::Unknown,
                }
            }

            proxy
                .match_signal(
                    move |c: PropertiesPropertiesChanged,
                          _: &LocalConnection,
                          _: &dbus::Message| {
                        if c.interface_name != "org.freedesktop.UPower.Device" {
                            return true;
                        }
                        let mut inner = inner.lock().unwrap();
                        for (key, value) in c.changed_properties {
                            match key.as_str() {
                                "State" => {
                                    inner.state = match value.0.as_u64().unwrap() {
                                        1 => UpowerBatteryState::Charging,
                                        2 => UpowerBatteryState::Discharging,
                                        3 => UpowerBatteryState::Empty,
                                        4 => UpowerBatteryState::Full,
                                        5 => UpowerBatteryState::NotCharging,
                                        6 => UpowerBatteryState::Discharging,
                                        _ => UpowerBatteryState::Unknown,
                                    };
                                    inner.dirty = true;
                                    let mut events = events.lock().unwrap();
                                    events.add_event(Event::PowerUpdate);
                                }
                                "Percentage" => {
                                    inner.value = value.0.as_f64().unwrap() as f32 / 100.;
                                    inner.dirty = true;
                                    let mut events = events.lock().unwrap();
                                    events.add_event(Event::PowerUpdate);
                                }
                                _ => (),
                            }
                        }
                        true
                    },
                )
                .unwrap();

            loop {
                conn.process(Duration::from_millis(60000)).unwrap();
            }
        })
        .unwrap();
}

pub struct Battery {
    inner: Arc<Mutex<InnerBattery>>,
}

impl Battery {
    pub fn new(events: Arc<Mutex<Events>>, fm: &mut FontMap, font: &'static str, size: f32) -> BarWidget {
        let battery = Battery {
            inner: Arc::new(Mutex::new(InnerBattery {
                value: 0.,
                state: UpowerBatteryState::Unknown,
                dirty: false,
            })),
        };
        start_monitor(battery.inner.clone(), events);
        BarWidget::new(Box::new(battery), fm, font, size)
    }
}

impl BarWidgetImpl for Battery {
    fn get_dirty(&self) -> bool {
        self.inner.lock().unwrap().dirty
    }
    fn name(&self) -> &'static str {
        "battery"
    }
    fn value(&mut self) -> f32 {
        let mut inner = self.inner.lock().unwrap();
        inner.dirty = false;
        inner.value
    }
    fn color(&self) -> Color {
        let inner = self.inner.lock().unwrap();
        match inner.state {
            UpowerBatteryState::Charging | UpowerBatteryState::Full => Color::LIGHTGREEN,
            UpowerBatteryState::NotCharging => Color::LIGHTRED,
            _ if inner.value > 0.25 => Color::WHITE,
            _ if inner.value > 0.1 => Color::DARKORANGE,
            _ => Color::RED,
        }
    }
}
