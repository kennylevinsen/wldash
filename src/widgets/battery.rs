use std::{
    thread,
    time::Duration,
    sync::{Arc, Mutex},
};

use calloop::ping::Ping;

use dbus::blocking::{
    LocalConnection,
    stdintf::org_freedesktop_dbus::{
        Properties,
        PropertiesPropertiesChanged,
    },
};

use crate::{
    color::Color,
    widgets::bar_widget::{
        BarWidget,
        BarWidgetImpl,
    },
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
}

fn start_monitor(inner: Arc<Mutex<InnerBattery>>, ping: Ping) {
    thread::Builder::new()
        .name("battmon".to_string())
        .spawn(move || {
        let conn = LocalConnection::new_system().unwrap();
        let device_path = "/org/freedesktop/UPower/devices/DisplayDevice";
        let proxy = conn.with_proxy("org.freedesktop.UPower", device_path, Duration::from_millis(500));

        let capacity: f64 = proxy.get("org.freedesktop.UPower.Device", "Percentage").expect("unable to get property");
        let state: u32 = proxy.get("org.freedesktop.UPower.Device", "State").expect("unable to get state");

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


        proxy.match_signal(move |c: PropertiesPropertiesChanged, _: &LocalConnection, _: &dbus::Message| {
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
                        ping.ping();
                    },
                    "Percentage" => {
                        inner.value = value.0.as_f64().unwrap() as f32 / 100.;
                        ping.ping();
                    },
                    _ => (),
                }
            }
            true
        }).unwrap();

        loop { conn.process(Duration::from_millis(60000)).unwrap(); }
    }).unwrap();
}

pub struct Battery {
    inner: Arc<Mutex<InnerBattery>>,
    dirty: bool,
}

impl Battery {
    pub fn new(ping: Ping) -> BarWidget {
        let battery = Battery{
            inner: Arc::new(Mutex::new(InnerBattery{
                value: 0.,
                state: UpowerBatteryState::Unknown,
            })),
            dirty: false,
        };
        start_monitor(battery.inner.clone(), ping);
        BarWidget::new(Box::new(battery))
    }
}


impl BarWidgetImpl for Battery {
    fn get_dirty(&self) -> bool {
        self.dirty
    }
    fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty
    }
    fn name(&self) -> &'static str {
        "battery"
    }
    fn value(&self) -> f32 {
        let inner = self.inner.lock().unwrap();
        inner.value
    }
    fn color(&self) -> Color {
        let inner = self.inner.lock().unwrap();
        match inner.state {
            UpowerBatteryState::Discharging | UpowerBatteryState::Unknown => {
                if inner.value > 0.25 {
                    Color::new(1.0, 1.0, 1.0, 1.0)
                } else if inner.value > 0.1 {
                    Color::new(1.0, 0.5, 0.0, 1.0)
                } else {
                    Color::new(1.0, 0.0, 0.0, 1.0)
                }
            }
            UpowerBatteryState::Charging | UpowerBatteryState::Full => {
                Color::new(0.5, 1.0, 0.5, 1.0)
            }
            UpowerBatteryState::NotCharging | UpowerBatteryState::Empty => {
                Color::new(1.0, 0.5, 0.5, 1.0)
            }
        }
    }
}
