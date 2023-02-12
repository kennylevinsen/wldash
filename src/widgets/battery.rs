use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
    error::Error,
};

use dbus::blocking::{
    stdintf::org_freedesktop_dbus::{Properties, PropertiesPropertiesChanged},
    LocalConnection,
};

use crate::{
    color::Color,
    event::{Event, Events},
    fonts::FontMap,
    widgets::bar_widget::{BarWidget, BarWidgetImpl},
};

enum UpowerBatteryState {
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
}

impl UpowerBatteryState {
    fn from_dbus(val: u64) -> UpowerBatteryState {
        match val {
            1 => UpowerBatteryState::Charging,
            2 => UpowerBatteryState::Discharging,
            3 => UpowerBatteryState::Empty,
            4 => UpowerBatteryState::FullyCharged,
            5 => UpowerBatteryState::PendingCharge,
            6 => UpowerBatteryState::PendingDischarge,
            _ => UpowerBatteryState::Unknown,
        }
    }
}

struct InnerBattery {
    value: f32,
    state: UpowerBatteryState,
    dirty: bool,
}

fn get_battery_state() -> Result<UpowerBatteryState, Box<dyn Error>> {
    let conn = LocalConnection::new_system()?;
    let device_path = "/org/freedesktop/UPower/devices/DisplayDevice";
    let proxy = conn.with_proxy(
        "org.freedesktop.UPower",
        device_path,
        Duration::from_millis(500),
    );
    let state: u32 = proxy
        .get("org.freedesktop.UPower.Device", "State")?;
    Ok(UpowerBatteryState::from_dbus(state as u64))
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
                inner.state = UpowerBatteryState::from_dbus(state as u64);
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
                                    inner.state =
                                        UpowerBatteryState::from_dbus(value.0.as_u64().unwrap());
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
    pub fn new(
        events: Arc<Mutex<Events>>,
        fm: &mut FontMap,
        font: &'static str,
        size: f32,
    ) -> BarWidget {
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

    pub fn detect() -> bool {
        match get_battery_state() {
            Ok(_) => true,
            Err(_) => false,
        }
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
            UpowerBatteryState::Charging | UpowerBatteryState::FullyCharged => Color::LIGHTGREEN,
            _ if inner.value > 0.25 => Color::WHITE,
            _ if inner.value > 0.1 => Color::DARKORANGE,
            _ => Color::RED,
        }
    }
}
