#![feature(seek_convenience)]

extern crate byteorder;
extern crate chrono;
extern crate smithay_client_toolkit as sctk;

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::sync::{Arc, Mutex};

use nix::poll::{poll, PollFd, PollFlags};
use os_pipe::pipe;

use chrono::{Duration as ChronoDuration, Local, Timelike};

use sctk::keyboard::{map_keyboard_auto_with_repeat, Event as KbEvent, KeyRepeatKind};
use sctk::reexports::client::protocol::wl_shm;
use sctk::reexports::client::{Display, EventQueue, NewProxy};
use sctk::utils::DoubleMemPool;
use sctk::window::{ConceptFrame, Event as WEvent, Window};
use sctk::Environment;

mod color;
mod buffer;
mod draw;

use crate::color::Color;
use crate::buffer::Buffer;
use crate::draw::{draw_clock, draw_calendar};

#[derive(Debug)]
enum Cmd {
    Exit,
    Configure,
    MaybeDraw,
    Draw,
}

struct App {
    pools: DoubleMemPool,
    display: Display,
    event_queue: EventQueue,
    window: Window<ConceptFrame>,
    cmd_queue: Arc<Mutex<VecDeque<Cmd>>>,
    dimensions: (u32, u32),
}

impl App {
    fn redraw(&mut self) -> Result<(), ::std::io::Error> {
        let pool = match self.pools.pool() {
            Some(pool) => pool,
            None => return Ok(()),
        };

        let (buf_x, buf_y) = self.dimensions;

        // resize the pool if relevant

        pool.resize((4 * buf_x * buf_y) as usize)
            .expect("Failed to resize the memory pool.");

        let mmap = pool.mmap();
        let mut buf = Buffer::new(mmap, self.dimensions);
        let bg = Color::new(0.0, 0.0, 0.0, 0.8);
        buf.memset(&bg);

        let time = Local::now();

        draw_clock(&mut buf.subdimensions((0, 0, 720, 320)), &bg, &time)?;
        draw_calendar(
            &mut buf.subdimensions((0, 384, 1472, 384)),
            &bg,
            &time.date(),
        )?;

        mmap.flush().unwrap();

        // get a buffer and attach it
        let new_buffer = pool.buffer(
            0,
            buf_x as i32,
            buf_y as i32,
            4 * buf_x as i32,
            wl_shm::Format::Argb8888,
        );
        self.window.surface().attach(Some(&new_buffer), 0, 0);
        self.window.surface().commit();
        Ok(())
    }

    fn window(&mut self) -> &mut Window<ConceptFrame> {
        &mut self.window
    }

    fn cmd_queue(&self) -> Arc<Mutex<VecDeque<Cmd>>> {
        self.cmd_queue.clone()
    }

    fn flush_display(&mut self) {
        self.display.flush().expect("unable to flush display");
    }

    fn dimensions(&self) -> (u32, u32) {
        self.dimensions
    }

    fn event_queue(&mut self) -> &mut EventQueue {
        &mut self.event_queue
    }

    fn new(dimensions: (u32, u32)) -> App {
        let cmd_queue = Arc::new(Mutex::new(VecDeque::new()));

        let (display, mut event_queue) = Display::connect_to_env().unwrap();
        let env = Environment::from_display(&*display, &mut event_queue).unwrap();

        let pools = DoubleMemPool::new(&env.shm, || {}).expect("Failed to create a memory pool !");

        let surface = env
            .compositor
            .create_surface(NewProxy::implement_dummy)
            .unwrap();

        let event_clone = cmd_queue.clone();
        let mut window =
            Window::<ConceptFrame>::init_from_env(&env, surface, dimensions.clone(), move |evt| {
                match evt {
                    WEvent::Close => return,
                    WEvent::Refresh => {
                        event_clone.lock().unwrap().push_back(Cmd::MaybeDraw);
                    }
                    WEvent::Configure {
                        new_size: _,
                        states: _,
                    } => {
                        event_clone.lock().unwrap().push_back(Cmd::Configure);
                    }
                }
            })
            .expect("Failed to create a window !");

        let seat = env
            .manager
            .instantiate_range(1, 6, NewProxy::implement_dummy)
            .unwrap();

        window.new_seat(&seat);

        let kbd_clone = cmd_queue.clone();
        map_keyboard_auto_with_repeat(
            &seat,
            KeyRepeatKind::System,
            move |event: KbEvent, _| match event {
                KbEvent::Key { keysym, .. } => match keysym {
                    0xFF1B => kbd_clone.lock().unwrap().push_back(Cmd::Exit),
                    _ => (),
                },
                _ => (),
            },
            |_, _| {},
        )
        .expect("Failed to map keyboard");

        window.set_title("dashboard".to_string());
        window.set_app_id("dashboard".to_string());

        App {
            window: window,
            display: display,
            event_queue: event_queue,
            cmd_queue: cmd_queue,
            pools: pools,
            dimensions: dimensions,
        }
    }
}

fn main() {
    let (mut rx_pipe, mut tx_pipe) = pipe().unwrap();

    let mut app = App::new((1600u32, 784u32));

    let mut last_time = Local::now();

    let worker_queue = app.cmd_queue();
    std::thread::spawn(move || loop {
        let n = Local::now();
        let target = (n + ChronoDuration::seconds(60))
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();

        let d = target - n;

        std::thread::sleep(d.to_std().unwrap());
        worker_queue.lock().unwrap().push_back(Cmd::MaybeDraw);
        tx_pipe.write_all(&[0x1]).unwrap();
    });

    let mut fds = [
        PollFd::new(app.event_queue().get_connection_fd(), PollFlags::POLLIN),
        PollFd::new(rx_pipe.as_raw_fd(), PollFlags::POLLIN),
    ];

    let q = app.cmd_queue();
    loop {
        let cmd = q.lock().unwrap().pop_front();
        match cmd {
            Some(cmd) => match cmd {
                Cmd::Configure => {
                    let d = app.dimensions();
                    app.window().resize(d.0, d.1);
                    q.lock().unwrap().push_back(Cmd::Draw);
                }
                Cmd::Draw => {
                    app.redraw().expect("Failed to draw");
                    app.flush_display();
                    last_time = Local::now();
                }
                Cmd::MaybeDraw => {
                    let t = Local::now();
                    if t.hour() != last_time.hour() || t.minute() != last_time.minute() {
                        q.lock().unwrap().push_back(Cmd::Draw);
                    }
                }
                Cmd::Exit => {
                    std::process::exit(0);
                }
            },
            None => {
                app.flush_display();

                poll(&mut fds, -1).unwrap();

                if fds[0].revents().unwrap().contains(PollFlags::POLLIN) {
                    if let Some(guard) = app.event_queue().prepare_read() {
                        if let Err(e) = guard.read_events() {
                            if e.kind() != ::std::io::ErrorKind::WouldBlock {
                                eprintln!(
                                    "Error while trying to read from the wayland socket: {:?}",
                                    e
                                );
                            }
                        }
                    }

                    app.event_queue()
                        .dispatch_pending()
                        .expect("Failed to dispatch all messages.");
                }

                if fds[1].revents().unwrap().contains(PollFlags::POLLIN) {
                    let mut v: Vec<u8> = vec![0x00];
                    rx_pipe.read_exact(&mut v).unwrap();
                }
            }
        }
    }
}
