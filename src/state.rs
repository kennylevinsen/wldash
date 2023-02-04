use std::{
    env,
    process::exit,
    rc::Rc,
    sync::{Arc, Mutex},
};

use wayland_client::{
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_shm, wl_shm_pool,
        wl_surface, wl_pointer,
    },
    Connection, Dispatch, QueueHandle, WEnum,
};

use wayland_protocols::xdg::{
    activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1},
    shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1, zwlr_layer_surface_v1,
};

use crate::{
    buffer::BufferManager,
    fonts::{FontMap, MaybeFontMap},
    keyboard::Keyboard,
    widgets::{Geometry, Layout, Widget, WidgetUpdater},
    event::{Event, Events, PointerEvent, PointerButton},
};

pub enum OperationMode {
    LayerSurface((u32, u32)),
    XdgToplevel,
}

pub struct State {
    mode: OperationMode,
    pub running: bool,
    pub dirty: bool,
    activated: bool,
    pub needs_memset: bool,
    pub base_surface: Option<wl_surface::WlSurface>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    xdg_surface: Option<(xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel)>,
    wl_shm: Option<wl_shm::WlShm>,
    xdg_activation: Option<xdg_activation_v1::XdgActivationV1>,
    pub configured: bool,
    pub dimensions: (i32, i32),
    pub bufmgr: BufferManager,
    pub widgets: Vec<Box<dyn Widget>>,
    keyboard: Keyboard,
    pointer: Option<(f64, f64)>,
    pub fonts: MaybeFontMap,
    pub events: Arc<Mutex<Events>>,
    layout: Rc<Box<dyn Layout>>,
}

impl State {
    pub fn new(
        mode: OperationMode,
        widgets: Vec<Box<dyn Widget>>,
        layout: Rc<Box<dyn Layout>>,
        fonts: MaybeFontMap,
        events: Arc<Mutex<Events>>,
    ) -> State {
        State {
            running: true,
            dirty: true,
            activated: false,
            needs_memset: true,
            base_surface: None,
            layer_shell: None,
            layer_surface: None,
            wm_base: None,
            xdg_surface: None,
            wl_shm: None,
            xdg_activation: None,
            configured: false,
            dimensions: (320, 240),
            bufmgr: BufferManager::new(),
            keyboard: Keyboard::new(),
            pointer: None,
            mode,
            widgets,
            fonts,
            events,
            layout,
        }
    }

    pub fn add_buffer(&mut self, qh: &QueueHandle<Self>) {
        self.bufmgr.add_buffer(
            self.wl_shm.as_ref().expect("missing wl_shm"),
            self.dimensions,
            qh,
        );
    }

    pub fn activate(&mut self, qh: &QueueHandle<Self>) {
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

    fn init_xdg_surface(&mut self, qh: &QueueHandle<State>) {
        let wm_base = self.wm_base.as_ref().unwrap();
        let base_surface = self.base_surface.as_ref().unwrap();

        let xdg_surface = wm_base.get_xdg_surface(base_surface, qh, ());
        let toplevel = xdg_surface.get_toplevel(qh, ());
        toplevel.set_title("wldash".into());

        base_surface.commit();

        self.xdg_surface = Some((xdg_surface, toplevel));
    }

    fn init_layer_surface(&mut self, qh: &QueueHandle<State>, size: (u32, u32)) {
        let layer_shell = self.layer_shell.as_ref().unwrap();
        let base_surface = self.base_surface.as_ref().unwrap();

        let layer_surface = layer_shell.get_layer_surface(base_surface, None, zwlr_layer_shell_v1::Layer::Top, "launcher".to_string(), qh, ());
        layer_surface.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive);
        layer_surface.set_size(size.0, size.1);

        base_surface.commit();
        self.layer_surface = Some(layer_surface);
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
                        registry.bind::<wl_compositor::WlCompositor, _, _>(name, 5, qh, ());
                    let surface = compositor.create_surface(qh, ());
                    state.base_surface = Some(surface);
                    state.activate(qh);

                    if state.wm_base.is_some() && state.xdg_surface.is_none() && state.layer_surface.is_none() {
                        match state.mode {
                            OperationMode::XdgToplevel => state.init_xdg_surface(qh),
                            OperationMode::LayerSurface(size) => state.init_layer_surface(qh, size),
                        }
                    }
                }
                "wl_shm" => {
                    state.wl_shm = Some(registry.bind::<wl_shm::WlShm, _, _>(name, 1, qh, ()));
                }
                "wl_seat" => {
                    registry.bind::<wl_seat::WlSeat, _, _>(name, 8, qh, ());
                }
                "xdg_wm_base" => if let OperationMode::XdgToplevel = state.mode {
                    let wm_base = registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 2, qh, ());
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
                "zwlr_layer_shell_v1" => if let OperationMode::LayerSurface(size) = state.mode {
                    state.layer_shell = Some(
                        registry.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(name, 4, qh, ()),
                    );
                    if state.base_surface.is_some() && state.layer_surface.is_none() {
                        state.init_layer_surface(qh, size);
                    }
                },
                "clay_control_v1" => {
                    // Buffers are zeroed by default, which is equivalent to zero alpha black. This
                    // is not a particularly good background, but memsetting is slow. On clay,
                    // toplevels that adhere to their requested dimensions only have black behind
                    // them, so we can save the memset in this case, speeding things up.
                    state.needs_memset = false;
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

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for State {
    fn event(
        state: &mut Self,
        layer_surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure { serial, width, height } = event {
            if (width, height) == (0, 0) {
                return;
            }
            layer_surface.ack_configure(serial);
            state.configured = true;
            state.dirty = true;

            if state.dimensions != (width as i32, height as i32) {
                state.bufmgr.clear_buffers();
                state.dimensions = (width as i32, height as i32);
                state.fonts.resolve();
                let fonts = state.fonts.unwrap();
                let layout = state.layout.clone();
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
            xdg_toplevel::Event::Configure {
                width,
                height,
                states,
                ..
            } => {
                if (width, height) == (0, 0) {
                    return;
                }
                let activated = states
                    .iter()
                    .find(|&st| *st as u32 == xdg_toplevel::State::Activated as u32)
                    .is_some();

                if activated {
                    state.activated = true;
                } else if state.activated && !activated {
                    exit(0);
                }

                if state.dimensions != (width, height) {
                    state.bufmgr.clear_buffers();
                    state.dimensions = (width, height);
                    state.fonts.resolve();
                    let fonts = state.fonts.unwrap();
                    let layout = state.layout.clone();
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
            if capabilities.contains(wl_seat::Capability::Pointer) {
                seat.get_pointer(qh, ());
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
                let ev = Event::KeyEvent(k);
                for widget in state.widgets.iter_mut() {
                    widget.event(&ev);
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
                // TODO: Keymap loading blocks xdg configure, consider delaying
                state.keyboard.keymap(format, fd, size);
            }
            _ => (),
        }
    }
}

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { surface_x, surface_y, .. } => {
                state.pointer = Some((surface_x, surface_y));
            },
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                state.pointer = Some((surface_x, surface_y));
            },
            wl_pointer::Event::Leave { .. } => {
                state.pointer = None
            },
            wl_pointer::Event::Axis { axis, value, .. } =>  match state.pointer {
                Some((x, y)) => {
                    let axis = match axis {
                        WEnum::Value(wl_pointer::Axis::VerticalScroll) => PointerButton::ScrollVertical(value),
                        WEnum::Value(wl_pointer::Axis::HorizontalScroll) => PointerButton::ScrollHorizontal(value),
                        _ => return,
                    };
                    let pos = (x as u32, y as u32);
                    for widget in state.widgets.iter_mut() {
                        let geo = widget.geometry();
                        if geo.contains(pos) {
                            let pos = (pos.0 - geo.x, pos.1 - geo.y);
                            let ev = Event::PointerEvent(PointerEvent{
                                button: axis,
                                pos: pos,
                            });
                            widget.event(&ev);
                            state.dirty = true;
                            break;
                        }
                    }
                },
                _ => (),
            },
            wl_pointer::Event::Button { button, state: button_state, .. } => match (button, button_state, state.pointer) {
                (button, WEnum::Value(wl_pointer::ButtonState::Pressed), Some((x, y))) => {
                    let button = match button {
                        BTN_LEFT => PointerButton::Left,
                        BTN_RIGHT => PointerButton::Right,
                        BTN_MIDDLE => PointerButton::Middle,
                        _ => return,
                    };
                    let pos = (x as u32, y as u32);
                    for widget in state.widgets.iter_mut() {
                        let geo = widget.geometry();
                        if geo.contains(pos) {

                            let pos = (pos.0 - geo.x, pos.1 - geo.y);
                            let ev = Event::PointerEvent(PointerEvent{
                                button: button,
                                pos: pos,
                            });
                            widget.event(&ev);
                            state.dirty = true;
                            break;
                        }
                    }
                },
                _ => (),
            },
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
                let ev = Event::TokenUpdate(token);
                for widget in state.widgets.iter_mut() {
                    widget.event(&ev);
                }
            }
            _ => (),
        }
    }
}
