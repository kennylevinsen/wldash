mod buffer;
mod color;
mod draw;
mod fonts;
mod keyboard;
mod utils;
mod widgets;
mod state;

use wayland_client::{Connection, QueueHandle, WaylandSource,};

use calloop::{
    timer::{TimeoutAction, Timer},
    EventLoop,
};
use chrono::{Duration, Local, Timelike};

use buffer::{BufferManager, BufferView};
use color::Color;
use fonts::{MaybeFontMap, FontMap};
use keyboard::Keyboard;
use widgets::{
    Backlight, Battery, Clock, Date, Geometry, Interface, Line, WidgetUpdater,
};
use state::State;

use std::thread;

impl WidgetUpdater for State {
    fn geometry_update(
        &mut self,
        idx: usize,
        fonts: &mut FontMap,
        geometry: &Geometry,
    ) -> Geometry {
        self.widgets[idx].geometry_update(fonts, geometry)
    }
}

fn main() {
    let font_thread = thread::spawn(move || {
        let mut fm = FontMap::new();
        // Hard-coding font paths make things a lot faster
        fm.queue_font_path("sans", "/usr/share/fonts/noto/NotoSans-Regular.ttf", 128.);
        fm.queue_font_path("sans", "/usr/share/fonts/noto/NotoSans-Regular.ttf", 48.);
        fm.queue_font_path("sans", "/usr/share/fonts/noto/NotoSans-Regular.ttf", 24.);
        fm.queue_font_path(
            "monospace",
            "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
            32.,
        );
        fm.load_fonts();
        fm
    });
    let conn = Connection::connect_to_env().unwrap();

    let event_queue = conn.new_event_queue();
    let qhandle: QueueHandle<State> = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let bufmgr = BufferManager {
        buffers: Vec::new(),
    };

    let (ping_sender, ping_source) = calloop::ping::make_ping().unwrap();

    let clock = Box::new(Clock::new("sans", 128.));
    let date = Box::new(Date::new("sans", 48.));
    let line = Box::new(Line::new());
    let launcher = Box::new(Interface::new("monospace", 32.));
    let battery = Box::new(Battery::new(ping_sender, "sans", 24.));
    let backlight = Box::new(Backlight::new("intel_backlight", "sans", 24.));

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
            state.widgets[2].set_dirty(true);
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
            state.widgets[0].set_dirty(true);
            state.widgets[1].set_dirty(true);
            state.dirty = true;

            // The timer event source requires us to return a TimeoutAction to
            // specify if the timer should be rescheduled. In our case we just drop it.
            TimeoutAction::ToDuration(d.to_std().unwrap())
        })
        .expect("Failed to insert event source!");

    let mut state = State {
        running: true,
        dirty: true,
        activated: false,
        base_surface: None,
        wm_base: None,
        xdg_surface: None,
        wl_shm: None,
        xdg_activation: None,
        configured: false,
        dimensions: (320, 240),
        bufmgr: bufmgr,
        widgets: vec![clock, date, battery, backlight, line, launcher],
        keyboard: Keyboard::new(),
        fonts: MaybeFontMap::Waiting(font_thread),
    };

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

        let mut drew = false;
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

        let bg = Color::new(0., 0., 0., 1.);

        let surface = state.base_surface.as_ref().unwrap().clone();
        for (idx, widget) in state.widgets.iter_mut().enumerate() {
            if force || widget.get_dirty() {
                widget.set_dirty(false);
                drew = true;

                let geo = widget.geometry();
                let old_damage = damage[idx];

                if !force {
                    bufview.subgeometry(old_damage).unwrap().memset(&bg);
                } else {
                    bufview.subgeometry(geo).unwrap().memset(&bg);
                }

                let mut subview = bufview.subgeometry(geo).unwrap();
                let new_damage = widget.draw(&mut state.fonts.unwrap().borrow_mut(), &mut subview);
                let combined_damage = new_damage.expand(old_damage);
                damage[idx] = new_damage;
                surface.damage_buffer(
                    combined_damage.x as i32,
                    combined_damage.y as i32,
                    combined_damage.width as i32,
                    combined_damage.height as i32,
                );
                //draw_box(&mut subview, &Color::new(1.0, 0.5, 0.0, 1.0), (geo.width, geo.height));
            }
        }
        if !drew {
            buf.release();
            continue;
        }

        if force {
            surface.damage_buffer(0, 0, 0x7FFFFFFF, 0x7FFFFFFF)
        }
        surface.attach(Some(&buf.buffer), 0, 0);
        surface.commit();
    }
}

