use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use chrono::Local;

use smithay_client_toolkit::keyboard::{
    keysyms, map_keyboard_auto, Event as KbEvent, KeyState, ModifiersState,
};

use wayland_client::protocol::{wl_compositor, wl_output, wl_pointer, wl_shm, wl_surface};
use wayland_client::{Display, EventQueue, GlobalEvent, GlobalManager, NewProxy};
use wayland_protocols::wlr::unstable::layer_shell::v1::client::{
    zwlr_layer_shell_v1, zwlr_layer_surface_v1,
};

use crate::buffer::Buffer;
use crate::color::Color;
use crate::widget::{DrawContext, Widget};

use crate::cmd::Cmd;
use crate::doublemempool::DoubleMemPool;

pub enum OutputMode {
    Active,
    All,
}

struct AppInner {
    compositor: Option<wl_compositor::WlCompositor>,
    surfaces: Vec<wl_surface::WlSurface>,
    shell_surfaces: Vec<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    configured_surfaces: Arc<Mutex<usize>>,
    outputs: Vec<(u32, wl_output::WlOutput)>,
    shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    draw_tx: Sender<Cmd>,
    output_mode: OutputMode,
    visible: bool,
    scale: u32,
}

impl AppInner {
    fn new(tx: Sender<Cmd>, output_mode: OutputMode, scale: u32) -> AppInner {
        AppInner {
            compositor: None,
            surfaces: Vec::new(),
            shell_surfaces: Vec::new(),
            configured_surfaces: Arc::new(Mutex::new(0)),
            outputs: Vec::new(),
            shell: None,
            draw_tx: tx,
            output_mode: output_mode,
            visible: true,
            scale: scale,
        }
    }

    fn add_shell_surface(
        compositor: &wl_compositor::WlCompositor,
        shell: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        scale: u32,
        configured_surfaces: Arc<Mutex<usize>>,
        tx: Sender<Cmd>,
        output: Option<&wl_output::WlOutput>,
    ) -> (
        wl_surface::WlSurface,
        zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    ) {
        let surface = compositor
            .create_surface(NewProxy::implement_dummy)
            .unwrap();

        let this_is_stupid = Arc::new(Mutex::new(false));

        let shell_surface = shell
            .get_layer_surface(
                &surface,
                output,
                zwlr_layer_shell_v1::Layer::Overlay,
                "".to_string(),
                move |layer| {
                    layer.implement_closure(
                        move |evt, layer| match evt {
                            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                                let mut x = this_is_stupid.lock().unwrap();
                                if !*x {
                                    *x = true;
                                    *(configured_surfaces.lock().unwrap()) += 1;
                                    layer.ack_configure(serial);
                                    tx.send(Cmd::ForceDraw).unwrap();
                                }
                            }
                            _ => unreachable!(),
                        },
                        (),
                    )
                },
            )
            .unwrap();

        shell_surface.set_keyboard_interactivity(1);
        surface.set_buffer_scale(scale as i32);
        surface.commit();
        (surface, shell_surface)
    }

    fn outputs_changed(&mut self) {
        let shell = match self.shell {
            Some(ref shell) => shell.to_owned(),
            None => return,
        };
        let compositor = match self.compositor {
            Some(ref c) => c.to_owned(),
            None => return,
        };

        for shell_surface in self.shell_surfaces.iter() {
            shell_surface.destroy();
        }
        for surface in self.surfaces.iter() {
            surface.destroy();
        }

        self.configured_surfaces = Arc::new(Mutex::new(0));

        if self.visible {
            match self.output_mode {
                OutputMode::Active => {
                    if self.shell_surfaces.len() > 0 {
                        return;
                    }
                    let (surface, shell_surface) = AppInner::add_shell_surface(
                        &compositor,
                        &shell,
                        self.scale,
                        self.configured_surfaces.clone(),
                        self.draw_tx.clone(),
                        None,
                    );
                    self.surfaces = vec![surface];
                    self.shell_surfaces = vec![shell_surface];
                }
                OutputMode::All => {
                    let mut surfaces = Vec::new();
                    let mut shell_surfaces = Vec::new();
                    for output in self.outputs.iter() {
                        let (surface, shell_surface) = AppInner::add_shell_surface(
                            &compositor,
                            &shell,
                            self.scale,
                            self.configured_surfaces.clone(),
                            self.draw_tx.clone(),
                            Some(&output.1),
                        );
                        surfaces.push(surface);
                        shell_surfaces.push(shell_surface);
                    }
                    self.surfaces = surfaces;
                    self.shell_surfaces = shell_surfaces;
                }
            }
            self.draw_tx.send(Cmd::ForceDraw).unwrap();
        } else {
            self.surfaces = Vec::new();
            self.shell_surfaces = Vec::new();
        }
    }

    fn add_output(&mut self, id: u32, output: wl_output::WlOutput) {
        self.outputs.push((id, output));
        self.outputs_changed();
    }

    fn remove_output(&mut self, id: u32) {
        let old_output = self.outputs.iter().find(|(output_id, _)| *output_id == id);
        if let Some(output) = old_output {
            let new_outputs = self
                .outputs
                .iter()
                .filter(|(output_id, _)| *output_id != id)
                .map(|(x, y)| (x.clone(), y.clone()))
                .collect();
            if output.1.as_ref().version() >= 3 {
                output.1.release()
            }
            self.outputs = new_outputs;
            self.outputs_changed();
        }
    }

    fn set_compositor(&mut self, compositor: Option<wl_compositor::WlCompositor>) {
        self.compositor = compositor
    }

    fn set_shell(&mut self, shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>) {
        self.shell = shell
    }
}

pub struct App {
    pools: DoubleMemPool,
    display: Display,
    event_queue: EventQueue,
    cmd_queue: Arc<Mutex<VecDeque<Cmd>>>,
    widget: Option<Box<dyn Widget + Send>>,
    bg: Color,
    inner: Arc<Mutex<AppInner>>,
    last_damage: Option<Vec<(i32, i32, i32, i32)>>,
    last_dim: (u32, u32),
}

impl App {
    pub fn redraw(&mut self, mut force: bool) -> Result<(), ::std::io::Error> {
        let widget = match self.widget {
            Some(ref mut widget) => widget,
            None => return Ok(()),
        };

        let inner = self.inner.lock().unwrap();
        let time = Local::now();

        if inner.shell_surfaces.len() != *inner.configured_surfaces.lock().unwrap() {
            // Not ready yet
            return Ok(());
        }

        let (last, pool) = match self.pools.pool() {
            Some((last, pool)) => (last, pool),
            None => return Ok(()),
        };

        let size = widget.size();
        let size_changed = self.last_dim != size;

        // resize the pool if relevant
        pool.resize((4 * size.0 * size.1) as usize)
            .expect("Failed to resize the memory pool.");
        let mmap = pool.mmap();
        let mut buf = Buffer::new(mmap, size);

        // Copy old damage
        if let Some(d) = &self.last_damage {
            if !size_changed {
                let lastmmap = last.mmap();
                let last = Buffer::new(lastmmap, size);

                if cfg!(feature = "damage_debug") {
                    buf.memset(&Color::new(0.5, 0.75, 0.75, 1.0));
                }
                for d in d {
                    last.copy_to(&mut buf, d.clone());
                }
            } else {
                force = true;
            }
        } else {
            force = true;
        }

        if force {
            buf.memset(&self.bg);
        }
        let report = widget.draw(
            &mut DrawContext {
                buf: &mut buf,
                bg: &self.bg,
                time: &time,
                force,
            },
            (0, 0),
        )?;

        mmap.flush().unwrap();

        if !size_changed && !report.full_damage && report.damage.len() == 0 {
            // Nothing to do
            return Ok(());
        }

        // get a buffer and attach it
        let new_buffer = pool.buffer(
            0,
            report.width as i32,
            report.height as i32,
            4 * size.0 as i32,
            wl_shm::Format::Argb8888,
        );
        if size_changed {
            for shell_surface in inner.shell_surfaces.iter() {
                shell_surface.set_size(size.0 / inner.scale, size.1 / inner.scale);
            }
        }
        for surface in inner.surfaces.iter() {
            surface.attach(Some(&new_buffer), 0, 0);
            if cfg!(feature = "damage_debug") || force || report.full_damage {
                surface.damage_buffer(0, 0, size.0 as i32, size.1 as i32);
            } else {
                for d in report.damage.iter() {
                    surface.damage_buffer(d.0, d.1, d.2, d.3);
                }
            }
            surface.commit();
        }
        self.last_damage = if force || report.full_damage {
            Some(vec![(0, 0, size.0 as i32, size.1 as i32)])
        } else {
            Some(report.damage)
        };
        self.last_dim = size;
        Ok(())
    }

    pub fn toggle_visible(&mut self) {
        let mut inner = self.inner.lock().unwrap();
        inner.visible = !inner.visible;
        inner.outputs_changed();
    }

    pub fn cmd_queue(&self) -> Arc<Mutex<VecDeque<Cmd>>> {
        self.cmd_queue.clone()
    }

    pub fn flush_display(&mut self) {
        self.display.flush().expect("unable to flush display");
    }

    pub fn event_queue(&mut self) -> &mut EventQueue {
        &mut self.event_queue
    }

    pub fn get_widget(&mut self) -> &mut Box<dyn Widget + Send> {
        self.widget.as_mut().unwrap()
    }

    pub fn set_widget(&mut self, w: Box<dyn Widget + Send>) -> Result<(), ::std::io::Error> {
        self.widget = Some(w);
        self.redraw(true)
    }

    pub fn new(tx: Sender<Cmd>, output_mode: OutputMode, bg: Color, scale: u32) -> App {
        let inner = Arc::new(Mutex::new(AppInner::new(tx.clone(), output_mode, scale)));

        //
        // Set up modules
        //

        let cmd_queue = Arc::new(Mutex::new(VecDeque::new()));

        let (display, mut event_queue) = Display::connect_to_env().unwrap();

        let display_wrapper = display
            .as_ref()
            .make_wrapper(&event_queue.get_token())
            .unwrap()
            .into();

        //
        // Set up global manager
        //
        let inner_global = inner.clone();
        let manager =
            GlobalManager::new_with_cb(&display_wrapper, move |event, registry| match event {
                GlobalEvent::New {
                    id,
                    ref interface,
                    version,
                } => {
                    if let "wl_output" = &interface[..] {
                        let output = registry
                            .bind(version, id, move |output| {
                                output.implement_closure(move |_, _| {}, ())
                            })
                            .unwrap();
                        inner_global.lock().unwrap().add_output(id, output);
                    }
                }
                GlobalEvent::Removed { id, ref interface } => {
                    if let "wl_output" = &interface[..] {
                        inner_global.lock().unwrap().remove_output(id);
                    }
                }
            });

        // double sync to retrieve the global list
        // and the globals metadata
        event_queue.sync_roundtrip().unwrap();
        event_queue.sync_roundtrip().unwrap();

        // wl_compositor
        let compositor: wl_compositor::WlCompositor = manager
            .instantiate_range(1, 4, NewProxy::implement_dummy)
            .expect("server didn't advertise `wl_compositor`");

        inner.lock().unwrap().set_compositor(Some(compositor));

        // wl_shm
        let shm_formats = Arc::new(Mutex::new(Vec::new()));
        let shm_formats2 = shm_formats.clone();
        let shm = manager
            .instantiate_range(1, 1, |shm| {
                shm.implement_closure(
                    move |evt, _| {
                        if let wl_shm::Event::Format { format } = evt {
                            shm_formats2.lock().unwrap().push(format);
                        }
                    },
                    (),
                )
            })
            .expect("server didn't advertise `wl_shm`");

        let pools = DoubleMemPool::new(&shm).expect("Failed to create a memory pool !");

        //
        // Get our seat
        //
        let seat = manager
            .instantiate_range(1, 6, NewProxy::implement_dummy)
            .unwrap();

        //
        // Keyboard processing
        //
        let kbd_clone = cmd_queue.clone();
        map_keyboard_auto(&seat, move |event: KbEvent, _| match event {
            KbEvent::Key {
                keysym,
                utf8,
                state,
                ..
            } => match state {
                KeyState::Pressed => match keysym {
                    keysyms::XKB_KEY_Escape => kbd_clone.lock().unwrap().push_back(Cmd::Exit),
                    v => kbd_clone.lock().unwrap().push_back(Cmd::Keyboard {
                        key: v,
                        key_state: state,
                        modifiers_state: ModifiersState {
                            ctrl: false,
                            alt: false,
                            shift: false,
                            caps_lock: false,
                            logo: false,
                            num_lock: false,
                        },
                        interpreted: utf8,
                    }),
                },
                _ => (),
            },
            _ => (),
        })
        .expect("Failed to map keyboard");

        //
        // Prepare shell so that we can create our shell surface
        //
        inner.lock().unwrap().set_shell(Some(
            if let Ok(layer) = manager.instantiate_exact(
                1,
                |layer: NewProxy<zwlr_layer_shell_v1::ZwlrLayerShellV1>| {
                    layer.implement_closure(|_, _| {}, ())
                },
            ) {
                layer
            } else {
                panic!("server didn't advertise `zwlr_layer_shell_v1`");
            },
        ));

        inner.lock().unwrap().outputs_changed();
        event_queue.sync_roundtrip().unwrap();

        //
        // Cursor processing
        //
        let pointer_clone = cmd_queue.clone();
        seat.get_pointer(move |ptr| {
            let mut pos: (u32, u32) = (0, 0);
            let mut vert_scroll: f64 = 0.0;
            let mut horiz_scroll: f64 = 0.0;
            let mut btn: u32 = 0;
            let mut btn_clicked = false;
            ptr.implement_closure(
                move |evt, _| match evt {
                    wl_pointer::Event::Enter {
                        surface_x,
                        surface_y,
                        ..
                    } => {
                        pos = (surface_x as u32, surface_y as u32);
                    }
                    wl_pointer::Event::Leave { .. } => {
                        pos = (0, 0);
                    }
                    wl_pointer::Event::Motion {
                        surface_x,
                        surface_y,
                        ..
                    } => {
                        pos = (surface_x as u32 * scale, surface_y as u32 * scale);
                    }
                    wl_pointer::Event::Axis { axis, value, .. } => {
                        if axis == wl_pointer::Axis::VerticalScroll {
                            vert_scroll += value;
                        }
                    }
                    wl_pointer::Event::Button { button, state, .. } => match state {
                        wl_pointer::ButtonState::Released => {
                            btn = button;
                            btn_clicked = true;
                        }
                        _ => {}
                    },
                    wl_pointer::Event::Frame => {
                        if vert_scroll != 0.0 || horiz_scroll != 0.0 {
                            pointer_clone.lock().unwrap().push_back(Cmd::MouseScroll {
                                scroll: (horiz_scroll, vert_scroll),
                                pos: pos,
                            });
                            vert_scroll = 0.0;
                            horiz_scroll = 0.0;
                        }
                        if btn_clicked {
                            pointer_clone
                                .lock()
                                .unwrap()
                                .push_back(Cmd::MouseClick { btn: btn, pos: pos });
                            btn_clicked = false;
                        }
                    }
                    _ => {}
                },
                (),
            )
        })
        .unwrap();

        display.flush().unwrap();

        App {
            display: display,
            event_queue: event_queue,
            cmd_queue: cmd_queue,
            pools: pools,
            widget: None,
            bg: bg,
            inner: inner,
            last_damage: None,
            last_dim: (0, 0),
        }
    }
}
