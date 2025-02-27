use std::{
    cell::RefCell,
    error::Error,
    os::fd::RawFd,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
};

use nix::unistd::{pipe, read, write};

use crate::{
    color::Color,
    event::{Event, Events, PointerButton},
    fonts::FontMap,
    widgets::bar_widget::{BarWidget, BarWidgetImpl},
};

use libpulse_binding::volume::ChannelVolumes;

fn monitor(
    events: Arc<Mutex<Events>>,
    inner: Arc<Mutex<InnerAudio>>,
    pipe: RawFd,
) -> Result<(), Box<dyn Error>> {
    use libpulse_binding::{
        callbacks::ListResult,
        context::{
            subscribe::{Facility, InterestMaskSet},
            Context, FlagSet, State,
        },
        mainloop::{
            self, api::Mainloop, events::io::FlagSet as IoFlagSet, standard::IterateResult,
        },
        proplist::{properties, Proplist},
        volume::Volume,
    };

    let mut proplist = Proplist::new().unwrap();
    proplist
        .set_str(properties::APPLICATION_NAME, "wldash")
        .unwrap();

    let mut mainloop = mainloop::standard::Mainloop::new().unwrap();
    let context = Rc::new(RefCell::new(
        Context::new_with_proplist(&mainloop, "wldash_context", &proplist).unwrap(),
    ));
    context.borrow_mut().connect(None, FlagSet::NOFLAGS, None)?;

    loop {
        mainloop.iterate(false);
        match context.borrow().get_state() {
            State::Ready => break,
            State::Failed | State::Terminated => panic!("damn"),
            _ => (),
        }
    }

    let local_context = context.clone();
    let local_inner = inner.clone();
    let local_events = events.clone();
    context.borrow_mut().set_subscribe_callback(Some(Box::new(
        move |facility, _operation, index| match facility {
            Some(Facility::Sink) => {
                let introspector = local_context.borrow_mut().introspect();
                let inner = local_inner.clone();
                let events = local_events.clone();
                introspector.get_sink_info_by_index(index, move |res| match res {
                    ListResult::End | ListResult::Error => (),
                    ListResult::Item(sink_info) => {
                        if let Some(_) = &sink_info.name {
                            let avg = sink_info.volume.avg().0 as f32 / Volume::NORMAL.0 as f32;
                            let mut inner = inner.lock().unwrap();
                            inner.volume = avg;
                            inner.mute = sink_info.mute;
                            inner.dirty = true;
                            drop(inner);
                            let mut events = events.lock().unwrap();
                            events.add_event(Event::AudioUpdate);
                        }
                    }
                });
            }
            _ => (),
        },
    )));

    context
        .borrow_mut()
        .subscribe(InterestMaskSet::SINK, |_| {});

    let introspector = context.borrow_mut().introspect();
    let local_inner = inner.clone();
    introspector.get_sink_info_by_name("@DEFAULT_SINK@", move |res| match res {
        ListResult::End | ListResult::Error => (),
        ListResult::Item(sink_info) => {
            if let Some(_) = &sink_info.name {
                let avg = sink_info.volume.avg().0 as f32 / Volume::NORMAL.0 as f32;
                let mut inner = local_inner.lock().unwrap();
                inner.volume = avg;
                inner.channel_volume = Some(sink_info.volume);
                inner.mute = sink_info.mute;
                inner.dirty = true;
                drop(inner);
                let mut events = events.lock().unwrap();
                events.add_event(Event::AudioUpdate);
            }
        }
    });
    drop(introspector);

    let local_context = context.clone();
    let local_inner = inner.clone();
    let event_source = mainloop.new_io_event(
        pipe as i32,
        IoFlagSet::INPUT,
        Box::new(move |_, _, _| {
            let mut buf = [0; 8];
            _ = read(pipe, &mut buf);

            let mut inner = local_inner.lock().unwrap();
            let mut introspector = local_context.borrow_mut().introspect();
            match inner.last_click.take() {
                Some(Change::Volume(volume)) => {
                    // do something
                    let mut vol = inner.channel_volume.unwrap();
                    vol.scale(Volume((volume * Volume::NORMAL.0 as f32).round() as u32));
                    introspector.set_sink_volume_by_name("@DEFAULT_SINK@", &vol, None);
                }
                Some(Change::VolumeInc(value)) => {
                    let mut vol = inner.channel_volume.unwrap();
                    // apply step to volumes
                    let step = (value * Volume::NORMAL.0 as f32).round() as i32;
                    if step > 0 {
                        vol.inc_clamp(Volume(step as u32), Volume::NORMAL);
                    } else {
                        vol.decrease(Volume(-step as u32));
                    }
                    // HACK: It would be better to only update the volume from feedback, but axis
                    // events fire fast and require the last set volume immediately for reference.
                    inner.channel_volume = Some(vol);
                    introspector.set_sink_volume_by_name("@DEFAULT_SINK@", &vol, None);
                }
                Some(Change::ToggleMute) => {
                    introspector.set_sink_mute_by_name("@DEFAULT_SINK@", !inner.mute, None);
                }
                None => (),
            }
        }),
    );

    loop {
        match mainloop.iterate(true) {
            IterateResult::Quit(_) | IterateResult::Err(_) => break,
            _ => (),
        }
    }

    // Need to keep the event source alive
    _ = event_source;

    Ok(())
}

fn start_monitor(events: Arc<Mutex<Events>>, inner: Arc<Mutex<InnerAudio>>, pipe: RawFd) {
    thread::Builder::new()
        .name("audiomon".to_string())
        .spawn(move || monitor(events, inner, pipe).unwrap())
        .unwrap();
}

enum Change {
    Volume(f32),
    VolumeInc(f32),
    ToggleMute,
}

struct InnerAudio {
    volume: f32,
    channel_volume: Option<ChannelVolumes>,
    mute: bool,
    dirty: bool,
    last_click: Option<Change>,
    pipe: RawFd,
}

pub struct PulseAudio {
    inner: Arc<Mutex<InnerAudio>>,
}

impl PulseAudio {
    pub fn new(
        events: Arc<Mutex<Events>>,
        fm: &mut FontMap,
        font: &'static str,
        size: f32,
    ) -> BarWidget {
        let (a, b) = pipe().unwrap();
        let audio = PulseAudio {
            inner: Arc::new(Mutex::new(InnerAudio {
                volume: 0.0,
                channel_volume: None,
                mute: false,
                dirty: false,
                last_click: None,
                pipe: b,
            })),
        };
        start_monitor(events, audio.inner.clone(), a);
        BarWidget::new(Box::new(audio), fm, font, size)
    }
}

impl BarWidgetImpl for PulseAudio {
    fn get_dirty(&self) -> bool {
        self.inner.lock().unwrap().dirty
    }
    fn name(&self) -> &'static str {
        "volume"
    }
    fn value(&mut self) -> f32 {
        let mut inner = self.inner.lock().unwrap();
        inner.dirty = false;
        inner.volume
    }
    fn color(&self) -> Color {
        match self.inner.lock().unwrap().mute {
            true => Color::YELLOW,
            false => Color::WHITE,
        }
    }
    fn click(&mut self, pos: f32, btn: PointerButton) {
        let mut inner = self.inner.lock().unwrap();
        inner.last_click = match btn {
            PointerButton::Left => Some(Change::Volume(pos)),
            PointerButton::Right => Some(Change::ToggleMute),
            PointerButton::ScrollVertical(val) => Some(Change::VolumeInc((val / 512.) as f32)),
            PointerButton::ScrollHorizontal(val) => Some(Change::VolumeInc((val / 100.) as f32)),
            _ => None,
        };
        _ = write(inner.pipe, &[0]);
    }
}
