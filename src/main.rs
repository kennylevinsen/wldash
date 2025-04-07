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
use utils::{
    desktop::{load_desktop_files, write_desktop_cache},
    xdg,
};
use widgets::{Geometry, Widget};

use std::{
    env,
    fs::File,
    fs,
    path::Path,
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    rc::Rc,
    thread,
};

fn print_usage() {
    println!(
        "
usage: wldash [OPTIONS]

OPTIONS:
  --config CONFIG_FILE                     use the specified config file
                                           (~/.config/wldash/config.yml by default)
  --write-default-config [v1|v2|toplevel]  generate and write a new config file
                                           (v2 by default)
  --desktop-refresh                        refresh desktop file cache
"
    );
}

fn ensure_dir_exists(path: &str) {
    match fs::create_dir_all(path) {
        Ok(_) => (),
        Err(err) => panic!("error creating directory: {}", err),
    }
}

fn create_required_directories() {
    let dir_config_wldash = format!("{}/wldash", xdg::config_folder());
    let dir_cache_wldash = format!("{}/wldash", xdg::cache_folder());
    ensure_dir_exists(&dir_config_wldash);
    ensure_dir_exists(&dir_cache_wldash);
}

fn main() {
    let mut args = env::args();
    // Skip program name
    _ = args.next();

    create_required_directories();

    let mut config_file = format!("{}/wldash/config.yml", xdg::config_folder());

    loop {
        match args.next() {
            Some(ref s) if s == "--config" => match args.next() {
                Some(ref s) => config_file = s.clone(),
                None => panic!("missing argument to --config"),
            },
            Some(ref s) if s == "--write-default-config" => {
                let config = match args.next() {
                    Some(ref s) if s == "v1" => Config::generate_v1(),
                    Some(ref s) if s == "v2" => Config::generate_v2(false),
                    Some(ref s) if s == "toplevel" => Config::generate_v2(true),
                    None => Config::generate_v2(false),
                    Some(s) => panic!("unknown argument: {}", s),
                };
                match File::create(config_file) {
                    Ok(f) => serde_yaml::to_writer(f, &config).unwrap(),
                    Err(_) => panic!("uh"),
                }
                std::process::exit(0);
            }
            Some(ref s) if s == "--desktop-refresh" => {
                let v = load_desktop_files();
                write_desktop_cache(&v).unwrap();
                std::process::exit(0);
            }
            Some(ref s) if s == "--help" => {
                print_usage();
                std::process::exit(0);
            }
            Some(_) => {
                print_usage();
                std::process::exit(1);
            }
            None => break,
        }
    }

    let conn = Connection::connect_to_env().unwrap();

    let event_queue = conn.new_event_queue();
    let qhandle: QueueHandle<State> = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());
    display.sync(&qhandle, ());

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

    let socket_path = match env::var("XDG_RUNTIME_DIR") {
        Ok(dir) => dir + "/wldash",
        Err(_) => "/tmp/wldash".to_string(),
    };

    let events = Events::new(ping_sender);

    let mut fm = FontMap::new();

    let config: Config = match File::open(config_file) {
        Ok(f) => serde_yaml::from_reader(f).unwrap(),
        Err(_) => panic!("configuration file missing: try 'wldash --write-default-config'"),
    };

    if let Some(true) = config.server {
        if let Ok(mut socket) = UnixStream::connect(socket_path.clone()) {
            socket.write_all(b"kill\n").unwrap();
            return;
        };

        let _ = std::fs::remove_file(socket_path.clone());
        let _ = thread::Builder::new()
            .name("ipc_server".to_string())
            .spawn(move || {
                let listener = UnixListener::bind(socket_path.clone()).unwrap();
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let _ = thread::Builder::new().name("ipc_client".to_string()).spawn(
                                move || {
                                    let r = BufReader::new(stream);
                                    for line in r.lines() {
                                        match line {
                                            Ok(ref cmd) if cmd == "kill" => {
                                                std::process::exit(0);
                                            }
                                            _ => {}
                                        }
                                    }
                                },
                            );
                        }
                        _ => (),
                    }
                }
            });
    }

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
        config.background,
        widgets,
        layout,
        MaybeFontMap::Waiting(font_thread),
        events,
        keyrepeat_sender,
    );

    // Initial setup
    //
    while state.running && !state.ready {
        event_loop
            .dispatch(None, &mut state)
            .expect("Could not dispatch event loop");
    }
    state.check_registry(&qhandle);

    while state.running {
        event_loop
            .dispatch(None, &mut state)
            .expect("Could not dispatch event loop");

        if !state.configured || !state.dirty || !state.ready {
            continue;
        }

        let mut force = false;
        if state.bufmgr.buffers.len() == 0 {
            state.add_buffer(&qhandle);
            force = true;
        }

        let mut bufcnt = state.bufmgr.buffers.len();
        let buf = match state.bufmgr.next_buffer() {
            Some(b) => b,
            None => {
                if state.bufmgr.buffers.len() >= 3 {
                    continue;
                }
                state.add_buffer(&qhandle);
                bufcnt = state.bufmgr.buffers.len();

                match state.bufmgr.next_buffer() {
                    Some(b) => b,
                    None => {
                        continue;
                    }
                }
            }
        };

        if buf.last_damage.len() == 0 {
            for _ in 0..state.widgets.len() {
                buf.last_damage.push(Geometry::new());
            }
        }

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
                    buf.last_damage[idx] =
                        widget.draw(&mut state.fonts.unwrap().borrow_mut(), &mut subview);
                }
            }
            surface.damage_buffer(0, 0, 0x7FFFFFFF, 0x7FFFFFFF);
        } else {
            let mut drew = false;
            for (idx, widget) in state.widgets.iter_mut().enumerate() {
                if bufcnt > 1 || widget.get_dirty() {
                    drew = true;

                    let old_damage = buf.last_damage[idx];
                    bufview.subgeometry(old_damage).clear();

                    let geo = widget.geometry();
                    let mut subview = bufview.subgeometry(geo);

                    let new_damage =
                        widget.draw(&mut state.fonts.unwrap().borrow_mut(), &mut subview);
                    let combined_damage = new_damage.expand(old_damage);
                    buf.last_damage[idx] = new_damage;

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
                conn.flush().unwrap();
                continue;
            }
        }

        state.ready = false;
        surface.attach(Some(&buf.buffer), 0, 0);
        surface.frame(&qhandle, ());
        surface.commit();
        conn.flush().unwrap();

        // Now is a good as time as any to load the keymap
        state.keyboard.resolve();
    }
}
