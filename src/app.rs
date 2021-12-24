use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use chrono::{Duration, Local, NaiveDateTime};

use crate::keyboard::{keysyms, map_keyboard, Event as KbEvent, KeyState, ModifiersState};

use wayland_client::protocol::{wl_compositor, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Display, EventQueue, GlobalEvent, GlobalManager, Main};
use wayland_protocols::wlr::unstable::layer_shell::v1::client::{
    zwlr_layer_shell_v1, zwlr_layer_surface_v1,
};

use crate::buffer::Buffer;
use crate::color::Color;
use crate::widget::{DrawContext, WaitContext, Widget};

use crate::cmd::Cmd;
use crate::doublemempool::DoubleMemPool;

#[derive(Debug)]
pub enum OutputMode {
    Active,
    All,
}

struct AppInner {
    compositor: Option<Main<wl_compositor::WlCompositor>>,
    surfaces: Vec<Main<wl_surface::WlSurface>>,
    shell_surfaces: Vec<Main<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>>,
    configured_surfaces: Arc<Mutex<usize>>,
    outputs: Vec<(u32, Main<wl_output::WlOutput>)>,
    shell: Option<Main<zwlr_layer_shell_v1::ZwlrLayerShellV1>>,
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
            output_mode,
            visible: true,
            scale,
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
        Main<wl_surface::WlSurface>,
        Main<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    ) {
        let surface = compositor.create_surface();

        let this_is_stupid = AtomicBool::new(false);

        let shell_surface = shell.get_layer_surface(
            &surface,
            output,
            zwlr_layer_shell_v1::Layer::Overlay,
            "".to_string(),
        );
        shell_surface.quick_assign(move |layer, event, _| match event {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                if this_is_stupid
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    *(configured_surfaces.lock().unwrap()) += 1;
                    layer.ack_configure(serial);
                    tx.send(Cmd::ForceDraw).unwrap();
                }
            }
            _ => unreachable!(),
        });

        shell_surface.set_keyboard_interactivity(1);
        shell_surface.set_size(1, 1);
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

        if !self.visible {
            self.surfaces = Vec::new();
            self.shell_surfaces = Vec::new();
            return;
        }

        match self.output_mode {
            OutputMode::Active => {
                if !self.shell_surfaces.is_empty() {
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
    }

    fn add_output(&mut self, id: u32, output: Main<wl_output::WlOutput>) {
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
                .map(|(x, y)| (*x, y.clone()))
                .collect();
            if output.1.as_ref().version() >= 3 {
                output.1.release()
            }
            self.outputs = new_outputs;
            self.outputs_changed();
        }
    }

    fn set_compositor(&mut self, compositor: Option<Main<wl_compositor::WlCompositor>>) {
        self.compositor = compositor
    }

    fn set_shell(&mut self, shell: Option<Main<zwlr_layer_shell_v1::ZwlrLayerShellV1>>) {
        self.shell = shell
    }
}

struct AppKeyboard {
    current: Option<Cmd>,
    delay: i32,
    rate: i32,
    next: Option<NaiveDateTime>,
}

pub struct App<'a> {
    pools: DoubleMemPool,
    display: Display,
    event_queue: EventQueue,
    cmd_queue: Arc<Mutex<VecDeque<Cmd>>>,
    widget: Option<Box<dyn Widget + Send + 'a>>,
    bg: Color,
    inner: Arc<Mutex<AppInner>>,
    last_damage: Option<Vec<(i32, i32, i32, i32)>>,
    last_dim: (u32, u32),
    keyboard: Arc<Mutex<AppKeyboard>>,
}

impl<'a> App<'a> {
    pub fn redraw(&mut self, force: bool) -> Result<(), ::std::io::Error> {
        let widget = match self.widget {
            Some(ref mut widget) => widget,
            None => return Ok(()),
        };

        let inner = self.inner.lock().unwrap();
        let time = Local::now().naive_local();

        if !inner.visible
            || inner.shell_surfaces.len() != *inner.configured_surfaces.lock().unwrap()
            || inner.surfaces.is_empty()
        {
            // Not ready yet
            return Ok(());
        }

        let (last, pool) = match self.pools.pool() {
            Some((last, pool)) => (last, pool),
            None => {
                self.pools.never_mind();
                return Ok(());
            }
        };

        let size = widget.size();
        let size_changed = self.last_dim != size;
        let force = force | size_changed;

        // resize the pool if relevant
        pool.resize((4 * size.0 * size.1) as usize)
            .expect("Failed to resize the memory pool.");
        let mmap = pool.mmap();
        let mut buf = Buffer::new(mmap, size);

        // Copy old damage
        let force = match (force, &self.last_damage) {
            (false, Some(d)) => {
                let lastmmap = last.mmap();
                let last = Buffer::new(lastmmap, size);

                if cfg!(feature = "damage_debug") {
                    buf.memset(&Color::new(0.5, 0.75, 0.75, 1.0));
                }
                for d in d {
                    last.copy_to(&mut buf, d.clone());
                }
                false
            }
            _ => true,
        };

        if force {
            buf.memset(&self.bg);
        }
        let report = widget.draw(
            &mut DrawContext {
                buf: &mut buf,
                bg: &self.bg,
                time,
                force,
            },
            (0, 0),
            size,
        )?;

        mmap.flush().unwrap();

        if !force && !report.full_damage && report.damage.is_empty() {
            // Nothing to do
            self.pools.never_mind();
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

    pub fn hide(&mut self) {
        let mut inner = self.inner.lock().unwrap();
        inner.visible = false;
        self.last_dim = (0, 0);
        inner.outputs_changed();
    }

    pub fn show(&mut self) {
        let mut inner = self.inner.lock().unwrap();
        inner.visible = true;
        self.last_dim = (0, 0);
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

    pub fn get_widget(&mut self) -> &mut Box<dyn Widget + Send + 'a> {
        self.widget.as_mut().unwrap()
    }

    pub fn set_widget(&mut self, w: Box<dyn Widget + Send + 'a>) -> Result<(), ::std::io::Error> {
        self.widget = Some(w);
        self.redraw(true)
    }

    pub fn set_keyboard_repeat(&mut self, ctx: &mut WaitContext) {
        let kbd = self.keyboard.lock().unwrap();
        if let Some(t) = kbd.next {
            ctx.set_time(t);
        }
    }

    pub fn key_repeat(&mut self) -> Option<Cmd> {
        let time = Local::now().naive_local();
        let mut kbd = self.keyboard.lock().unwrap();
        if let Some(target) = kbd.next {
            if time >= target {
                let cmd = kbd.current.as_ref().unwrap().clone();
                kbd.next = Some(time + Duration::milliseconds(kbd.rate.into()));
                return Some(cmd);
            }
        }
        None
    }

    pub fn new(tx: Sender<Cmd>, output_mode: OutputMode, bg: Color, scale: u32) -> App<'a> {
        let inner = Arc::new(Mutex::new(AppInner::new(tx, output_mode, scale)));

        //
        // Set up modules
        //

        let cmd_queue = Arc::new(Mutex::new(VecDeque::new()));

        let display = Display::connect_to_env().unwrap();
        let mut event_queue = display.create_event_queue();
        let display_wrapper = (*display).clone().attach(event_queue.token());

        //
        // Set up global manager
        //
        let inner_global = inner.clone();
        let manager =
            GlobalManager::new_with_cb(&display_wrapper, move |event, registry, _| match event {
                GlobalEvent::New {
                    id,
                    ref interface,
                    version,
                } => {
                    if let "wl_output" = &interface[..] {
                        let output = registry.bind(std::cmp::min(version, 3), id);
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
        event_queue.sync_roundtrip(&mut (), |_, _, _| {}).unwrap();
        event_queue.sync_roundtrip(&mut (), |_, _, _| {}).unwrap();

        // wl_compositor
        let compositor = manager
            .instantiate_range(1, 4)
            .expect("server didn't advertise `wl_compositor`");

        inner.lock().unwrap().set_compositor(Some(compositor));

        // wl_shm
        let shm: Main<wl_shm::WlShm> = manager
            .instantiate_range(1, 1)
            .expect("server didn't advertise `wl_shm`");

        let pools = DoubleMemPool::new(shm).expect("Failed to create a memory pool !");

        //
        // Get our seat
        //
        let seat: Main<wl_seat::WlSeat> = manager.instantiate_range(1, 6).unwrap();
        event_queue.sync_roundtrip(&mut (), |_, _, _| {}).unwrap();

        //
        // Keyboard processing
        //
        let kbd_clone = cmd_queue.clone();
        let modifiers_state = Arc::new(Mutex::new(ModifiersState {
            ctrl: false,
            alt: false,
            shift: false,
            caps_lock: false,
            logo: false,
            num_lock: false,
        }));

        let keyboard = Arc::new(Mutex::new(AppKeyboard {
            current: None,
            delay: 0,
            rate: 0,
            next: None,
        }));

        let kb2 = keyboard.clone();

        map_keyboard(&seat, None, move |event: KbEvent, _, _| match event {
            KbEvent::Key {
                keysym,
                utf8,
                state,
                ..
            } => {
                if let KeyState::Pressed = state {
                    match keysym {
                        keysyms::XKB_KEY_Escape => kbd_clone.lock().unwrap().push_back(Cmd::Exit),
                        keysyms::XKB_KEY_c if modifiers_state.lock().unwrap().ctrl => {
                            kbd_clone.lock().unwrap().push_back(Cmd::Exit)
                        }
                        v => {
                            let ev = Cmd::Keyboard {
                                key: v,
                                key_state: state,
                                modifiers_state: *modifiers_state.lock().unwrap(),
                                interpreted: utf8,
                            };
                            let mut kbd = kb2.lock().unwrap();
                            kbd.current = Some(ev.clone());
                            kbd.next = if kbd.delay > 0 {
                                Some(
                                    Local::now().naive_local()
                                        + Duration::milliseconds(kbd.delay.into()),
                                )
                            } else {
                                None
                            };
                            drop(kbd);
                            kbd_clone.lock().unwrap().push_back(ev);
                        }
                    }
                } else {
                    kb2.lock().unwrap().next = None;
                }
            }
            KbEvent::Leave { .. } => {
                kb2.lock().unwrap().next = None;
            }
            KbEvent::RepeatInfo { delay, rate } => {
                let mut kbd = kb2.lock().unwrap();
                kbd.delay = delay;
                kbd.rate = rate;
            }
            KbEvent::Modifiers { modifiers } => *modifiers_state.lock().unwrap() = modifiers,
            _ => (),
        })
        .expect("could not map keyboard");

        //
        // Prepare shell so that we can create our shell surface
        //
        inner
            .lock()
            .unwrap()
            .set_shell(Some(if let Ok(layer) = manager.instantiate_exact(1) {
                layer
            } else {
                panic!("server didn't advertise `zwlr_layer_shell_v1`");
            }));

        event_queue.sync_roundtrip(&mut (), |_, _, _| {}).unwrap();

        //
        // Cursor processing
        //
        let pointer_clone = cmd_queue.clone();
        let pointer = seat.get_pointer();
        let mut pos: (u32, u32) = (0, 0);
        let mut vert_scroll: f64 = 0.0;
        let mut horiz_scroll: f64 = 0.0;
        let mut btn: u32 = 0;
        let mut btn_clicked = false;
        pointer.quick_assign(move |_, event, _| match event {
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
            wl_pointer::Event::Button { button, state, .. } => {
                if let wl_pointer::ButtonState::Released = state {
                    btn = button;
                    btn_clicked = true;
                }
            }
            wl_pointer::Event::Frame => {
                if vert_scroll != 0.0 || horiz_scroll != 0.0 {
                    pointer_clone.lock().unwrap().push_back(Cmd::MouseScroll {
                        scroll: (horiz_scroll, vert_scroll),
                        pos,
                    });
                    vert_scroll = 0.0;
                    horiz_scroll = 0.0;
                }
                if btn_clicked {
                    pointer_clone
                        .lock()
                        .unwrap()
                        .push_back(Cmd::MouseClick { btn, pos });
                    btn_clicked = false;
                }
            }
            _ => {}
        });

        display.flush().unwrap();

        App {
            display,
            event_queue,
            cmd_queue,
            pools,
            widget: None,
            bg,
            inner,
            last_damage: None,
            last_dim: (0, 0),
            keyboard,
        }
    }
}
