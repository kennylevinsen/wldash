use crate::buffer::Buffer;
use crate::color::Color;
use crate::draw::{draw_bar, draw_box, Font, ROBOTO_REGULAR};
use crate::modules::module::{Input, ModuleImpl};
use crate::cmd::Cmd;

use std::cell::RefCell;
use std::cmp::{max, min};
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use chrono::{DateTime, Local};

use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::{
    flags, introspect::ServerInfo, introspect::SinkInfo, subscribe::subscription_masks,
    subscribe::Facility, subscribe::Operation as SubscribeOperation, Context, State as PulseState,
};
use libpulse_binding::mainloop::standard::IterateResult;
use libpulse_binding::mainloop::standard::Mainloop;
use libpulse_binding::proplist::{properties, Proplist};
use libpulse_binding::volume::{ChannelVolumes, VOLUME_MAX, VOLUME_NORM};

struct PulseAudioConnection {
    mainloop: Rc<RefCell<Mainloop>>,
    context: Rc<RefCell<Context>>,
}

#[derive(Clone)]
struct PulseAudioSinkInfo {
    volume: ChannelVolumes,
    mute: bool,
}

struct PulseAudioClient {
    sender: Sender<PulseAudioClientRequest>,
    default_sink: String,
    sinks: HashMap<String, PulseAudioSinkInfo>,
    listener: Sender<bool>,
}

enum PulseAudioClientRequest {
    GetDefaultDevice(Option<Sender<bool>>),
    GetSinkInfoByIndex(Option<Sender<bool>>, u32),
    GetSinkInfoByName(Option<Sender<bool>>, String),
    SetSinkVolumeByName(Option<Sender<bool>>, String, ChannelVolumes),
    SetSinkMuteByName(Option<Sender<bool>>, String, bool),
}

struct PulseAudioSoundDevice {
    client: Arc<Mutex<PulseAudioClient>>,
    name: Option<String>,
    volume: Option<ChannelVolumes>,
    volume_avg: f32,
    muted: bool,
}

impl PulseAudioConnection {
    fn new() -> Result<Self, ::std::io::Error> {
        let mut proplist = Proplist::new().unwrap();
        proplist
            .sets(properties::APPLICATION_NAME, "wldash")
            .unwrap();

        let mainloop = Rc::new(RefCell::new(Mainloop::new().unwrap()));

        let context = Rc::new(RefCell::new(
            Context::new_with_proplist(mainloop.borrow().deref(), "wldash_context", &proplist)
                .unwrap(),
        ));

        context
            .borrow_mut()
            .connect(None, flags::NOFLAGS, None)
            .map_err(|_e| {
                ::std::io::Error::new(
                    ::std::io::ErrorKind::Other,
                    "unable to connect to pulseaudio context",
                )
            })?;

        let mut connection = PulseAudioConnection { mainloop, context };

        // Wait for context to be ready
        loop {
            connection.iterate(false)?;
            match connection.context.borrow().get_state() {
                PulseState::Ready => {
                    break;
                }
                PulseState::Failed | PulseState::Terminated => {}
                _ => {}
            }
        }

        Ok(connection)
    }

    fn iterate(&mut self, blocking: bool) -> Result<(), ::std::io::Error> {
        match self.mainloop.borrow_mut().iterate(blocking) {
            IterateResult::Quit(_) | IterateResult::Err(_) => Err(::std::io::Error::new(
                ::std::io::ErrorKind::Other,
                "unable to iterate pulseaudio state",
            )),
            IterateResult::Success(_) => Ok(()),
        }
    }
}

impl PulseAudioClient {
    fn new(listener: Sender<bool>) -> Result<Arc<Mutex<Self>>, ::std::io::Error> {
        let (tx, rx) = channel();

        let client = Arc::new(Mutex::new(PulseAudioClient {
            sender: tx,
            default_sink: "@DEFAULT_SINK@".to_string(),
            sinks: HashMap::new(),
            listener: listener,
        }));

        let cl1 = client.clone();
        let (tx1, rx1) = channel();
        thread::spawn(move || {
            let mut conn = match PulseAudioConnection::new() {
                Ok(v) => {
                    tx1.send(true).unwrap();
                    v
                }
                Err(_) => {
                    tx1.send(false).unwrap();
                    return;
                }
            };

            // make sure mainloop dispatched everything
            for _ in 0..10 {
                conn.iterate(false).unwrap();
            }

            loop {
                let cl11 = cl1.clone();
                let cl12 = cl1.clone();
                let cl13 = cl1.clone();
                match rx.recv() {
                    Err(_) => return,
                    Ok(req) => {
                        let mut introspector = conn.context.borrow_mut().introspect();

                        match req {
                            PulseAudioClientRequest::GetDefaultDevice(s) => {
                                introspector.get_server_info(move |info| {
                                    let _res = cl11.lock().unwrap().server_info_callback(info);
                                    if let Some(s) = &s {
                                        let _ = s.send(true);
                                    }
                                });
                            }
                            PulseAudioClientRequest::GetSinkInfoByIndex(s, index) => {
                                introspector.get_sink_info_by_index(index, move |res| {
                                    cl12.lock().unwrap().sink_info_callback(res);
                                    if let Some(s) = &s {
                                        let _ = s.send(true);
                                    }
                                });
                            }
                            PulseAudioClientRequest::GetSinkInfoByName(s, name) => {
                                introspector.get_sink_info_by_name(&name, move |res| {
                                    cl13.lock().unwrap().sink_info_callback(res);
                                    if let Some(s) = &s {
                                        let _ = s.send(true);
                                    }
                                });
                            }
                            PulseAudioClientRequest::SetSinkVolumeByName(s, name, volumes) => {
                                introspector.set_sink_volume_by_name(&name, &volumes, None);
                                if let Some(s) = &s {
                                    let _ = s.send(true);
                                }
                            }
                            PulseAudioClientRequest::SetSinkMuteByName(s, name, mute) => {
                                introspector.set_sink_mute_by_name(&name, mute, None);
                                if let Some(s) = &s {
                                    let _ = s.send(true);
                                }
                            }
                        };

                        // send request and receive response
                        conn.iterate(true).unwrap();
                        conn.iterate(true).unwrap();
                    }
                }
            }
        });

        // subscribe
        let cl2 = client.clone();
        let (tx2, rx2) = channel();
        thread::spawn(move || {
            let conn = match PulseAudioConnection::new() {
                Ok(v) => {
                    tx2.send(true).unwrap();
                    v
                }
                Err(_) => {
                    tx2.send(false).unwrap();
                    return;
                }
            };
            // subcribe for events
            conn.context
                .borrow_mut()
                .set_subscribe_callback(Some(Box::new(move |facility, operation, index| {
                    cl2.lock()
                        .unwrap()
                        .subscribe_callback(facility, operation, index)
                })));
            conn.context.borrow_mut().subscribe(
                subscription_masks::SERVER | subscription_masks::SINK,
                |_| {},
            );

            conn.mainloop.borrow_mut().run().unwrap();
        });

        if !rx1.recv().unwrap() || !rx2.recv().unwrap() {
            return Err(::std::io::Error::new(
                ::std::io::ErrorKind::Other,
                "unable to start pulseaudio thread",
            ));
        }

        Ok(client)
    }

    fn send(&self, request: PulseAudioClientRequest) -> Result<(), ::std::io::Error> {
        let res = self.sender.send(request).map_err(|_e| {
            ::std::io::Error::new(
                ::std::io::ErrorKind::Other,
                "unable to send pulseaudio request",
            )
        });
        res
    }

    fn server_info_callback(&mut self, server_info: &ServerInfo) {
        match server_info.default_sink_name.clone() {
            None => {}
            Some(default_sink) => {
                self.default_sink = default_sink.into();
            }
        }
    }

    fn sink_info_callback(&mut self, result: ListResult<&SinkInfo>) {
        match result {
            ListResult::End | ListResult::Error => {}
            ListResult::Item(sink_info) => match sink_info.name.clone() {
                None => {}
                Some(name) => {
                    let info = PulseAudioSinkInfo {
                        volume: sink_info.volume,
                        mute: sink_info.mute,
                    };
                    self.sinks.insert(name.into(), info);
                    self.listener.send(true).unwrap();
                }
            },
        }
    }

    fn subscribe_callback(
        &self,
        facility: Option<Facility>,
        _operation: Option<SubscribeOperation>,
        index: u32,
    ) {
        match facility {
            None => {}
            Some(facility) => match facility {
                Facility::Server => {
                    let _ = self.send(PulseAudioClientRequest::GetDefaultDevice(None));
                }
                Facility::Sink => {
                    let _ = self.send(PulseAudioClientRequest::GetSinkInfoByIndex(None, index));
                }
                _ => {}
            },
        }
    }
}

impl PulseAudioSoundDevice {
    fn new(listener: Sender<bool>) -> Result<Arc<Mutex<Self>>, ::std::io::Error> {
        let client = PulseAudioClient::new(listener)?;
        let cl = client.clone();
        let (tx, rx) = channel();
        {
            let cl = client.lock().unwrap();
            cl.send(PulseAudioClientRequest::GetDefaultDevice(Some(tx)))?;
        }
        rx.recv().unwrap();
        let name = {
            let cl = client.lock().unwrap();
            cl.default_sink.to_string()
        };
        let device = Arc::new(Mutex::new(PulseAudioSoundDevice {
            client: cl,
            name: Some(name.to_string()),
            volume: None,
            volume_avg: 0.0,
            muted: false,
        }));
        let (tx, rx) = channel();
        {
            let cl = client.lock().unwrap();
            cl.send(PulseAudioClientRequest::GetSinkInfoByName(Some(tx), name))?;
        }
        rx.recv().unwrap();
        device.lock().unwrap().get_info()?;

        Ok(device)
    }

    fn name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| self.client.lock().unwrap().default_sink.clone())
    }

    fn update_volume(&mut self, volume: ChannelVolumes) {
        self.volume = Some(volume);
        self.volume_avg = volume.avg().0 as f32 / VOLUME_NORM.0 as f32;
    }

    fn volume(&self) -> f32 {
        self.volume_avg
    }

    fn get_info(&mut self) -> Result<(), ::std::io::Error> {
        let sink_info = match self.client.lock().unwrap().sinks.get(&self.name()) {
            None => return Ok(()),
            Some(sink_info) => (*sink_info).clone(),
        };
        self.update_volume(sink_info.volume);
        self.muted = sink_info.mute;

        Ok(())
    }

    fn set_volume(&mut self, step: f32) -> Result<(), ::std::io::Error> {
        let mut volume = match self.volume {
            Some(volume) => volume,
            None => {
                return Err(::std::io::Error::new(
                    ::std::io::ErrorKind::Other,
                    "unable to set volume",
                ))
            }
        };

        // apply step to volumes
        let step = (step * VOLUME_NORM.0 as f32).round() as i32;
        for vol in volume.values.iter_mut() {
            vol.0 = min(max(0, vol.0 as i32 + step) as u32, VOLUME_MAX.0);
        }

        // update volumes
        self.update_volume(volume);
        self.client
            .lock()
            .unwrap()
            .send(PulseAudioClientRequest::SetSinkVolumeByName(
                None,
                self.name(),
                volume,
            ))?;

        Ok(())
    }

    fn toggle(&mut self) -> Result<(), ::std::io::Error> {
        self.muted = !self.muted;
        self.client
            .lock()
            .unwrap()
            .send(PulseAudioClientRequest::SetSinkMuteByName(
                None,
                self.name(),
                self.muted,
            ))?;

        Ok(())
    }
}

pub struct PulseAudio {
    device: Arc<Mutex<PulseAudioSoundDevice>>,
    font: Font,
    dirty: Arc<Mutex<bool>>,
}

impl PulseAudio {
    pub fn new(listener: Sender<Cmd>) -> Result<PulseAudio, ::std::io::Error> {
        let (tx, rx) = channel();
        let mut font = Font::new(&ROBOTO_REGULAR, 24.0);
        font.add_str_to_cache("volume");
        let pa = PulseAudio {
            device: PulseAudioSoundDevice::new(tx)?,
            font: font,
            dirty: Arc::new(Mutex::new(true)),
        };
        let device = pa.device.clone();
        let dirty = pa.dirty.clone();
        std::thread::spawn(move || loop {
            rx.recv().unwrap();
            device.lock().unwrap().get_info().unwrap();
            *(dirty.lock().unwrap()) = true;
            listener.send(Cmd::Draw).unwrap();
        });
        Ok(pa)
    }
}

impl ModuleImpl for PulseAudio {
    fn draw(
        &self,
        buf: &mut Buffer,
        bg: &Color,
        _time: &DateTime<Local>,
    ) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error> {
        let muted = self.device.lock().unwrap().muted;
        let mut vol = self.device.lock().unwrap().volume();
        buf.memset(bg);
        let c = if muted {
            Color::new(1.0, 1.0, 0.0, 1.0)
        } else {
            Color::new(1.0, 1.0, 1.0, 1.0)
        };
        self.font.draw_text(
            &mut buf.subdimensions((0, 0, 128, 24))?,
            bg,
            &Color::new(1.0, 1.0, 1.0, 1.0),
            "volume",
        )?;
        draw_bar(&mut buf.subdimensions((128, 0, 464, 24))?, &c, 464, 24, vol)?;
        let mut iter = 1.0;
        while vol > 1.0 {
            let c = &Color::new(0.75 / iter, 0.25 / iter, 0.25 / iter, 1.0);
            vol -= 1.0;
            iter += 1.0;
            draw_bar(&mut buf.subdimensions((128, 0, 464, 24))?, &c, 464, 24, vol)?;
        }
        draw_box(&mut buf.subdimensions((128, 0, 464, 24))?, &c, (464, 24))?;
        Ok(vec![buf.get_signed_bounds()])
    }

    fn update(&mut self, _time: &DateTime<Local>, force: bool) -> Result<bool, ::std::io::Error> {
        let mut d = self.dirty.lock().unwrap();
        if *d || force {
            *d = false;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn input(&mut self, input: Input) {
        match input {
            Input::Scroll {
                pos: _pos,
                x: _x,
                y,
            } => {
                self.device
                    .lock()
                    .unwrap()
                    .set_volume(y as f32 / -800.0)
                    .unwrap();
            }
            Input::Click { pos: _pos, button } => match button {
                273 => {
                    self.device.lock().unwrap().toggle().unwrap();
                }
                _ => {}
            },
            _ => {}
        }
    }
}
