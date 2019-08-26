use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::sync::mpsc::channel;
use std::env;

use nix::poll::{poll, PollFd, PollFlags};
use os_pipe::pipe;

mod buffer;
mod color;
mod draw;
mod modules;
mod app;

use app::{App, Cmd};

fn main() {
    let (mut rx_pipe, mut tx_pipe) = pipe().unwrap();
    let (tx_draw, rx_draw) = channel();

    let all_outputs = match env::var("WLDASH_ALL_OUTPUTS") {
        Ok(_) => true,
        Err(_) => false,
    };

    let mut app = App::new(tx_draw, all_outputs);
    app.wipe();

    let worker_queue = app.cmd_queue();
    std::thread::spawn(move || loop {
        if rx_draw.recv().unwrap() {
            worker_queue.lock().unwrap().push_back(Cmd::Draw);
        } else {
            worker_queue.lock().unwrap().push_back(Cmd::ForceDraw);
        }
        tx_pipe.write_all(&[0x1]).unwrap();
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
