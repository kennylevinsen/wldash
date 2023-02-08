mod buffer;
mod color;
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
use event::{Event, Events};
use fonts::{FontMap, MaybeFontMap};
use state::{OperationMode, State};
use widgets::{
    Backlight, Battery, Calendar, Clock, Date, Geometry, HorizontalLayout, IndexedLayout,
    Interface, InvertedHorizontalLayout, Layout, Line, Margin, PulseAudio, VerticalLayout, Widget,
};

use std::{env, rc::Rc, thread};

enum UIVersion {
    V1,
    V2,
}

fn main() {
    let mut args = env::args();
    // Skip program name
    _ = args.next();

    let mut version = UIVersion::V2;
    let mut mode = OperationMode::XdgToplevel;
    loop {
        match args.next() {
            Some(ref s) if s == "layer_shell" => {
                let width = args.next().map(|x| x.parse::<u32>().ok()).flatten();
                let height = args.next().map(|x| x.parse::<u32>().ok()).flatten();
                match (width, height) {
                    (Some(w), Some(h)) => mode = OperationMode::LayerSurface((w, h)),
                    _ => panic!("invalid arguments"),
                }
            }
            Some(ref s) if s == "xdg_shell" => mode = OperationMode::XdgToplevel,
            Some(ref s) if s == "v1" => version = UIVersion::V1,
            Some(ref s) if s == "v2" => version = UIVersion::V2,
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

    let events = Events::new(ping_sender);

    let mut fm = FontMap::new();

    // TODO: Cache in a file
    fm.add_font_path(
        "sans",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf".to_string(),
    );
    fm.add_font_path(
        "monospace",
        "/usr/share/fonts/noto/NotoSansMono-Regular.ttf".to_string(),
    );

    let (widgets, layout): (Vec<Box<dyn Widget>>, Rc<Box<dyn Layout>>) = match version {
        UIVersion::V1 => {
            let clock = Box::new(Clock::new(&mut fm, "sans", 256.));
            let calendar = Box::new(Calendar::new(&mut fm, "monospace", "sans", 36.0, 3, 1));
            let date = Box::new(Date::new(&mut fm, "sans", 48.));
            let launcher = Box::new(Interface::new(events.clone(), &mut fm, "monospace", 32.));
            let battery = Box::new(Battery::new(events.clone(), &mut fm, "sans", 24.));
            let backlight = Box::new(Backlight::new("intel_backlight", &mut fm, "sans", 24.));
            let audio = Box::new(PulseAudio::new(events.clone(), &mut fm, "sans", 24.));

            let v: Vec<Box<dyn Widget>> =
                vec![clock, date, battery, backlight, audio, calendar, launcher];
            let l = Rc::new(Margin::new(
                VerticalLayout::new(vec![
                    HorizontalLayout::new(vec![
                        VerticalLayout::new(vec![IndexedLayout::new(1), IndexedLayout::new(0)]),
                        Margin::new(
                            VerticalLayout::new(vec![
                                Margin::new(IndexedLayout::new(2), (0, 0, 0, 8)),
                                Margin::new(IndexedLayout::new(3), (0, 0, 0, 8)),
                                Margin::new(IndexedLayout::new(4), (0, 0, 0, 8)),
                            ]),
                            (88, 0, 0, 0),
                        ),
                    ]),
                    IndexedLayout::new(5),
                    IndexedLayout::new(6),
                ]),
                (20, 20, 20, 20),
            ));
            (v, l)
        }
        UIVersion::V2 => {
            let clock = Box::new(Clock::new(&mut fm, "sans", 128.));
            let calendar = Box::new(Calendar::new(&mut fm, "monospace", "sans", 24.0, 1, -1));
            let date = Box::new(Date::new(&mut fm, "sans", 48.));
            let line = Box::new(Line::new(1));
            let launcher = Box::new(Interface::new(events.clone(), &mut fm, "monospace", 32.));
            let battery = Box::new(Battery::new(events.clone(), &mut fm, "sans", 24.));
            let backlight = Box::new(Backlight::new("intel_backlight", &mut fm, "sans", 24.));
            let audio = Box::new(PulseAudio::new(events.clone(), &mut fm, "sans", 24.));
            let v: Vec<Box<dyn Widget>> = vec![
                clock, date, battery, backlight, audio, line, calendar, launcher,
            ];
            let l = Rc::new(VerticalLayout::new(vec![
                HorizontalLayout::new(vec![
                    IndexedLayout::new(0),
                    Margin::new(IndexedLayout::new(1), (16, 16, 0, 0)),
                    VerticalLayout::new(vec![
                        Margin::new(IndexedLayout::new(2), (16, 8, 8, 0)),
                        Margin::new(IndexedLayout::new(3), (16, 8, 8, 0)),
                        Margin::new(IndexedLayout::new(4), (16, 8, 8, 0)),
                    ]),
                ]),
                IndexedLayout::new(5),
                InvertedHorizontalLayout::new(vec![IndexedLayout::new(6), IndexedLayout::new(7)]),
            ]));
            (v, l)
        }
    };

    let font_thread = thread::Builder::new()
        .name("fontloader".to_string())
        .spawn(move || {
            fm.load_fonts();
            fm
        })
        .unwrap();

    let mut state = State::new(
        mode,
        widgets,
        layout,
        MaybeFontMap::Waiting(font_thread),
        events,
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
