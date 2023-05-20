use std::{
    cmp::max,
    default::Default,
    env,
    rc::Rc,
    sync::{Arc, Mutex},
};

use wayland_client::{
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface,
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
    wl_shm: Option<wl_shm::WlShm>,
    xdg_activation: Option<xdg_activation_v1::XdgActivationV1>,
    viewporter: Option<wp_viewporter::WpViewporter>,
}

pub struct State {
    mode: OperationMode,
    pub main_surface: MainSurface,
    pub bg_surface: BackgroundSurface,
    pub protocols: Protocols,
    pub running: bool,
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
            widgets,
            fonts,
            events,
            layout,
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
                    let main_surface = compositor.create_surface(qh, ());
                    let bg_surface = compositor.create_surface(qh, ());
                    state.main_surface.wl_surface = Some(main_surface);
                    state.bg_surface.wl_surface = Some(bg_surface);
                    state.activate(qh);
                }
                "wl_subcompositor" => {
                    let subcompositor =
                        registry.bind::<wl_subcompositor::WlSubcompositor, _, _>(name, 1, qh, ());

                    match (&state.main_surface.wl_surface, &state.bg_surface.wl_surface) {
                        (Some(parent), Some(child)) => {
                            let subsurface = subcompositor.get_subsurface(child, parent, qh, ());
                            subsurface.place_below(parent);
                            state.bg_surface.subsurface = Some(subsurface);
                        }
                        _ => todo!("handle early subcompositor creation"),
                    }
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
                        let wm_base =
                            registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 2, qh, ());

                        match &state.main_surface.wl_surface {
                            Some(surface) => {
                                let xdg_surface = wm_base.get_xdg_surface(surface, qh, ());
                                let toplevel = xdg_surface.get_toplevel(qh, ());
                                toplevel.set_title("wldash".into());

                                surface.commit();

                                state.main_surface.xdg_surface = Some((xdg_surface, toplevel));
                            }
                            _ => todo!("handle early xdg_shell creation"),
                        }
                    }
                }
                "xdg_activation_v1" => {
                    state.protocols.xdg_activation = Some(
                        registry.bind::<xdg_activation_v1::XdgActivationV1, _, _>(name, 1, qh, ()),
                    );
                    state.activate(qh);
                }
                "zwlr_layer_shell_v1" => {
                    if let OperationMode::LayerSurface(size) = state.mode {
                        let layer_shell = registry
                            .bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(name, 4, qh, ());
                        state.fonts.resolve();
                        let fonts = state.fonts.unwrap();
                        let layout = state.layout.clone();
                        let min_size = layout.minimum_size(&mut fonts.borrow_mut(), state);
                        match &state.main_surface.wl_surface {
                            Some(surface) => {
                                let layer_surface = layer_shell.get_layer_surface(
                                    surface,
                                    None,
                                    zwlr_layer_shell_v1::Layer::Top,
                                    "launcher".to_string(),
                                    qh,
                                    (),
                                );
                                layer_surface.set_keyboard_interactivity(
                                    zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive,
                                );
                                layer_surface.set_size(
                                    max(size.0, min_size.width),
                                    max(size.1, min_size.height),
                                );

                                surface.commit();
                                state.main_surface.layer_surface = Some(layer_surface);
                            }
                            _ => todo!("handle early layer_shell creation"),
                        }
                    }
                }
                "wp_viewporter" => {
                    let viewporter =
                        registry.bind::<wp_viewporter::WpViewporter, _, _>(name, 1, qh, ());
                    match &state.bg_surface.wl_surface {
                        Some(surface) => {
                            state.bg_surface.viewport =
                                Some(viewporter.get_viewport(surface, qh, ()));
                        }
                        _ => todo!("handle early viewporter creation"),
                    }
                    state.protocols.viewporter = Some(viewporter);
                }
                "wp_single_pixel_buffer_manager_v1" => {
                    let singlepixel = registry.bind::<wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1, _, _>(name, 1, qh, ());
                    match &state.bg_surface.wl_surface {
                        Some(surface) => {
                            let buffer =
                                singlepixel.create_u32_rgba_buffer(0, 0, 0, 0xFAFAFAFA, qh, ());
                            surface.attach(Some(&buffer), 0, 0);
                            surface.damage_buffer(0, 0, 1, 1);
                        }
                        _ => todo!("handle early single_pixel_buffer creation"),
                    }
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
        if let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            if (width, height) == (0, 0) {
                return;
            }
            layer_surface.ack_configure(serial);
            state.configured = true;
            state.dirty = true;

            let dim = (width as i32, height as i32);

            if state.dimensions == dim {
                return;
            }
            state.bufmgr.clear_buffers();
            state.dimensions = dim;
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

            match (&state.bg_surface.wl_surface, &state.bg_surface.viewport) {
                (Some(surface), Some(viewport)) => {
                    viewport.set_destination(dim.0, dim.1);
                    surface.commit();
                }
                _ => todo!("handle early layer shell creation"),
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
                    let ev = Event::FocusLost;
                    for widget in state.widgets.iter_mut() {
                        widget.event(&ev);
                    }
                }

                if state.dimensions != (width, height) {
                    match (&state.bg_surface.wl_surface, &state.bg_surface.viewport) {
                        (Some(surface), Some(viewport)) => {
                            viewport.set_destination(width, height);
                            surface.commit();
                        }
                        _ => todo!("handle early xdg creation"),
                    }

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
