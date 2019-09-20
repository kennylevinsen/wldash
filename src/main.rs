use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc::channel;

use nix::poll::{poll, PollFd, PollFlags};
use os_pipe::pipe;

mod app;
mod buffer;
mod cmd;
mod color;
mod config;
mod configfmt;
mod desktop;
mod doublemempool;
mod draw;
mod widget;
mod widgets;

use app::{App, OutputMode};
use config::Config;
use cmd::Cmd;
use configfmt::ConfigFmt;

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
            Err(_) => panic!("unable to find user folder"),
        },
    };

    // From all existing files take the first readable one and write it's extension to `ext`
    let mut ext = [0x0; 8];
    let file = configfmt::CONFIG_NAMES
        .iter()
        .map(|name| { std::path::Path::new(&config_home).join(name) })
        .filter_map(|path| {
            match File::open(&path) {
                Ok(file) => {
                    let e = path.extension().and_then(|e| e.to_str())?;
                    let len = e.len();
                    let from = len.saturating_sub(8); // the longest possible extension
                    ext[0..len - from].copy_from_slice(&e.as_bytes()[from..]);
                    Some(file)
                },
                Err(_) => None
            }
        })
        .next();

    let fmt = std::str::from_utf8(&ext).ok()
        .and_then(ConfigFmt::new)
        .unwrap_or_default();
    
    let config: Config = file.map(|f| fmt.from_reader(BufReader::new(f))).unwrap_or_default();

    let scale = config.scale;

    let mut args = env::args();
    let _ = args.next();
    let mode = match args.next() {
        Some(arg) => {
            match arg.as_str() {
                "start" => Mode::Daemonize,
                "start-or-kill" => Mode::StartOrKill,
                "toggle-visible" => Mode::ToggleVisible,
                "print-config" => Mode::PrintConfig(fmt),
                s => {
                    let ext = s.trim_start_matches("print-config-");
                    match ConfigFmt::new(ext) {
                        Some(fmt) => Mode::PrintConfig(fmt),
                        None => {
                            eprintln!("unsupported sub-command {}", s);
                            std::process::exit(1);
                        }
                    }
                }
            }
        },
        None => Mode::Start
    };
    if let Some(_) = args.next() {
        // total = args.count + 2 (the two we skipped + the rest)
        eprintln!("expected 0 or 1 arguments, got {}", args.count() + 2);
        std::process::exit(1);
    }

    let mut daemon = false;

    match mode {
        Mode::ToggleVisible => {
            if let Ok(mut socket) = UnixStream::connect(socket_path.clone()) {
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
            if let Ok(_) = UnixStream::connect(socket_path.clone()) {
                eprintln!("wldash is already running");
                std::process::exit(1);
            };
        }
        Mode::Daemonize => {
            if let Ok(_) = UnixStream::connect(socket_path.clone()) {
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
    let (mod_tx, mod_rx) = channel();
    std::thread::spawn(move || {
        // Print, write to a file, or send to an HTTP server.
        match config.widget.construct(tx_draw_mod) {
            Some(w) => mod_tx.send(w).unwrap(),
            None => panic!("no widget configured"),
        }
    });

    let mut app = App::new(tx_draw, output_mode, background, scale);
    if daemon {
        app.hide();
    } else {
        app.show();
    }
    let widget = mod_rx.recv().unwrap();
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

    let ipc_queue = app.cmd_queue();
    let _ = std::thread::Builder::new()
        .name("ipc_server".to_string())
        .spawn(move || loop {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let client_queue = ipc_queue.clone();
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
                    Err(_) => break,
                }
            }
        });

    let mut fds = [
        PollFd::new(app.event_queue().get_connection_fd(), PollFlags::POLLIN),
        PollFd::new(rx_pipe.as_raw_fd(), PollFlags::POLLIN),
    ];

    app.cmd_queue().lock().unwrap().push_back(Cmd::Draw);

    let q = app.cmd_queue();
    loop {
        let cmd = q.lock().unwrap().pop_front();
        match cmd {
            Some(cmd) => match cmd {
                Cmd::Draw => {
                    app.redraw(false).expect("Failed to draw");
                    app.flush_display();
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
                    app.toggle_visible();
                    app.flush_display();
                }
                Cmd::Exit => {
                    if daemon {
                        app.hide();
                        app.flush_display();
                    } else {
                        let _ = std::fs::remove_file(socket_path);
                        return;
                    }
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
                    let mut v = [0x00];
                    rx_pipe.read_exact(&mut v).unwrap();
                }
            }
        }
    }
}
