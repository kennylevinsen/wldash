mod buffer;
mod color;
mod draw;
mod fonts;
mod keyboard;
mod utils;
mod widgets;

use wayland_client::{
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_shm, wl_shm_pool,
        wl_surface,
    },
    Connection, Dispatch, QueueHandle, WEnum, WaylandSource,
};

use wayland_protocols::xdg::{
    activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1},
    shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use calloop::{
    timer::{TimeoutAction, Timer},
    EventLoop,
};
use chrono::{Duration, Local, Timelike};

use buffer::{BufferManager, BufferView};
use color::Color;
use fonts::FontMap;
use keyboard::Keyboard;
use widgets::{
    Battery, Backlight, Clock, Date, Geometry, HorizontalLayout, IndexedLayout, Interface, Layout, Line,
    Margin, VerticalLayout, Widget, WidgetUpdater,
};

use std::{cell::RefCell, env, mem, rc::Rc, thread, process::exit};

enum MaybeFontMap {
    Waiting(thread::JoinHandle<FontMap>),
    Ready(Rc<RefCell<FontMap>>),
    Invalid,
}

impl MaybeFontMap {
    fn unwrap(&self) -> Rc<RefCell<FontMap>> {
        match self {
            MaybeFontMap::Ready(f) => f.clone(),
            _ => panic!("fontmap not yet ready"),
        }
    }

    fn resolve(&mut self) {
        if matches!(self, MaybeFontMap::Waiting(_)) {
            let s = mem::replace(self, MaybeFontMap::Invalid);
            match s {
                MaybeFontMap::Waiting(handle) => {
                    *self = MaybeFontMap::Ready(Rc::new(RefCell::new(handle.join().unwrap())));
                }
                _ => unreachable!(),
            }
        }
    }
}

struct State {
    running: bool,
    dirty: bool,
    activated: bool,
    base_surface: Option<wl_surface::WlSurface>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    xdg_surface: Option<(xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel)>,
    wl_shm: Option<wl_shm::WlShm>,
    xdg_activation: Option<xdg_activation_v1::XdgActivationV1>,
    configured: bool,
    dimensions: (i32, i32),
    bufmgr: BufferManager,
    widgets: Vec<Box<dyn Widget>>,
    keyboard: Keyboard,
    fonts: MaybeFontMap,
}

impl State {
    fn add_buffer(&mut self, qh: &QueueHandle<Self>) {
        self.bufmgr.add_buffer(
            self.wl_shm.as_ref().expect("missing wl_shm"),
            self.dimensions,
            qh,
        );
    }

    fn activate(&mut self, qh: &QueueHandle<Self>) {
        let key = match env::var("XDG_ACTIVATION_TOKEN") {
            Ok(token) => token,
            Err(_) => return,
        };
        match (&self.base_surface, &self.xdg_activation) {
            (Some(surface), Some(activation)) => {
                activation.activate(key, surface);
                let activation_token = activation.get_activation_token(qh, ());
                activation_token.commit();
                env::remove_var("XDG_ACTIVATION_TOKEN");
            }
            _ => (),
        }
    }
}

impl WidgetUpdater for State {
    fn geometry_update(
        &mut self,
        idx: usize,
        fonts: &mut FontMap,
        geometry: &Geometry,
    ) -> Geometry {
        self.widgets[idx].geometry_update(fonts, geometry)
    }
}

fn main() {
    let font_thread = thread::spawn(move || {
        let mut fm = FontMap::new();
        // Hard-coding font paths make things a lot faster
        fm.queue_font_path("sans", "/usr/share/fonts/noto/NotoSans-Regular.ttf", 96.);
        fm.queue_font_path("sans", "/usr/share/fonts/noto/NotoSans-Regular.ttf", 40.);
        fm.queue_font_path("sans", "/usr/share/fonts/noto/NotoSans-Regular.ttf", 24.);
        fm.queue_font_path(
            "monospace",
            "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
            32.,
        );
        fm.load_fonts();
        fm
    });
    let conn = Connection::connect_to_env().unwrap();

    let event_queue = conn.new_event_queue();
    let qhandle: QueueHandle<State> = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let bufmgr = BufferManager {
        buffers: Vec::new(),
    };
    
    let (ping_sender, ping_source) = calloop::ping::make_ping().unwrap();

    let clock = Box::new(Clock::new());
    let date = Box::new(Date::new());
    let line = Box::new(Line::new());
    let launcher = Box::new(Interface::new());
    let battery = Box::new(Battery::new(ping_sender));
    let backlight = Box::new(Backlight::new("intel_backlight"));

    let mut event_loop: EventLoop<State> =
        EventLoop::try_new().expect("Failed to initialize the event loop!");

    let handle = event_loop.handle();
    handle
        .insert_source(
            WaylandSource::new(event_queue).expect("Could not create WaylandSource!"),
            |_event, queue, mut state| queue.dispatch_pending(&mut state),
        )
        .expect("Failed to insert event source!");

    handle
        .insert_source(ping_source, |(), &mut (), state| {
            state.widgets[2].set_dirty(true);
            state.dirty = true;
        })
        .expect("Failed to insert ping source!");

    let clock_source = Timer::from_duration(std::time::Duration::from_secs(1));
    handle
        .insert_source(clock_source, |_event, _metadata, state| {
            let now = Local::now().naive_local();
            let target = (now + Duration::seconds(60))
                .with_second(0)
                .unwrap()
                .with_nanosecond(0)
                .unwrap();
            let d = target - now;
            state.widgets[0].set_dirty(true);
            state.widgets[1].set_dirty(true);
            state.dirty = true;

            // The timer event source requires us to return a TimeoutAction to
            // specify if the timer should be rescheduled. In our case we just drop it.
            TimeoutAction::ToDuration(d.to_std().unwrap())
        })
        .expect("Failed to insert event source!");

    let mut state = State {
        running: true,
        dirty: true,
        activated: false,
        base_surface: None,
        wm_base: None,
        xdg_surface: None,
        wl_shm: None,
        xdg_activation: None,
        configured: false,
        dimensions: (320, 240),
        bufmgr: bufmgr,
        widgets: vec![clock, date, battery, backlight, line, launcher],
        keyboard: Keyboard::new(),
        fonts: MaybeFontMap::Waiting(font_thread),
    };

    let mut damage = Vec::new();
    for _ in 0..state.widgets.len() {
        damage.push(Geometry::new());
    }
   
    while state.running {
        event_loop
            .dispatch(None, &mut state)
            .expect("Could not dispatch event loop");

        if !state.configured || !state.dirty {
            continue;
        }

        let mut drew = false;
        let mut force = false;
        if state.bufmgr.buffers.len() == 0 {
            state.add_buffer(&qhandle);
            force = true;
        }

        // We expect to see the usual shm optimization
        let buf = match state.bufmgr.next_buffer() {
            Some(b) => b,
            None => {
                // We are still waiting for our buffer
                continue;
            }
        };

        state.dirty = false;
        buf.acquire();
        let mut bufview = BufferView::new(
            &mut buf.mmap,
            (state.dimensions.0 as u32, state.dimensions.1 as u32),
        );

        let bg = Color::new(0., 0., 0., 1.);

        let surface = state.base_surface.as_ref().unwrap().clone();
        for (idx, widget) in state.widgets.iter_mut().enumerate() {
            if force || widget.get_dirty() {
                widget.set_dirty(false);
                drew = true;

                let geo = widget.geometry();
                let old_damage = damage[idx];

                if !force {
                    bufview.subgeometry(old_damage).unwrap().memset(&bg);
                } else {
                    bufview.subgeometry(geo).unwrap().memset(&bg);
                }

                let mut subview = bufview.subgeometry(geo).unwrap();
                let new_damage = widget.draw(&mut state.fonts.unwrap().borrow_mut(), &mut subview);
                let combined_damage = new_damage.expand(old_damage);
                damage[idx] = new_damage;
                surface.damage_buffer(
                    combined_damage.x as i32,
                    combined_damage.y as i32,
                    combined_damage.width as i32,
                    combined_damage.height as i32,
                );
                //draw_box(&mut subview, &Color::new(1.0, 0.5, 0.0, 1.0), (geo.width, geo.height));
            }
        }
        if !drew {
            buf.release();
            continue;
        }

        if force {
            surface.damage_buffer(0, 0, 0x7FFFFFFF, 0x7FFFFFFF)
        }
        surface.attach(Some(&buf.buffer), 0, 0);
        surface.commit();
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            match &interface[..] {
                "wl_compositor" => {
                    let compositor =
                        registry.bind::<wl_compositor::WlCompositor, _, _>(name, 4, qh, ());
                    let surface = compositor.create_surface(qh, ());
                    state.base_surface = Some(surface);
                    state.activate(qh);

                    if state.wm_base.is_some() && state.xdg_surface.is_none() {
                        state.init_xdg_surface(qh);
                    }
                }
                "wl_shm" => {
                    state.wl_shm = Some(registry.bind::<wl_shm::WlShm, _, _>(name, 1, qh, ()));
                }
                "wl_seat" => {
                    registry.bind::<wl_seat::WlSeat, _, _>(name, 1, qh, ());
                }
                "xdg_wm_base" => {
                    let wm_base = registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 1, qh, ());
                    state.wm_base = Some(wm_base);

                    if state.base_surface.is_some() && state.xdg_surface.is_none() {
                        state.init_xdg_surface(qh);
                    }
                }
                "xdg_activation_v1" => {
                    state.xdg_activation = Some(
                        registry.bind::<xdg_activation_v1::XdgActivationV1, _, _>(name, 1, qh, ()),
                    );
                    state.activate(qh);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm::WlShm, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for State {
    fn event(
        state: &mut Self,
        buffer: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        for buf in state.bufmgr.buffers.iter_mut() {
            if &buf.buffer == buffer {
                buf.release();
            }
        }
    }
}

impl State {
    fn init_xdg_surface(&mut self, qh: &QueueHandle<State>) {
        let wm_base = self.wm_base.as_ref().unwrap();
        let base_surface = self.base_surface.as_ref().unwrap();

        let xdg_surface = wm_base.get_xdg_surface(base_surface, qh, ());
        let toplevel = xdg_surface.get_toplevel(qh, ());
        toplevel.set_title("wldash".into());

        base_surface.commit();

        self.xdg_surface = Some((xdg_surface, toplevel));
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(
        state: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial, .. } = event {
            xdg_surface.ack_configure(serial);
            state.configured = true;
            state.dirty = true;
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for State {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, states, .. } => {
                if (width, height) == (0, 0) {
                    return;
                }
                let activated = states.iter().find(|&st| *st as u32 == xdg_toplevel::State::Activated as u32).is_some();

                if activated {
                    state.activated = true;
                } else if state.activated && !activated {
                    exit(0);
                }


                if state.dimensions != (width, height) {
                    state.bufmgr.clear_buffers();
                    state.dimensions = (width, height);
                    let mut layout = VerticalLayout {
                        widgets: vec![
                            Box::new(HorizontalLayout {
                                widgets: vec![
                                    Box::new(IndexedLayout { widget_idx: 0 }),
                                    Box::new(Margin {
                                        widget: Box::new(IndexedLayout { widget_idx: 1 }),
                                        margin: (16, 8, 0, 0),
                                    }),
                                    Box::new(VerticalLayout{
                                        widgets: vec![
                                            Box::new(Margin {
                                                widget: Box::new(IndexedLayout { widget_idx: 2 }),
                                                margin: (16, 8, 8, 0),
                                            }),
                                            Box::new(Margin {
                                                widget: Box::new(IndexedLayout { widget_idx: 3 }),
                                                margin: (16, 8, 8, 0),
                                            }),
                                        ],
                                    }),
                                ],
                            }),
                            Box::new(IndexedLayout { widget_idx: 4 }),
                            Box::new(IndexedLayout { widget_idx: 5 }),
                        ],
                    };

                    state.fonts.resolve();
                    let fonts = state.fonts.unwrap();
                    layout.geometry_update(
                        &mut fonts.borrow_mut(),
                        &Geometry {
                            x: 0,
                            y: 0,
                            width: width as u32,
                            height: height as u32,
                        },
                        state,
                    );
                }
            }
            xdg_toplevel::Event::Close => {
                state.running = false;
            }
            _ => (),
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        {
            if capabilities.contains(wl_seat::Capability::Keyboard) {
                seat.get_keyboard(qh, ());
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Key {
                serial,
                time,
                key,
                state: kbstate,
            } => {
                if key == 1 {
                    state.running = false;
                    return;
                }
                let k = state.keyboard.key(serial, time, key, kbstate);
                for widget in state.widgets.iter_mut() {
                    widget.keyboard_input(&k);
                }
                state.dirty = true;
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                state
                    .keyboard
                    .modifiers(mods_depressed, mods_latched, mods_locked, group);
            }
            wl_keyboard::Event::Keymap { format, fd, size } => {
                state.keyboard.keymap(format, fd, size);
            }
            _ => (),
        }
    }
}

impl Dispatch<xdg_activation_v1::XdgActivationV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &xdg_activation_v1::XdgActivationV1,
        _: xdg_activation_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &xdg_activation_token_v1::XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_activation_token_v1::Event::Done { token } => {
                for widget in state.widgets.iter_mut() {
                    widget.token_update(&token);
                }
            }
            _ => (),
        }
    }
}
