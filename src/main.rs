mod buffer;
mod color;
mod config;
mod draw;
mod event;
mod fonts;
mod keyboard;
mod state;
mod utils;
mod widgets;

use std::fs::File;

use wayland_client::{Connection, QueueHandle, WaylandSource};

use calloop::{
    timer::{TimeoutAction, Timer},
    EventLoop,
};
use chrono::{Duration, Local, Timelike};

use buffer::BufferView;
use config::Config;
use event::{Event, Events};
use fonts::{FontMap, MaybeFontMap};
use keyboard::{KeyRepeatSource, RepeatMessage};
use state::State;
use widgets::{Geometry, Widget};

use std::{env, rc::Rc, thread};

fn main() {
    let mut args = env::args();
    // Skip program name
    _ = args.next();

    loop {
        match args.next() {
            Some(ref s) if s == "generate" => {
                let config = match args.next() {
                    Some(ref s) if s == "v1" => Config::generate_v1(),
                    Some(ref s) if s == "v2" => Config::generate_v2(false),
                    Some(ref s) if s == "clay" => Config::generate_v2(true),
                    None => Config::generate_v2(false),
                    Some(s) => panic!("unknown argument: {}", s),
                };
                let home = env::var_os("HOME").unwrap().into_string().unwrap();
                match File::create(format!("{}/.config/wldash/config.yml", home)) {
                    Ok(f) => serde_yaml::to_writer(f, &config).unwrap(),
                    Err(_) => panic!("uh"),
                }
                std::process::exit(0);
            }
            Some(_) => panic!("unknown argument"),
            None => break,
        }
    }

    let conn = Connection::connect_to_env().unwrap();

    let event_queue = conn.new_event_queue();
    let qhandle: QueueHandle<State> = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let (ping_sender, ping_source) = calloop::ping::make_ping().unwrap();
    let (keyrepeat_sender, keyrepeat_channel) = calloop::channel::channel::<RepeatMessage>();

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
            let events = state.events.lock().unwrap().flush();
            for widget in state.widgets.iter_mut() {
                for event in events.iter() {
                    widget.event(event);
                }
            }
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
            for widget in state.widgets.iter_mut() {
                widget.event(&Event::NewMinute);
            }
            state.dirty = true;

            // The timer event source requires us to return a TimeoutAction to
            // specify if the timer should be rescheduled. In our case we just drop it.
            TimeoutAction::ToDuration(d.to_std().unwrap())
        })
        .expect("Failed to insert event source!");

    let keyrepeat_source = KeyRepeatSource::new(keyrepeat_channel);
    handle
        .insert_source(keyrepeat_source, |event, _metadata, state| {
            let ev = Event::KeyEvent(event);
            for widget in state.widgets.iter_mut() {
                widget.event(&ev);
            }
            state.dirty = true;
        })
        .expect("Failed to insert keyrepeat source!");

    let events = Events::new(ping_sender);

    let mut fm = FontMap::new();

    let home = env::var_os("HOME").unwrap().into_string().unwrap();
    let config: Config = match File::open(format!("{}/.config/wldash/config.yml", home)) {
        Ok(f) => serde_yaml::from_reader(f).unwrap(),
        Err(_) => Default::default(),
    };

    if let Some(font_paths) = config.font_paths {
        for (key, value) in font_paths.into_iter() {
            fm.add_font_path(Box::leak(key.into_boxed_str()), value);
        }
    }

    let layout = Rc::new(config.widget.construct_layout(&mut 0));
    let mut widgets: Vec<Box<dyn Widget>> = Vec::new();
    config
        .widget
        .construct_widgets(&mut widgets, &mut fm, &events);

    let font_thread = thread::Builder::new()
        .name("fontloader".to_string())
        .spawn(move || {
            fm.load_fonts();
            fm
        })
        .unwrap();

    let mut state = State::new(
        config.mode,
        widgets,
        layout,
        MaybeFontMap::Waiting(font_thread),
        events,
        keyrepeat_sender,
    );

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

        let surface = state.main_surface.wl_surface.as_ref().unwrap().clone();
        if force {
            for (idx, widget) in state.widgets.iter_mut().enumerate() {
                if force || widget.get_dirty() {
                    let geo = widget.geometry();
                    let mut subview = bufview.subgeometry(geo);
                    damage[idx] = widget.draw(&mut state.fonts.unwrap().borrow_mut(), &mut subview);
                }
            }
            surface.damage_buffer(0, 0, 0x7FFFFFFF, 0x7FFFFFFF)
        } else {
            let mut drew = false;
            for (idx, widget) in state.widgets.iter_mut().enumerate() {
                if widget.get_dirty() {
                    drew = true;

                    let old_damage = damage[idx];
                    bufview.subgeometry(old_damage).clear();

                    let geo = widget.geometry();
                    let mut subview = bufview.subgeometry(geo);

                    let new_damage =
                        widget.draw(&mut state.fonts.unwrap().borrow_mut(), &mut subview);
                    let combined_damage = new_damage.expand(old_damage);
                    damage[idx] = new_damage;

                    surface.damage_buffer(
                        combined_damage.x as i32,
                        combined_damage.y as i32,
                        combined_damage.width as i32,
                        combined_damage.height as i32,
                    );
                }
            }
            if !drew {
                buf.release();
                continue;
            }
        }

        surface.attach(Some(&buf.buffer), 0, 0);
        surface.commit();
        conn.flush().unwrap();
        state.keyboard.realize();
    }
}
