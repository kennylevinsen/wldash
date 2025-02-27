use std::{
    cmp::max,
    default::Default,
    env,
    rc::Rc,
    sync::{Arc, Mutex},
};

use wayland_client::{
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat,
        wl_shm, wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface,
    },
    Connection, Dispatch, QueueHandle, WEnum,
};

use wayland_protocols::{
    wp::{
        single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1,
        viewporter::client::{wp_viewport, wp_viewporter},
    },
    xdg::{
        activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1},
        shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
    },
};

use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::{
    buffer::BufferManager,
    config::OperationMode,
    event::{Event, Events, PointerButton, PointerEvent},
    fonts::{FontMap, MaybeFontMap},
    keyboard::{Keyboard, RepeatMessage},
    widgets::{Geometry, Layout, Widget, WidgetUpdater},
};

use calloop::channel::Sender;

#[derive(Default)]
pub struct MainSurface {
    pub wl_surface: Option<wl_surface::WlSurface>,
    xdg_surface: Option<(xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel)>,
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
}

#[derive(Default)]
pub struct BackgroundSurface {
    wl_surface: Option<wl_surface::WlSurface>,
    subsurface: Option<wl_subsurface::WlSubsurface>,
    viewport: Option<wp_viewport::WpViewport>,
}

#[derive(Default)]
pub struct Protocols {
    wl_compositor: Option<wl_compositor::WlCompositor>,
    wl_subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    wl_shm: Option<wl_shm::WlShm>,
    xdg_wm_base: Option<xdg_wm_base::XdgWmBase>,
    xdg_activation: Option<xdg_activation_v1::XdgActivationV1>,
    zwlr_layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    viewporter: Option<wp_viewporter::WpViewporter>,
    wp_single_pixel_buffer_manager:
        Option<wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1>,
}

pub struct State {
    mode: OperationMode,
    background: Option<u32>,
    pub main_surface: MainSurface,
    pub bg_surface: BackgroundSurface,
    pub protocols: Protocols,
    pub running: bool,
    pub ready: bool,
    pub dirty: bool,
    activated: bool,
    pub configured: bool,
    pub dimensions: (i32, i32),
    pub bufmgr: BufferManager,
    pub widgets: Vec<Box<dyn Widget>>,
    pub keyboard: Keyboard,
    pub keyrepeat_sender: Sender<RepeatMessage>,
    pointer: Option<(f64, f64)>,
    pub fonts: MaybeFontMap,
    pub events: Arc<Mutex<Events>>,
    layout: Rc<Box<dyn Layout>>,
}

impl State {
    pub fn new(
        mode: OperationMode,
        background: Option<u32>,
        widgets: Vec<Box<dyn Widget>>,
        layout: Rc<Box<dyn Layout>>,
        fonts: MaybeFontMap,
        events: Arc<Mutex<Events>>,
        keyrepeat_sender: Sender<RepeatMessage>,
    ) -> State {
        State {
            protocols: Default::default(),
            bg_surface: Default::default(),
            main_surface: Default::default(),
            running: true,
            dirty: true,
            activated: false,
            configured: false,
            dimensions: (320, 240),
            bufmgr: BufferManager::new(),
            keyboard: Keyboard::new(),
            pointer: None,
            keyrepeat_sender,
            mode,
            background,
            widgets,
            fonts,
            events,
            layout,
            ready: false,
        }
    }

    pub fn add_buffer(&mut self, qh: &QueueHandle<Self>) {
        self.bufmgr.add_buffer(
            self.protocols.wl_shm.as_ref().expect("missing wl_shm"),
            self.dimensions,
            qh,
        );
    }

    pub fn activate(&mut self, qh: &QueueHandle<Self>) {
        let key = match env::var("XDG_ACTIVATION_TOKEN") {
            Ok(token) => token,
            Err(_) => return,
        };
        match (
            &self.main_surface.wl_surface,
            &self.protocols.xdg_activation,
        ) {
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

    fn minimum_size(&mut self, idx: usize, fonts: &mut FontMap) -> Geometry {
        self.widgets[idx].minimum_size(fonts)
    }
}

impl State {
    pub fn check_registry(&mut self, qh: &QueueHandle<Self>) {
        let wl_compositor = self
            .protocols
            .wl_compositor
            .as_ref()
            .expect("wl_compositor missing");
        let main_surface = wl_compositor.create_surface(qh, ());
        let bg_surface = wl_compositor.create_surface(qh, ());
        self.main_surface.wl_surface = Some(main_surface.clone());
        self.bg_surface.wl_surface = Some(bg_surface.clone());
        self.activate(qh);

        let wl_subcompositor = self
            .protocols
            .wl_subcompositor
            .as_ref()
            .expect("wl_subcompositor missing");
        let subsurface = wl_subcompositor.get_subsurface(&bg_surface, &main_surface, qh, ());
        subsurface.place_below(&main_surface);
        self.bg_surface.subsurface = Some(subsurface.clone());

        match self.mode {
            OperationMode::XdgToplevel => {
                self.fonts.resolve();
                let xdg_wm_base = self
                    .protocols
                    .xdg_wm_base
                    .as_ref()
                    .expect("xdg_wm_base missing");
                let xdg_surface = xdg_wm_base.get_xdg_surface(&main_surface, qh, ());
                let toplevel = xdg_surface.get_toplevel(qh, ());
                toplevel.set_title("wldash".into());
                main_surface.commit();

                self.main_surface.xdg_surface = Some((xdg_surface, toplevel));
            }
            OperationMode::LayerSurface(size) => {
                self.fonts.resolve();
                let layer_shell = self
                    .protocols
                    .zwlr_layer_shell
                    .as_ref()
                    .expect("zwlr_layer_shell_v1 missing");
                let layer_surface = layer_shell.get_layer_surface(
                    &main_surface,
                    None,
                    zwlr_layer_shell_v1::Layer::Overlay,
                    "launcher".to_string(),
                    qh,
                    (),
                );
                let fonts = self.fonts.unwrap();
                let layout = self.layout.clone();
                let min_size = layout.minimum_size(&mut fonts.borrow_mut(), self);
                layer_surface.set_keyboard_interactivity(
                    zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive,
                );
                layer_surface.set_anchor(
                    zwlr_layer_surface_v1::Anchor::Left
                        .union(zwlr_layer_surface_v1::Anchor::Bottom)
                        .union(zwlr_layer_surface_v1::Anchor::Right)
                        .union(zwlr_layer_surface_v1::Anchor::Top),
                );
                layer_surface.set_size(max(size.0, min_size.width), max(size.1, min_size.height));

                main_surface.commit();
                self.main_surface.layer_surface = Some(layer_surface);
            }
        }

        let viewporter = self
            .protocols
            .viewporter
            .as_ref()
            .expect("wp_viewporter missing");
        let viewport = viewporter.get_viewport(&bg_surface, qh, ());
        self.bg_surface.viewport = Some(viewport);

        if let Some(background) = self.background {
            let wp_single_pixel_buffer_manager = self
                .protocols
                .wp_single_pixel_buffer_manager
                .as_ref()
                .expect("wp_single_pixel_buffer_manager_v1 missing");
            let buffer =
                wp_single_pixel_buffer_manager.create_u32_rgba_buffer(0, 0, 0, background, qh, ());
            bg_surface.attach(Some(&buffer), 0, 0);
            bg_surface.damage_buffer(0, 0, 1, 1);
        }
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
                    state.protocols.wl_compositor =
                        Some(registry.bind::<wl_compositor::WlCompositor, _, _>(name, 5, qh, ()));
                }
                "wl_subcompositor" => {
                    state.protocols.wl_subcompositor = Some(
                        registry.bind::<wl_subcompositor::WlSubcompositor, _, _>(name, 1, qh, ()),
                    );
                }
                "wl_shm" => {
                    state.protocols.wl_shm =
                        Some(registry.bind::<wl_shm::WlShm, _, _>(name, 1, qh, ()));
                }
                "wl_seat" => {
                    registry.bind::<wl_seat::WlSeat, _, _>(name, 8, qh, ());
                }
                "xdg_wm_base" => {
                    if let OperationMode::XdgToplevel = state.mode {
                        state.protocols.xdg_wm_base =
                            Some(registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 2, qh, ()));
                    }
                }
                "xdg_activation_v1" => {
                    state.protocols.xdg_activation = Some(
                        registry.bind::<xdg_activation_v1::XdgActivationV1, _, _>(name, 1, qh, ()),
                    );
                }
                "zwlr_layer_shell_v1" => {
                    if let OperationMode::LayerSurface(_) = state.mode {
                        state.protocols.zwlr_layer_shell = Some(
                            registry.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(
                                name,
                                4,
                                qh,
                                (),
                            ),
                        );
                    }
                }
                "wp_viewporter" => {
                    state.protocols.viewporter =
                        Some(registry.bind::<wp_viewporter::WpViewporter, _, _>(name, 1, qh, ()));
                }
                "wp_single_pixel_buffer_manager_v1" => {
                    state.protocols.wp_single_pixel_buffer_manager = Some(registry.bind::<wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1, _, _>(name, 1, qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_callback::Event::Done { callback_data: _ } => state.ready = true,
            _ => {}
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

impl Dispatch<wl_subcompositor::WlSubcompositor, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_subcompositor::WlSubcompositor,
        _: wl_subcompositor::Event,
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

impl Dispatch<wl_subsurface::WlSubsurface, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_subsurface::WlSubsurface,
        _: wl_subsurface::Event,
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
    ) {
    }
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
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                mut width,
                mut height,
            } => {
                if (width, height) == (0, 0) {
                    if state.configured {
                        return;
                    }
                    (width, height) = (640, 480);
                }
                layer_surface.ack_configure(serial);
                state.configured = true;
                state.dirty = true;

                let dim = (width as i32, height as i32);

                if state.configured && state.dimensions == dim {
                    return;
                }
                state.bufmgr.clear_buffers();
                state.dimensions = dim;
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

                let surface = state
                    .bg_surface
                    .wl_surface
                    .as_ref()
                    .expect("bg surface was not ready");
                let viewport = state
                    .bg_surface
                    .viewport
                    .as_ref()
                    .expect("bg viewport was not ready");
                viewport.set_destination(dim.0, dim.1);
                surface.commit();
            }
            zwlr_layer_surface_v1::Event::Closed => {
                std::process::exit(0);
            }
            _ => (),
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
                mut width,
                mut height,
                states,
                ..
            } => {
                if (width, height) == (0, 0) {
                    if state.configured {
                        return;
                    }
                    (width, height) = (1280, 720);
                }
                let activated = states
                    .iter()
                    .find(|&st| *st as u32 == xdg_toplevel::State::Activated as u32)
                    .is_some();

                if activated {
                    state.activated = true;
                } else if state.activated && !activated {
                    let ev = Event::FocusLost;
                    for widget in state.widgets.iter_mut() {
                        widget.event(&ev);
                    }
                }

                if !state.configured || state.dimensions != (width, height) {
                    let surface = state
                        .bg_surface
                        .wl_surface
                        .as_ref()
                        .expect("bg surface was not ready");
                    let viewport = state
                        .bg_surface
                        .viewport
                        .as_ref()
                        .expect("bg viewportw as not ready");
                    viewport.set_destination(width, height);
                    surface.commit();

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
            wl_keyboard::Event::Leave { .. } => {
                state
                    .keyrepeat_sender
                    .send(RepeatMessage::StopRepeat)
                    .unwrap();
            }
            wl_keyboard::Event::Key {
                key,
                state: kbstate,
                ..
            } => {
                if key == 1 {
                    state.running = false;
                    return;
                }
                state.keyboard.resolve();
                let k = state.keyboard.key(key, kbstate);
                let repeats = k.repeats;
                let ev = Event::KeyEvent(k);
                for widget in state.widgets.iter_mut() {
                    widget.event(&ev);
                }
                if repeats {
                    if let Event::KeyEvent(k) = ev {
                        state
                            .keyrepeat_sender
                            .send(RepeatMessage::KeyEvent(k))
                            .unwrap();
                    }
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
            wl_keyboard::Event::RepeatInfo { rate, delay } => {
                state
                    .keyrepeat_sender
                    .send(RepeatMessage::RepeatInfo((rate as u32, delay as u32)))
                    .unwrap();
            }
            _ => (),
        }
    }
}

// TODO: Move to dedicated pointer module
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
            wl_pointer::Event::Enter {
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer = Some((surface_x, surface_y));
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer = Some((surface_x, surface_y));
            }
            wl_pointer::Event::Leave { .. } => state.pointer = None,
            wl_pointer::Event::Axis { axis, value, .. } => match state.pointer {
                Some((x, y)) => {
                    let axis = match axis {
                        WEnum::Value(wl_pointer::Axis::VerticalScroll) => {
                            PointerButton::ScrollVertical(value)
                        }
                        WEnum::Value(wl_pointer::Axis::HorizontalScroll) => {
                            PointerButton::ScrollHorizontal(value)
                        }
                        _ => return,
                    };
                    let pos = (x as u32, y as u32);
                    for widget in state.widgets.iter_mut() {
                        let geo = widget.geometry();
                        if geo.contains(pos) {
                            let pos = (pos.0 - geo.x, pos.1 - geo.y);
                            let ev = Event::PointerEvent(PointerEvent {
                                button: axis,
                                pos: pos,
                            });
                            widget.event(&ev);
                            state.dirty = true;
                            break;
                        }
                    }
                }
                _ => (),
            },
            wl_pointer::Event::Button {
                button,
                state: button_state,
                ..
            } => match (button, button_state, state.pointer) {
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
                            let ev = Event::PointerEvent(PointerEvent {
                                button: button,
                                pos: pos,
                            });
                            widget.event(&ev);
                            state.dirty = true;
                            break;
                        }
                    }
                }
                _ => (),
            },
            _ => (),
        }
    }
}

impl Dispatch<wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1,
        _: wp_single_pixel_buffer_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_viewporter::WpViewporter, ()> for State {
    fn event(
        _: &mut Self,
        _: &wp_viewporter::WpViewporter,
        _: wp_viewporter::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_viewport::WpViewport, ()> for State {
    fn event(
        _: &mut Self,
        _: &wp_viewport::WpViewport,
        _: wp_viewport::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
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
