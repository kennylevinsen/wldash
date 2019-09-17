use std::default::Default;
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
mod desktop;
mod doublemempool;
mod draw;
mod widget;
mod widgets;

use app::{App, OutputMode};
use cmd::Cmd;

enum Mode {
    Start,
    Daemonize,
    StartOrKill,
    ToggleVisible,
    PrintConfig(bool),
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

    let (is_yaml, config): (bool, config::Config) = match File::open(config_home.clone() + "/config.yaml") {
        Ok(f) => {
            let reader = BufReader::new(f);
            (true, serde_yaml::from_reader(reader).unwrap())
        }
        Err(_) =>  match File::open(config_home + "/config.json") {
            Ok(f) => {
                let reader = BufReader::new(f);
                (false, serde_json::from_reader(reader).unwrap())
            }
            Err(_) => (true, Default::default()),
        }
    };

    let scale = config.scale;

    let args: Vec<String> = env::args().collect();
    let mode = match args.len() {
        1 => Mode::Start,
        2 => match args[1].as_str() {
            "start" => Mode::Daemonize,
            "start-or-kill" => Mode::StartOrKill,
            "toggle-visible" => Mode::ToggleVisible,
            "print-config" => Mode::PrintConfig(!is_yaml),
            "print-config-json" => Mode::PrintConfig(true),
            "print-config-yaml" => Mode::PrintConfig(false),
            s => {
                eprintln!("unsupported sub-command {}", s);
                std::process::exit(1);
            }
        },
        v => {
            eprintln!("expected 0 or 1 arguments, got {}", v);
            std::process::exit(1);
        }
    };

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
        Mode::PrintConfig(json) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&config).unwrap());
            } else {
                println!("{}", serde_yaml::to_string(&config).unwrap());
            }
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
