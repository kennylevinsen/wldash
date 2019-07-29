#![feature(seek_convenience)]

extern crate byteorder;
extern crate chrono;
extern crate smithay_client_toolkit as sctk;

use std::collections::VecDeque;
use std::os::unix::io::AsRawFd;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use memmap::MmapMut;

use nix::poll::{poll, PollFd, PollFlags};

use chrono::{Date, DateTime, Datelike, Local, Timelike};

use rusttype::{point, Font, Scale};

use sctk::keyboard::{map_keyboard_auto_with_repeat, Event as KbEvent, KeyRepeatKind};
use sctk::reexports::client::protocol::wl_shm;
use sctk::reexports::client::{Display, EventQueue, NewProxy};
use sctk::utils::DoubleMemPool;
use sctk::window::{ConceptFrame, Event as WEvent, Window};
use sctk::Environment;

static DEJAVUSANS_MONO: &'static [u8] = include_bytes!("../fonts/dejavu/DejaVuSansMono.ttf");
static ROBOTO_REGULAR: &'static [u8] = include_bytes!("../fonts/Roboto-Regular.ttf");

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

fn draw_text(
    font_data: &'static [u8],
    mmap: &mut MmapMut,
    pos: (u32, u32),
    dimensions: (u32, u32),
    size: f32,
    color: (u8, u8, u8),
    s: &str,
) -> Result<(), ::std::io::Error> {
    // Load the font
    // This only succeeds if collection consists of one font
    let font = Font::from_bytes(font_data as &[u8]).expect("Error constructing Font");

    // The font size to use
    let scale = Scale::uniform(size);

    let v_metrics = font.v_metrics(scale);

    // layout the glyphs in a line with 20 pixels padding
    let glyphs: Vec<_> = font
        .layout(s, scale, point(20.0, 20.0 + v_metrics.ascent))
        .collect();

    // Loop through the glyphs in the text, positing each one on a line
    for glyph in glyphs {
        if let Some(bounding_box) = glyph.pixel_bounding_box() {
            // Draw the glyph into the image per-pixel by using the draw closure
            glyph.draw(|x, y, o| {
                let p = (((pos.1 + y + bounding_box.min.y as u32) * dimensions.0)
                    + (pos.0 + x + bounding_box.min.x as u32))
                    * 4;
                let a: u32 = 255 << 24;
                let r: u32 = (((color.0 as f32) * o) as u32) << 16;
                let g: u32 = (((color.1 as f32) * o) as u32) << 8;
                let b: u32 = ((color.2 as f32) * o) as u32;
                let v = a | r | g | b;
                unsafe {
                    let ptr = mmap.as_mut_ptr().offset(p as isize);
                    *(ptr as *mut u32) = v;
                }
            });
        }
    }

    Ok(())
}

fn draw_clock(
    mmap: &mut MmapMut,
    time: &DateTime<Local>,
    pos: (u32, u32),
    dimensions: (u32, u32),
) -> Result<(), ::std::io::Error> {
    draw_text(
        ROBOTO_REGULAR,
        mmap,
        pos,
        dimensions,
        64.0,
        (255, 255, 255),
        &format!(
            "{:?}, {:02}/{:02}/{:4}",
            time.weekday(),
            time.day(),
            time.month(),
            time.year()
        ),
    )?;

    draw_text(
        ROBOTO_REGULAR,
        mmap,
        (pos.0, pos.1 + 64),
        dimensions,
        256.0,
        (255, 255, 255),
        &format!("{:02}", time.hour()),
    )?;

    draw_text(
        ROBOTO_REGULAR,
        mmap,
        (pos.0 + 256, pos.1 + 64),
        dimensions,
        256.0,
        (255, 255, 255),
        ":",
    )?;

    draw_text(
        ROBOTO_REGULAR,
        mmap,
        (pos.0 + 320, pos.1 + 64),
        dimensions,
        256.0,
        (255, 255, 255),
        &format!("{:02}", time.minute()),
    )?;
    Ok(())
}

fn draw_month(
    mmap: &mut MmapMut,
    orig: &Date<Local>,
    time: &Date<Local>,
    pos: (u32, u32),
    dimensions: (u32, u32),
) -> Result<(), ::std::io::Error> {
    let mut time = time.clone();
    let mut y_off = 1;
    let mut done = false;

    let month_str = match time.month() {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => panic!("impossible value"),
    };

    draw_text(
        ROBOTO_REGULAR,
        mmap,
        pos,
        dimensions,
        68.0,
        (255, 255, 255),
        month_str,
    )?;

    while !done {
        let mut x_pos = 0;
        let mut wk = time.weekday();
        while wk != chrono::Weekday::Sun {
            x_pos += 1;
            wk = wk.pred();
        }

        while x_pos < 7 {
            let c = if time.day() == orig.day() && time.month() == orig.month() {
                (255, 255, 255)
            } else {
                (96, 96, 96)
            };
            draw_text(
                DEJAVUSANS_MONO,
                mmap,
                (pos.0 + (x_pos * 48), pos.1 + (y_off * 32) + 64),
                dimensions,
                32.0,
                c,
                &format!("{:02}", time.day()),
            )?;
            let t = time.with_day(time.day() + 1);
            if t.is_none() {
                done = true;
                break;
            }
            time = t.unwrap();
            x_pos += 1;
        }

        y_off += 1;
    }

    Ok(())
}

fn draw_calendar(
    mmap: &mut MmapMut,
    time: &Date<Local>,
    pos: (u32, u32),
    dimensions: (u32, u32),
) -> Result<(), ::std::io::Error> {
    // ~1546x384px
    let t = time.with_day(1).unwrap();
    draw_month(
        mmap,
        time,
        &t.pred().with_day(1).unwrap(),
        (pos.0, pos.1),
        dimensions,
    )?;
    draw_month(mmap, time, &t, (pos.0 + 512, pos.1), dimensions)?;
    let n = if t.month() == 12 {
        t.with_year(t.year() + 1).unwrap().with_month(1).unwrap()
    } else {
        t.with_month(t.month() + 1).unwrap()
    };
    draw_month(mmap, time, &n, (pos.0 + 1024, pos.1), dimensions)?;
    Ok(())
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
        {
            unsafe {
                let ptr = mmap.as_mut_ptr();
                for p in 0..(buf_x * buf_y) {
                    *((ptr as *mut u32).offset(p as isize)) = 0xC0000000;
                }
            }
        }

        let time = Local::now();

        draw_clock(mmap, &time, (0, 0), (buf_x, buf_y))?;
        draw_calendar(mmap, &time.date(), (0, 384), (buf_x, buf_y))?;

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
    let mut app = App::new((1408u32, 734u32));

    let mut last_time = Local::now();

    let mut tfd = timerfd::TimerFd::new_custom(timerfd::ClockId::Monotonic, true, true).unwrap();
    tfd.set_state(
        timerfd::TimerState::Periodic {
            current: Duration::from_millis(2000),
            interval: Duration::from_millis(2000),
        },
        timerfd::SetTimeFlags::Default,
    );

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

                let mut fds = [
                    PollFd::new(app.event_queue().get_connection_fd(), PollFlags::POLLIN),
                    PollFd::new(tfd.as_raw_fd(), PollFlags::POLLIN),
                ];
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
                    tfd.read();
                    q.lock().unwrap().push_back(Cmd::MaybeDraw);
                }
            }
        }
    }
}
