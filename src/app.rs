use std::cmp::max;
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

use chrono::Local;

use smithay_client_toolkit::keyboard::{keysyms, map_keyboard_auto, Event as KbEvent, KeyState};
use smithay_client_toolkit::utils::DoubleMemPool;

use wayland_client::protocol::{wl_compositor, wl_pointer, wl_shm, wl_surface, wl_output};
use wayland_client::{Display, EventQueue, GlobalEvent, GlobalManager, NewProxy};
use wayland_protocols::wlr::unstable::layer_shell::v1::client::{
    zwlr_layer_shell_v1, zwlr_layer_surface_v1,
};

use crate::color::Color;
use crate::buffer::Buffer;

use crate::cmd::Cmd;

use crate::modules::backlight::Backlight;
use crate::modules::battery::UpowerBattery;
use crate::modules::calendar::Calendar;
use crate::modules::clock::Clock;
use crate::modules::launcher::Launcher;
use crate::modules::module::{Input, Module};
use crate::modules::sound::PulseAudio;

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
    dimensions: (u32, u32),
    draw_tx: Sender<Cmd>,
    output_mode: OutputMode,
    visible: bool,
}

impl AppInner {
    fn new(tx: Sender<Cmd>, output_mode: OutputMode) -> AppInner{
        AppInner{
            compositor: None,
            surfaces: Vec::new(),
            shell_surfaces: Vec::new(),
            configured_surfaces: Arc::new(Mutex::new(0)),
            outputs: Vec::new(),
            shell: None,
            dimensions: (0, 0),
            draw_tx: tx,
            output_mode: output_mode,
            visible: true,
        }
    }

    fn add_shell_surface(
        compositor: &wl_compositor::WlCompositor,
        shell: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        dimensions: (u32, u32),
        configured_surfaces: Arc<Mutex<usize>>,
        tx: Sender<Cmd>,
        output: Option<&wl_output::WlOutput>
    ) -> (
        wl_surface::WlSurface,
        zwlr_layer_surface_v1::ZwlrLayerSurfaceV1
    ) {
        let surface = compositor
            .create_surface(NewProxy::implement_dummy)
            .unwrap();

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
                                *(configured_surfaces.lock().unwrap()) += 1;
                                layer.ack_configure(serial);
                                tx.send(Cmd::ForceDraw).unwrap();
                            }
                            _ => unreachable!(),
                        },
                        (),
                    )
                },
            )
            .unwrap();

        shell_surface.set_keyboard_interactivity(1);
        shell_surface.set_size(dimensions.0, dimensions.1);

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
                    let (surface, shell_surface) = AppInner::add_shell_surface(&compositor, &shell, self.dimensions, self.configured_surfaces.clone(), self.draw_tx.clone(), None);
                    self.surfaces = vec![surface];
                    self.shell_surfaces = vec![shell_surface];

                },
                OutputMode::All => {
                    let mut surfaces = Vec::new();
                    let mut shell_surfaces = Vec::new();
                    for output in self.outputs.iter() {
                        let (surface, shell_surface) = AppInner::add_shell_surface(&compositor, &shell, self.dimensions, self.configured_surfaces.clone(), self.draw_tx.clone(), Some(&output.1));
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
            let new_outputs = self.outputs.iter().filter(|(output_id, _)| *output_id != id).map(|(x, y)| (x.clone(), y.clone())).collect();
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

    fn set_dimensions(&mut self, dimensions: (u32, u32)) {
        self.dimensions = dimensions
    }
}

pub struct App {
    pools: DoubleMemPool,
    display: Display,
    event_queue: EventQueue,
    cmd_queue: Arc<Mutex<VecDeque<Cmd>>>,
    modules: Vec<Module>,
    inner: Arc<Mutex<AppInner>>,
}

impl App {
    pub fn redraw(&mut self, force: bool) -> Result<(), ::std::io::Error> {
        let inner = self.inner.lock().unwrap();
        let time = Local::now();

        if inner.shell_surfaces.len() != *inner.configured_surfaces.lock().unwrap() {
            // Not ready yet
            return Ok(());
        }

        let pool = match self.pools.pool() {
            Some(pool) => pool,
            None => return Ok(()),
        };

        let (buf_x, buf_y) = inner.dimensions;

        let bg = Color::new(0.0, 0.0, 0.0, 0.9);

        // resize the pool if relevant

        pool.resize((4 * buf_x * buf_y) as usize)
            .expect("Failed to resize the memory pool.");

        let mmap = pool.mmap();
        let mut buf = Buffer::new(mmap, inner.dimensions);
        let mut damage = vec![];

        {
            let mut margin_buf = buf.subdimensions((20, 20, buf_x - 40, buf_y - 40))?;
            for module in self.modules.iter() {
                if module.update(&time, force)? {
                    let mut b = &mut margin_buf.subdimensions(module.get_bounds())?;
                    let mut d = module.draw(&mut b, &bg, &time)?;
                    damage.append(&mut d);
                }
            }
        }

        mmap.flush().unwrap();

        if damage.len() == 0 {
            // Nothing to do
            return Ok(());
        }

        // get a buffer and attach it
        let new_buffer = pool.buffer(
            0,
            buf_x as i32,
            buf_y as i32,
            4 * buf_x as i32,
            wl_shm::Format::Argb8888,
        );
        for surface in inner.surfaces.iter() {
            surface.attach(Some(&new_buffer), 0, 0);
            for d in damage.iter() {
                surface.damage(d.0, d.1, d.2, d.3);
            }
            surface.commit();
        }
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

    pub fn wipe(&mut self) {
        let inner = self.inner.lock().unwrap();
        let pool = match self.pools.pool() {
            Some(pool) => pool,
            None => return,
        };
        pool.resize((4 * inner.dimensions.0 * inner.dimensions.1) as usize)
            .expect("Failed to resize the memory pool.");
        let mmap = pool.mmap();
        let mut buf = Buffer::new(mmap, inner.dimensions);
        let bg = Color::new(0.0, 0.0, 0.0, 0.9);
        buf.memset(&bg);
    }

    pub fn with_modules<F>(&self, f: F)
        where F: Fn(&Module) {
        for m in self.modules.iter() {
            f(m);
        }
    }

    pub fn get_module(&self, pos: (u32, u32)) -> Option<&Module> {
        for m in self.modules.iter() {
            if m.intersect(pos) {
                return Some(&m);
            }
        }
        None
    }

    pub fn new(tx: Sender<Cmd>, output_mode: OutputMode) -> App {
        let inner = Arc::new(Mutex::new(AppInner::new(tx.clone(), output_mode)));

        //
        // Set up modules
        //
        let (mod_tx, mod_rx) = channel();
        std::thread::spawn(move || {
            let mut modules = vec![
                Module::new(Box::new(Clock::new(tx.clone())), (0, 0, 536, 320)),
                Module::new(Box::new(Calendar::new()), (0, 368, 1232, 344)),
            ];

            if let Ok(m) = Launcher::new(tx.clone()) {
                modules.push(Module::new(Box::new(m), (0, 728, 1232, 32)));
            }

            let mut vert_off = 0;
            if let Ok(m) = UpowerBattery::new(tx.clone()) {
                modules.push(Module::new(Box::new(m), (640, vert_off, 592, 32)));
                vert_off += 32;
            }
            if let Ok(m) = Backlight::new() {
                modules.push(Module::new(Box::new(m), (640, vert_off, 592, 32)));
                vert_off += 32;
            }
            if let Ok(m) = PulseAudio::new(tx.clone()) {
                modules.push(Module::new(Box::new(m), (640, vert_off, 592, 32)));
            }

            mod_tx.send(modules).unwrap();
        });

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
        let manager = GlobalManager::new_with_cb(&display_wrapper, move |event, registry| match event {
            GlobalEvent::New {
                id,
                ref interface,
                version,
            } => {
                if let "wl_output" = &interface[..] {
                    let output = registry.bind(version, id, move |output| {
                        output.implement_closure (move |_, _| {}, ())
                    }).unwrap();
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

        let pools = DoubleMemPool::new(&shm, || {}).expect("Failed to create a memory pool !");

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
                    v => kbd_clone.lock().unwrap().push_back(Cmd::KeyboardInput {
                        input: Input::Keypress {
                            key: v,
                            interpreted: utf8,
                        },
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
        inner.lock().unwrap().set_shell(Some(if let Ok(layer) = manager.instantiate_exact(
            1,
            |layer: NewProxy<zwlr_layer_shell_v1::ZwlrLayerShellV1>| {
                layer.implement_closure(|_, _| {}, ())
            },
        ) {
            layer
        } else {
            panic!("server didn't advertise `zwlr_layer_shell_v1`");
        }));

        //
        // Calculate window dimensions
        //
        let modules = mod_rx.recv().unwrap();

        let mut dimensions = (0, 0);
        for m in modules.iter() {
            let b = m.get_bounds();
            dimensions = (max(dimensions.0, b.0 + b.2), max(dimensions.1, b.1 + b.3));
        }

        // Add padding
        inner.lock().unwrap().set_dimensions((dimensions.0 + 40, dimensions.1 + 40));
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
                        pos = (surface_x as u32, surface_y as u32);
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
                        if pos.0 < 20 || pos.1 < 20 {
                            // Ignore stuff outside our margins
                            return;
                        }
                        let pos = (pos.0 - 20, pos.1 - 20);
                        if vert_scroll != 0.0 || horiz_scroll != 0.0 {
                            pointer_clone.lock().unwrap().push_back(Cmd::MouseInput {
                                pos: pos,
                                input: Input::Scroll {
                                    pos: pos,
                                    x: horiz_scroll,
                                    y: vert_scroll,
                                },
                            });
                            vert_scroll = 0.0;
                            horiz_scroll = 0.0;
                        }
                        if btn_clicked {
                            pointer_clone.lock().unwrap().push_back(Cmd::MouseInput {
                                pos: pos,
                                input: Input::Click {
                                    pos: pos,
                                    button: btn,
                                },
                            });
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
            modules: modules,
            inner: inner,
        }
    }
}
