use std::{
    sync::{Arc, Mutex},
    thread,
    error::Error,
};

use crate::{
    color::Color,
    event::{Event, Events},
    fonts::FontMap,
    widgets::bar_widget::{BarWidget, BarWidgetImpl},
};

use upower_dbus::BatteryState;

struct InnerBattery {
    value: f32,
    state: BatteryState,
    dirty: bool,
}

async fn monitor(inner: Arc<Mutex<InnerBattery>>, events: Arc<Mutex<Events>>) -> Result<(), Box<dyn Error>> {
    use zbus::Connection;
    use upower_dbus::UPowerProxy;
    use tokio_stream::StreamExt;

    let conn = Connection::system().await?;
    let proxy = UPowerProxy::new(&conn).await?;

    let device = proxy.get_display_device().await?;

    let local_inner = inner.clone();
    let local_events = events.clone();
    let mut percent_stream = device.receive_percentage_changed().await;
    let a = tokio::spawn(async move {
        while let Some(percent) = percent_stream.next().await {
            let percent = percent.get().await.unwrap() as f32 / 100.;
            let mut inner = local_inner.lock().unwrap();
            inner.value = percent;
            inner.dirty = true;
            drop(inner);

            let mut events = local_events.lock().unwrap();
            events.add_event(Event::PowerUpdate);
        }
    });

    let local_inner = inner.clone();
    let local_events = events.clone();
    let mut state_stream = device.receive_state_changed().await;
    let b = tokio::spawn(async move {
        while let Some(state) = state_stream.next().await {
            let state = state.get().await.unwrap();
            let mut inner = local_inner.lock().unwrap();
            inner.state = state;
            inner.dirty = true;
            drop(inner);

            let mut events = local_events.lock().unwrap();
            events.add_event(Event::PowerUpdate);
        }
    });
    a.await?;
    b.await?;

    Ok(())
}

fn start_monitor(inner: Arc<Mutex<InnerBattery>>, events: Arc<Mutex<Events>>) {
    use tokio::runtime;
    thread::Builder::new()
        .name("battmon".to_string())
        .spawn(move || {
            let rt = runtime::Builder::new_current_thread()
                .enable_io()
                .build()
                .unwrap();
            rt.block_on(async move { monitor(inner, events).await }).unwrap();
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
                state: BatteryState::Unknown,
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
    fn value(&self) -> f32 {
        let mut inner = self.inner.lock().unwrap();
        inner.dirty = false;
        inner.value
    }
    fn color(&self) -> Color {
        let inner = self.inner.lock().unwrap();
        match inner.state {
            BatteryState::Charging | BatteryState::FullyCharged => Color::LIGHTGREEN,
            BatteryState::PendingCharge => Color::LIGHTRED,
            _ => if inner.value > 0.25 {
                Color::WHITE
            } else if inner.value > 0.1 {
                Color::DARKORANGE
            } else {
                Color::RED
            }
        }
    }
}
