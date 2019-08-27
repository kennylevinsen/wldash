use std::io::{Read, Write, BufRead, BufReader};
use std::os::unix::io::AsRawFd;
use std::sync::mpsc::channel;
use std::env;
use std::os::unix::net::{UnixStream, UnixListener};

use nix::poll::{poll, PollFd, PollFlags};
use os_pipe::pipe;

mod buffer;
mod color;
mod draw;
mod modules;
mod app;
mod cmd;

use app::{App, OutputMode};
use cmd::Cmd;

enum Mode {
    Start,
    StartOrKill,
    ToggleVisible,
}

fn main() {
    let socket_path = match env::var("XDG_RUNTIME_DIR") {
        Ok(dir) => dir + "/wldash",
        Err(_) => "/tmp/wldash".to_string(),
    };

    let args: Vec<String> = env::args().collect();
    let mode = match args.len() {
        1 => Mode::Start,
        2 => match args[1].as_str() {
            "start" => Mode::Start,
            "start-or-kill" => Mode::StartOrKill,
            "toggle-visible" => Mode::ToggleVisible,
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
    }

    let _ = std::fs::remove_file(socket_path.clone());

    let listener = UnixListener::bind(socket_path.clone()).unwrap();

    let (mut rx_pipe, mut tx_pipe) = pipe().unwrap();
    let ipc_pipe = tx_pipe.try_clone().unwrap();
    let (tx_draw, rx_draw) = channel();

    let output_mode = match env::var("WLDASH_ALL_OUTPUTS") {
        Ok(_) => OutputMode::All,
        Err(_) => OutputMode::Active,
    };

    let mut app = App::new(tx_draw, output_mode);
    app.wipe();

    let worker_queue = app.cmd_queue();
    std::thread::spawn(move || loop {
        let cmd = rx_draw.recv().unwrap();
        worker_queue.lock().unwrap().push_back(cmd);
        tx_pipe.write_all(&[0x1]).unwrap();
    });

    let ipc_queue = app.cmd_queue();
    std::thread::spawn(move || loop {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let client_queue = ipc_queue.clone();
                    let mut client_pipe = ipc_pipe.try_clone().unwrap();
                    std::thread::spawn(move || {
                        let r = BufReader::new(stream);
                        for line in r.lines() {
                            match line {
                                Ok(v) => match v.as_str() {
                                    "kill" => client_queue.lock().unwrap().push_back(Cmd::Exit),
                                    "toggle_visible" => client_queue.lock().unwrap().push_back(Cmd::ToggleVisible),
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
                Cmd::MouseInput { pos, input } => {
                    if let Some(m) = app.get_module(pos) {
                        let bounds = m.get_bounds();
                        let input = input.offset((bounds.0, bounds.1));
                        m.input(input);
                        q.lock().unwrap().push_back(Cmd::Draw);
                    }
                }
                Cmd::KeyboardInput { input } => {
                    app.with_modules(|m| {
                        m.input(input.clone());
                    });
                    q.lock().unwrap().push_back(Cmd::Draw);
                }
                Cmd::ToggleVisible => {
                    app.toggle_visible();
                }
                Cmd::Exit => {
                    let _ = std::fs::remove_file(socket_path);
                    return;
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
