use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::{collections::HashMap, sync::mpsc::channel};

use chrono::{Duration, Local};
use nix::poll::{poll, PollFd, PollFlags};
use os_pipe::pipe;
use timerfd::{SetTimeFlags, TimerFd, TimerState};

mod app;
mod buffer;
mod cmd;
mod color;
mod config;
mod configfmt;
mod data;
mod desktop;
mod doublemempool;
mod draw;
mod fonts;
mod keyboard;
mod widget;
mod widgets;

#[macro_use]
extern crate dlib;

use app::{App, OutputMode};
use cmd::Cmd;
use config::Config;
use configfmt::ConfigFmt;
use fonts::{FontLoader, FontMap, FontSeeker};
use widget::WaitContext;

enum Mode {
    Start,
    Daemonize,
    StartOrKill,
    ToggleVisible,
    PrintConfig(ConfigFmt),
}

fn main() {
    let socket_path = match env::var("XDG_RUNTIME_DIR") {
        Ok(dir) => dir + "/wldash",
        Err(_) => "/tmp/wldash".to_string(),
    };
    let config_home = match env::var("XDG_CONFIG_HOME") {
        Ok(dir) => dir + "/wldash",
        Err(_) => match env::var("HOME") {
            Ok(home) => home + "/.config/wldash",
            _ => panic!("unable to find user folder"),
        },
    };

    // From all existing files take the first readable one and write it's extension to `ext`
    let mut ext = [0x0; 8];
    let file = configfmt::CONFIG_NAMES
        .iter()
        .map(|name| std::path::Path::new(&config_home).join(name))
        .filter_map(|path| {
            match File::open(&path) {
                Ok(file) => {
                    let e = path.extension().and_then(|e| e.to_str())?;
                    let len = e.len();
                    let from = len.saturating_sub(8); // the longest possible extension
                    ext[0..len - from].copy_from_slice(&e.as_bytes()[from..]);
                    Some(file)
                }
                Err(_) => None,
            }
        })
        .next();

    let fmt = std::str::from_utf8(&ext)
        .ok()
        .and_then(ConfigFmt::new)
        .unwrap_or_default();

    let config: Config = file
        .map(|f| fmt.from_reader(BufReader::new(f)))
        .unwrap_or_default();

    let scale = config.scale;

    let fonts: FontMap = {
        let load_font = |font_name| {
            let path = FontSeeker::from_string(font_name);
            FontLoader::from_path(&path).expect(&format!("Loading {} failed", path.display()))
        };

        config
            .fonts
            .iter()
            .map(|(key, val)| (key.clone(), load_font(val)))
            .collect::<HashMap<_, _>>()
    };

    let mut args = env::args();
    let _ = args.next();
    let mode = match args.next() {
        Some(arg) => match arg.as_str() {
            "start" => Mode::Daemonize,
            "start-or-kill" => Mode::StartOrKill,
            "toggle-visible" => Mode::ToggleVisible,
            "print-config" => Mode::PrintConfig(fmt),
            s => {
                let p = "print-config-";
                let l = p.len();
                let fmt = if s.starts_with(p) {
                    ConfigFmt::new(&s[l..])
                } else {
                    None
                };
                if let Some(fmt) = fmt {
                    Mode::PrintConfig(fmt)
                } else {
                    eprintln!("unsupported sub-command {}", s);
                    std::process::exit(1);
                }
            }
        },
        None => Mode::Start,
    };
    if args.next().is_some() {
        // total = args.count + 2 (the two we skipped + the rest)
        eprintln!("expected 0 or 1 arguments, got {}", args.count() + 2);
        std::process::exit(1);
    }

    let mut daemon = false;

    match mode {
        Mode::ToggleVisible => {
            if let Ok(mut socket) = UnixStream::connect(socket_path) {
                socket.write_all(b"toggle_visible\n").unwrap();
                return;
            };
            eprintln!("wldash is not running");
            std::process::exit(1);
        }
        Mode::StartOrKill => {
            if let Ok(mut socket) = UnixStream::connect(socket_path.clone()) {
                socket.write_all(b"kill\n").unwrap();
                return;
            };
        }
        Mode::Start => {
            if UnixStream::connect(socket_path.clone()).is_ok() {
                eprintln!("wldash is already running");
                std::process::exit(1);
            };
        }
        Mode::Daemonize => {
            if UnixStream::connect(socket_path.clone()).is_ok() {
                eprintln!("wldash is already running");
                std::process::exit(1);
            };
            daemon = true;
        }
        Mode::PrintConfig(fmt) => {
            println!("{}", fmt.to_string(&config));
            std::process::exit(0);
        }
    }

    let _ = std::fs::remove_file(socket_path.clone());
    let listener = UnixListener::bind(socket_path.clone()).unwrap();

    let output_mode = match config.output_mode {
        config::OutputMode::All => OutputMode::All,
        config::OutputMode::Active => OutputMode::Active,
    };

    let background = config.background;

    let (tx_draw, rx_draw) = channel();
    let tx_draw_mod = tx_draw.clone();

    // Print, write to a file, or send to an HTTP server.
    let widget = config
        .widget
        .construct(Local::now().naive_local(), tx_draw_mod, &fonts)
        .expect("no widget configured");

    let mut app = App::new(tx_draw, output_mode, background, scale);
    if daemon {
        app.hide();
    } else {
        app.show();
    }
    app.set_widget(widget).unwrap();

    let (mut rx_pipe, mut tx_pipe) = pipe().unwrap();
    let ipc_pipe = tx_pipe.try_clone().unwrap();

    let worker_queue = app.cmd_queue();
    let _ = std::thread::Builder::new()
        .name("cmd_proxy".to_string())
        .spawn(move || loop {
            let cmd = rx_draw.recv().unwrap();
            worker_queue.lock().unwrap().push_back(cmd);
            tx_pipe.write_all(&[0x1]).unwrap();
        });

    let mut timer = TimerFd::new().unwrap();
    let ev_fd = PollFd::new(
        app.event_queue().display().get_connection_fd(),
        PollFlags::POLLIN,
    );
    let rx_fd = PollFd::new(rx_pipe.as_raw_fd(), PollFlags::POLLIN);
    let tm_fd = PollFd::new(timer.as_raw_fd(), PollFlags::POLLIN);
    let ipc_fd = PollFd::new(listener.as_raw_fd(), PollFlags::POLLIN);

    app.cmd_queue().lock().unwrap().push_back(Cmd::Draw);

    let mut visible = !daemon;
    let mut wait_ctx = WaitContext {
        fds: Vec::new(),
        target_time: None,
    };

    let q = app.cmd_queue();
    loop {
        let cmd = q.lock().unwrap().pop_front();
        match cmd {
            Some(cmd) => match cmd {
                Cmd::Draw => {
                    app.redraw(false).expect("Failed to draw");
                    app.flush_display();
                }
                Cmd::KeyboardTest => {
                    if let Some(cmd) = app.key_repeat() {
                        q.lock().unwrap().push_back(cmd);
                    }
                }
                Cmd::ForceDraw => {
                    app.redraw(true).expect("Failed to draw");
                    app.flush_display();
                }
                Cmd::MouseClick { btn, pos } => {
                    app.get_widget().mouse_click(btn, pos);
                    q.lock().unwrap().push_back(Cmd::Draw);
                }
                Cmd::MouseScroll { scroll, pos } => {
                    app.get_widget().mouse_scroll(scroll, pos);
                    q.lock().unwrap().push_back(Cmd::Draw);
                }
                Cmd::Keyboard {
                    key,
                    key_state,
                    modifiers_state,
                    interpreted,
                } => {
                    app.get_widget()
                        .keyboard_input(key, modifiers_state, key_state, interpreted);
                    q.lock().unwrap().push_back(Cmd::Draw);
                }
                Cmd::ToggleVisible => {
                    visible = !visible;
                    if visible {
                        app.get_widget().enter();
                        app.show();
                    } else {
                        app.hide();
                        app.get_widget().leave();
                    }
                    app.flush_display();
                }
                Cmd::Exit => {
                    if daemon {
                        visible = false;
                        app.hide();
                        app.get_widget().leave();
                        app.flush_display();
                    } else {
                        let _ = std::fs::remove_file(socket_path);
                        return;
                    }
                }
            },
            None => {
                app.flush_display();

                wait_ctx.fds.clear();
                wait_ctx.fds.push(ev_fd);
                wait_ctx.fds.push(rx_fd);
                wait_ctx.fds.push(ipc_fd);
                wait_ctx.target_time = None;

                app.get_widget().wait(&mut wait_ctx);
                app.set_keyboard_repeat(&mut wait_ctx);

                if let Some(target_time) = wait_ctx.target_time {
                    let n = Local::now().naive_local();
                    let sleep = if target_time > n {
                        target_time - n
                    } else {
                        Duration::seconds(0)
                    };
                    timer.set_state(
                        TimerState::Oneshot(sleep.to_std().unwrap()),
                        SetTimeFlags::Default,
                    );
                    wait_ctx.fds.push(tm_fd);
                }

                poll(&mut wait_ctx.fds, -1).unwrap();

                if wait_ctx.fds[0]
                    .revents()
                    .unwrap()
                    .contains(PollFlags::POLLIN)
                {
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
                        .dispatch_pending(&mut (), |_, _, _| {})
                        .expect("Failed to dispatch all messages.");
                }

                if wait_ctx.fds[1]
                    .revents()
                    .unwrap()
                    .contains(PollFlags::POLLIN)
                {
                    let mut v = [0x00];
                    rx_pipe.read_exact(&mut v).unwrap();
                }

                if wait_ctx.fds[2]
                    .revents()
                    .unwrap()
                    .contains(PollFlags::POLLIN)
                {
                    if let Ok((stream, _)) = listener.accept() {
                        let client_queue = q.clone();
                        let mut client_pipe = ipc_pipe.try_clone().unwrap();
                        let _ = std::thread::Builder::new()
                            .name("ipc_client".to_string())
                            .spawn(move || {
                                let r = BufReader::new(stream);
                                for line in r.lines() {
                                    match line {
                                        Ok(v) => match v.as_str() {
                                            "kill" => {
                                                client_queue.lock().unwrap().push_back(Cmd::Exit)
                                            }
                                            "toggle_visible" => client_queue
                                                .lock()
                                                .unwrap()
                                                .push_back(Cmd::ToggleVisible),
                                            v => eprintln!("unknown command: {}", v),
                                        },
                                        Err(_) => return,
                                    }
                                    client_pipe.write_all(&[0x1]).unwrap();
                                }
                            });
                    }
                }

                if wait_ctx.target_time.is_some()
                    && wait_ctx.fds[wait_ctx.fds.len() - 1]
                        .revents()
                        .unwrap()
                        .contains(PollFlags::POLLIN)
                {
                    timer.read();
                    let mut qq = q.lock().unwrap();
                    qq.push_back(Cmd::KeyboardTest);
                    qq.push_back(Cmd::Draw);
                }
            }
        }
    }
}
