use crate::cmd::Cmd;
use crate::color::Color;
use crate::widget::WaitContext;
use crate::{fonts::FontRef, widgets::bar_widget::{BarWidget, BarWidgetImpl}};

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::{
    flags, introspect::ServerInfo, introspect::SinkInfo, subscribe::subscription_masks,
    subscribe::Facility, subscribe::Operation as SubscribeOperation, Context, State as PulseState,
};
use libpulse_binding::mainloop::standard::IterateResult;
use libpulse_binding::mainloop::standard::Mainloop;
use libpulse_binding::proplist::{properties, Proplist};
use libpulse_binding::volume::{ChannelVolumes, Volume, VOLUME_MAX, VOLUME_NORM};

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
}

enum PulseAudioClientRequest {
    GetDefaultDevice(Option<Sender<bool>>),
    GetSinkInfoByIndex(Option<Sender<bool>>, u32),
    GetSinkInfoByName(Option<Sender<bool>>, String),
    SetSinkVolumeByName(Option<Sender<bool>>, String, ChannelVolumes),
    SetSinkMuteByName(Option<Sender<bool>>, String, bool),
}

#[derive(Debug)]
struct PulseAudioSoundDeviceInner {
    name: Option<String>,
    volume: Option<ChannelVolumes>,
    volume_avg: f32,
    muted: bool,
    default_sink: String,
}

struct PulseAudioSoundDevice {
    client: Arc<Mutex<PulseAudioClient>>,
    inner: Arc<Mutex<PulseAudioSoundDeviceInner>>,
}

impl PulseAudioConnection {
    fn new() -> Result<Self, ::std::io::Error> {
        let mut proplist = Proplist::new().unwrap();
        proplist
            .set_str(properties::APPLICATION_NAME, "wldash")
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
    fn new<F>(listener: F) -> Result<Arc<Mutex<Self>>, ::std::io::Error>
    where
        F: Fn(Arc<Mutex<Self>>) -> (),
        F: Send + 'static + Clone,
    {
        let (tx, rx) = channel();

        let client = Arc::new(Mutex::new(PulseAudioClient {
            sender: tx,
            default_sink: "@DEFAULT_SINK@".to_string(),
            sinks: HashMap::new(),
        }));

        let loop_client = client.clone();
        let (tx1, rx1) = channel();
        let _ = thread::Builder::new()
            .name("pa_writer".to_string())
            .spawn(move || {
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
                    let cl = loop_client.clone();
                    let l = listener.clone();
                    match rx.recv() {
                        Err(_) => return,
                        Ok(req) => {
                            let mut introspector = conn.context.borrow_mut().introspect();

                            match req {
                                PulseAudioClientRequest::GetDefaultDevice(s) => {
                                    introspector.get_server_info(move |info| {
                                        PulseAudioClient::server_info_callback(
                                            cl.clone(),
                                            l.clone(),
                                            info,
                                        );
                                        if let Some(s) = &s {
                                            let _ = s.send(true);
                                        }
                                    });
                                }
                                PulseAudioClientRequest::GetSinkInfoByIndex(s, index) => {
                                    introspector.get_sink_info_by_index(index, move |res| {
                                        PulseAudioClient::sink_info_callback(
                                            cl.clone(),
                                            l.clone(),
                                            res,
                                        );
                                        if let Some(s) = &s {
                                            let _ = s.send(true);
                                        }
                                    });
                                }
                                PulseAudioClientRequest::GetSinkInfoByName(s, name) => {
                                    introspector.get_sink_info_by_name(&name, move |res| {
                                        PulseAudioClient::sink_info_callback(
                                            cl.clone(),
                                            l.clone(),
                                            res,
                                        );
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
        let _ = thread::Builder::new()
            .name("pa_reader".to_string())
            .spawn(move || {
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

        let pa_writer_res = rx1.recv().unwrap();
        let pa_reader_res = rx2.recv().unwrap();

        if !pa_writer_res || !pa_reader_res {
            return Err(::std::io::Error::new(
                ::std::io::ErrorKind::Other,
                "unable to start pulseaudio thread",
            ));
        }

        Ok(client)
    }

    fn send(&self, request: PulseAudioClientRequest) -> Result<(), ::std::io::Error> {
        self.sender.send(request).map_err(|_e| {
            ::std::io::Error::new(
                ::std::io::ErrorKind::Other,
                "unable to send pulseaudio request",
            )
        })
    }

    fn server_info_callback<F>(s: Arc<Mutex<Self>>, listener: F, server_info: &ServerInfo)
    where
        F: Fn(Arc<Mutex<Self>>) -> (),
        F: Send + 'static,
    {
        match server_info.default_sink_name.clone() {
            None => {}
            Some(default_sink) => {
                (*s.lock().unwrap()).default_sink = default_sink.into();
                listener(s);
            }
        }
    }

    fn sink_info_callback<F>(s: Arc<Mutex<Self>>, listener: F, result: ListResult<&SinkInfo>)
    where
        F: Fn(Arc<Mutex<Self>>) -> (),
        F: Send + 'static,
    {
        match result {
            ListResult::End | ListResult::Error => {}
            ListResult::Item(sink_info) => match sink_info.name.clone() {
                None => {}
                Some(name) => {
                    let info = PulseAudioSinkInfo {
                        volume: sink_info.volume,
                        mute: sink_info.mute,
                    };
                    s.lock().unwrap().sinks.insert(name.into(), info);
                    listener(s);
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
    fn new<F>(listener: F) -> Result<Self, ::std::io::Error>
    where
        F: Fn() -> (),
        F: Send + 'static + Clone,
    {
        let inner = Arc::new(Mutex::new(PulseAudioSoundDeviceInner {
            name: None,
            volume: None,
            volume_avg: 0.0,
            muted: false,
            default_sink: "@DEFAULT_SINK@".to_string(),
        }));

        let cb_inner = inner.clone();
        let client = PulseAudioClient::new(move |client| {
            let mut inner = cb_inner.lock().unwrap();
            inner.default_sink = client.lock().unwrap().default_sink.clone();
            let name = inner
                .name
                .clone()
                .unwrap_or_else(|| inner.default_sink.clone());
            let sink_info = match client.lock().unwrap().sinks.get(&name) {
                None => return,
                Some(sink_info) => (*sink_info).clone(),
            };

            inner.volume = Some(sink_info.volume);
            inner.volume_avg = sink_info.volume.avg().0 as f32 / VOLUME_NORM.0 as f32;
            inner.muted = sink_info.mute;

            listener();
        })?;

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
        (*inner.lock().unwrap()).name = Some(name.clone());
        let device = PulseAudioSoundDevice { client: cl, inner };
        let (tx, rx) = channel();
        {
            let cl = client.lock().unwrap();
            cl.send(PulseAudioClientRequest::GetSinkInfoByName(Some(tx), name))?;
        }
        rx.recv().unwrap();

        Ok(device)
    }

    fn muted(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.muted
    }

    fn volume(&self) -> f32 {
        let inner = self.inner.lock().unwrap();
        inner.volume_avg
    }

    fn inc_volume(&mut self, step: f32) -> Result<(), ::std::io::Error> {
        let mut inner = self.inner.lock().unwrap();
        let mut volume = match inner.volume {
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
        if step > 0 {
            volume.inc_clamp(Volume(step as u32), VOLUME_MAX);
        } else {
            volume.decrease(Volume(-step as u32));
        }

        let name = inner
            .name
            .clone()
            .unwrap_or_else(|| inner.default_sink.clone());

        // update volumes
        inner.volume = Some(volume);
        inner.volume_avg = volume.avg().0 as f32 / VOLUME_NORM.0 as f32;
        self.client
            .lock()
            .unwrap()
            .send(PulseAudioClientRequest::SetSinkVolumeByName(
                None, name, volume,
            ))?;
        Ok(())
    }

    fn set_volume(&mut self, val: f32) -> Result<(), ::std::io::Error> {
        let mut inner = self.inner.lock().unwrap();
        let mut volume = match inner.volume {
            Some(volume) => volume,
            None => {
                return Err(::std::io::Error::new(
                    ::std::io::ErrorKind::Other,
                    "unable to set volume",
                ))
            }
        };

        // apply step to volumes
        volume.scale(Volume((val * VOLUME_NORM.0 as f32).round() as u32));

        let name = inner
            .name
            .clone()
            .unwrap_or_else(|| inner.default_sink.clone());

        // update volumes
        inner.volume = Some(volume);
        inner.volume_avg = volume.avg().0 as f32 / VOLUME_NORM.0 as f32;
        self.client
            .lock()
            .unwrap()
            .send(PulseAudioClientRequest::SetSinkVolumeByName(
                None, name, volume,
            ))?;
        Ok(())
    }

    fn toggle(&mut self) -> Result<(), ::std::io::Error> {
        let mut inner = self.inner.lock().unwrap();
        inner.muted = !inner.muted;
        self.client
            .lock()
            .unwrap()
            .send(PulseAudioClientRequest::SetSinkMuteByName(
                None,
                inner
                    .name
                    .clone()
                    .unwrap_or_else(|| inner.default_sink.clone()),
                inner.muted,
            ))?;
        Ok(())
    }
}

pub struct PulseAudio {
    device: PulseAudioSoundDevice,
}

impl PulseAudio {
    pub fn new(
        font: FontRef,
        font_size: f32,
        length: u32,
        sender: Sender<Cmd>,
    ) -> Result<Box<BarWidget>, ::std::io::Error> {
        BarWidget::new(font, font_size, length, move |dirty| {
            let device = PulseAudioSoundDevice::new(move || {
                *dirty.lock().unwrap() = true;
                sender.send(Cmd::Draw).unwrap();
            })?;
            Ok(Box::new(PulseAudio { device }))
        })
    }
}

impl BarWidgetImpl for PulseAudio {
    fn wait(&mut self, _: &mut WaitContext) {}
    fn name(&self) -> &str {
        "volume"
    }
    fn value(&self) -> f32 {
        self.device.volume()
    }
    fn color(&self) -> Color {
        let muted = self.device.muted();
        if muted {
            Color::new(1.0, 1.0, 0.0, 1.0)
        } else {
            Color::new(1.0, 1.0, 1.0, 1.0)
        }
    }
    fn inc(&mut self, inc: f32) {
        self.device.inc_volume(inc).unwrap();
    }
    fn set(&mut self, val: f32) {
        self.device.set_volume(val).unwrap();
    }
    fn toggle(&mut self) {
        self.device.toggle().unwrap();
    }
}
