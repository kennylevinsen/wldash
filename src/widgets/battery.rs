use crate::cmd::Cmd;
use crate::color::Color;
use crate::widgets::bar_widget::{BarWidget, BarWidgetImpl};

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;

use dbus;

fn get_upower_property(
    con: &dbus::Connection,
    device_path: &str,
    property: &str,
) -> Result<dbus::Message, ::std::io::Error> {
    let msg = dbus::Message::new_method_call(
        "org.freedesktop.UPower",
        device_path,
        "org.freedesktop.DBus.Properties",
        "Get",
    )
    .map_err(|_| {
        ::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            "could not send make dbus method call",
        )
    })?
    .append2(
        dbus::MessageItem::Str("org.freedesktop.UPower.Device".to_string()),
        dbus::MessageItem::Str(property.to_string()),
    );;

    con.send_with_reply_and_block(msg, 1000).map_err(|_| {
        ::std::io::Error::new(::std::io::ErrorKind::Other, "could not send dbus message")
    })
}

pub struct UpowerBattery {
    device_path: String,
    inner: Arc<Mutex<UpowerBatteryInner>>,
}

pub struct UpowerBatteryInner {
    state: UpowerBatteryState,
    capacity: f64,
}

enum UpowerBatteryState {
    Charging,
    Discharging,
    Empty,
    Full,
    NotCharging,
    Unknown,
}

impl UpowerBattery {
    pub fn from_device(device: &str) -> Result<Self, ::std::io::Error> {
        let device_path = format!("/org/freedesktop/UPower/devices/battery_{}", device);
        let con = dbus::Connection::get_private(dbus::BusType::System).map_err(|_| {
            ::std::io::Error::new(::std::io::ErrorKind::Other, "could not get dbus connection")
        })?;

        let upower_type: dbus::arg::Variant<u32> =
            match get_upower_property(&con, &device_path, "Type")?.get1() {
                Some(v) => v,
                None => {
                    return Err(::std::io::Error::new(
                        ::std::io::ErrorKind::Other,
                        "no such upower device",
                    ));
                }
            };

        // https://upower.freedesktop.org/docs/Device.html#Device:Type
        if upower_type.0 != 2 {
            return Err(::std::io::Error::new(
                ::std::io::ErrorKind::Other,
                "UPower device is not a battery.",
            ));
        }

        let capacity: f64 = match get_upower_property(&con, &device_path, "Percentage")?
            .get1::<dbus::arg::Variant<f64>>()
        {
            Some(v) => v.0,
            None => {
                return Err(::std::io::Error::new(
                    ::std::io::ErrorKind::Other,
                    "no such upower device",
                ));
            }
        };
        let state: UpowerBatteryState = match get_upower_property(&con, &device_path, "State")?
            .get1::<dbus::arg::Variant<u32>>()
        {
            Some(v) => match v.0 {
                1 => UpowerBatteryState::Charging,
                2 => UpowerBatteryState::Discharging,
                3 => UpowerBatteryState::Empty,
                4 => UpowerBatteryState::Full,
                5 => UpowerBatteryState::NotCharging,
                6 => UpowerBatteryState::Discharging,
                _ => UpowerBatteryState::Unknown,
            },
            None => {
                return Err(::std::io::Error::new(
                    ::std::io::ErrorKind::Other,
                    "no such upower device",
                ));
            }
        };

        Ok(UpowerBattery {
            device_path,
            inner: Arc::new(Mutex::new(UpowerBatteryInner {
                capacity: capacity,
                state: state,
            })),
        })
    }

    pub fn new(
        font_size: f32,
        length: u32,
        sender: Sender<Cmd>,
    ) -> Result<Box<BarWidget>, ::std::io::Error> {
        BarWidget::new(font_size, length, move |dirty| {
            let d = UpowerBattery::from_device("BAT0")?;
            let path = d.device_path.clone();
            let inner = d.inner.clone();
            let _ = thread::Builder::new()
                .name("battery_monitor".to_string())
                .spawn(move || {
                    let con = dbus::Connection::get_private(dbus::BusType::System)
                        .expect("Failed to establish D-Bus connection.");
                    let rule = format!(
                        "type='signal',\
                         path='{}',\
                         interface='org.freedesktop.DBus.Properties',\
                         member='PropertiesChanged'",
                        path
                    );

                    // First we're going to get an (irrelevant) NameAcquired event.
                    con.incoming(10_000).next();

                    con.add_match(&rule)
                        .expect("Failed to add D-Bus match rule.");

                    loop {
                        if con.incoming(10_000).next().is_some() {
                            let capacity = get_upower_property(&con, &path, "Percentage")
                                .unwrap()
                                .get1::<dbus::arg::Variant<f64>>()
                                .unwrap()
                                .0;
                            let state = match get_upower_property(&con, &path, "State")
                                .unwrap()
                                .get1::<dbus::arg::Variant<u32>>()
                                .unwrap()
                                .0
                            {
                                1 => UpowerBatteryState::Charging,
                                2 => UpowerBatteryState::Discharging,
                                3 => UpowerBatteryState::Empty,
                                4 => UpowerBatteryState::Full,
                                5 => UpowerBatteryState::NotCharging,
                                6 => UpowerBatteryState::Discharging,
                                _ => UpowerBatteryState::Unknown,
                            };
                            let mut inner = inner.lock().unwrap();
                            inner.state = state;
                            inner.capacity = capacity;
                            *dirty.lock().unwrap() = true;
                            sender.send(Cmd::Draw).unwrap();
                        }
                    }
                });

            Ok(Box::new(d))
        })
    }
}

impl BarWidgetImpl for UpowerBattery {
    fn name(&self) -> &str {
        "battery"
    }
    fn value(&self) -> f32 {
        let inner = self.inner.lock().unwrap();
        (inner.capacity as f32) / 100.0
    }
    fn color(&self) -> Color {
        let inner = self.inner.lock().unwrap();
        match inner.state {
            UpowerBatteryState::Discharging | UpowerBatteryState::Unknown => {
                if inner.capacity > 10.0 {
                    Color::new(1.0, 1.0, 1.0, 1.0)
                } else {
                    Color::new(1.0, 0.5, 0.0, 1.0)
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
    fn inc(&mut self, _: f32) {}
    fn set(&mut self, _: f32) {}
    fn toggle(&mut self) {}
}
