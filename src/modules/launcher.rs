use crate::buffer::Buffer;
use crate::cmd::Cmd;
use crate::color::Color;
use crate::draw::{Font, ROBOTO_REGULAR};
use crate::modules::module::{Input, ModuleImpl};
use crate::desktop::{Desktop, load_desktop_files};

use std::env;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::sync::mpsc::Sender;
use std::process::Command;

use chrono::{DateTime, Local};
use fuzzy_matcher::skim::{fuzzy_indices, fuzzy_match};
use smithay_client_toolkit::keyboard::keysyms;

pub struct Launcher {
    options: Vec<Desktop>,
    term_opener: String,
    app_opener: String,
    url_opener: String,
    matches: Vec<Desktop>,
    input: String,
    result: Option<String>,
    offset: usize,
    font: RefCell<Font>,
    dirty: bool,
    tx: Sender<Cmd>,
}

impl Launcher {
    pub fn new(listener: Sender<Cmd>) -> Result<Launcher, ::std::io::Error> {
        let term = match env::var_os("WLDASH_TERM_OPENER") {
            Some(s) => format!("{:} ", s.into_string().unwrap()).to_string(),
            None => "".to_string(),
        };
        let app = match env::var_os("WLDASH_APP_OPENER") {
            Some(s) => format!("{:} ", s.into_string().unwrap()).to_string(),
            None => "".to_string(),
        };
        let url = match env::var_os("WLDASH_URL_OPENER") {
            Some(s) => format!("{:} ", s.into_string().unwrap()).to_string(),
            None => "xdg-open ".to_string(),
        };

        Ok(Launcher {
            options: load_desktop_files(),
            term_opener: term,
            app_opener: app,
            url_opener: url,
            matches: vec![],
            input: "".to_string(),
            result: None,
            offset: 0,
            font: RefCell::new(Font::new(&ROBOTO_REGULAR, 32.0)),
            dirty: true,
            tx: listener,
        })
    }

    fn draw_launcher(
        &self,
        buf: &mut Buffer,
        bg: &Color,
    ) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error> {
        buf.memset(bg);

        let mut x_off = if self.input.len() > 0 {
            let c = if self.matches.len() == 0 {
                Color::new(1.0, 0.5, 0.5, 1.0)
            } else {
                Color::new(1.0, 1.0, 1.0, 1.0)
            };
            let dim = self.font.borrow_mut().auto_draw_text(
                buf,
                bg,
                &c,
                &self.input,
            )?;

            dim.0 + 8
        } else {
            0
        };

        let mut width_remaining: i32 = 1232 - x_off as i32;
        for (idx, m) in self.matches.iter().enumerate() {
            let mut b = match buf.offset((x_off, 0)) {
                Ok(b) => b,
                Err(_) => break,
            };
            let size = if idx == self.offset && self.input.len() > 0 {
                let (_, indices) =
                    fuzzy_indices(&m.name.to_lowercase(), &self.input.to_lowercase()).unwrap();
                let mut colors = Vec::with_capacity(m.name.len());
                for pos in 0..m.name.len() {
                    if indices.contains(&pos) {
                        colors.push(Color::new(1.0, 1.0, 1.0, 1.0));
                    } else {
                        colors.push(Color::new(0.75, 0.75, 0.75, 1.0));
                    }
                }
                self.font
                    .borrow_mut()
                    .auto_draw_text_individual_colors(&mut b, bg, &colors, &m.name)?
            } else {
                self.font.borrow_mut().auto_draw_text(
                    &mut b,
                    bg,
                    &Color::new(0.5, 0.5, 0.5, 1.0),
                    &m.name,
                )?
            };

            x_off += size.0 + 8;
            width_remaining -= (size.0 + 8) as i32;

            if width_remaining < 0 {
                break;
            }
        }

        Ok(vec![buf.get_signed_bounds()])
    }

    fn draw_calc(
        &self,
        buf: &mut Buffer,
        bg: &Color,
    ) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error> {
        buf.memset(bg);

        let x_off = self
            .font
            .borrow_mut()
            .auto_draw_text(
                buf,
                bg,
                &Color::new(1.0, 1.0, 0.0, 1.0),
                "=",
            )?
            .0
            + 8;

        let x_off = if self.input.len() > 0 {
            let dim = self.font.borrow_mut().auto_draw_text(
                &mut buf.offset((x_off, 0))?,
                bg,
                &Color::new(1.0, 1.0, 1.0, 1.0),
                &self.input[1..],
            )?;

            x_off + dim.0 + 8
        } else {
            0
        };

        if let Some(result) = &self.result {
            self.font.borrow_mut().auto_draw_text(
                &mut buf.offset((x_off, 0))?,
                bg,
                &Color::new(0.75, 0.75, 0.75, 1.0),
                &format!(" = {:}", result),
            )?;
        }

        Ok(vec![buf.get_signed_bounds()])
    }

    fn draw_shell(
        &self,
        buf: &mut Buffer,
        bg: &Color,
    ) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error> {
        buf.memset(bg);

        let x_off = self
            .font
            .borrow_mut()
            .auto_draw_text(
                &mut buf.offset((0, 0))?,
                bg,
                &Color::new(1.0, 1.0, 0.0, 1.0),
                "!",
            )?
            .0
            + 8;

        if self.input.len() > 0 {
            self.font.borrow_mut().auto_draw_text(
                &mut buf.offset((x_off, 0))?,
                bg,
                &Color::new(1.0, 1.0, 1.0, 1.0),
                &self.input[1..],
            )?;
        };

        Ok(vec![buf.get_signed_bounds()])
    }
}

#[cfg(feature = "ivy")]
fn calc(s: &str) -> Result<String, String> {
    libivy::eval(s)
}

#[cfg(feature = "rcalc")]
fn calc(s: &str) -> Result<String, String> {
    rcalc_lib::parse::eval(s, &mut rcalc_lib::parse::CalcState::new())
        .map(|x| format!("{}", x).to_string())
        .map_err(|x| format!("{}", x).to_string())
}

#[cfg(feature = "bc")]
fn calc(s: &str) -> Result<String, String> {
    use std::io::Write;
    let mut child = std::process::Command::new("bc")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .arg("--mathlib")
        .spawn()
        .map_err(|_| "bc not available".to_string())?;
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin
            .write_all(s.as_bytes())
            .map_err(|_| "unable to write to stdin".to_string())?;
        stdin
            .write_all("\n".as_bytes())
            .map_err(|_| "unable to write to stdin".to_string())?;
    }
    let output = child
        .wait_with_output()
        .map_err(|_| "unable to run bc".to_string())?;
    Ok(std::str::from_utf8(&output.stdout)
        .map_err(|_| "unable to read from bc")?
        .trim()
        .to_string())
}

#[cfg(not(any(feature = "ivy", feature = "bc", feature = "rcalc")))]
fn calc(_s: &str) -> Result<String, String> {
    Err("no calculator implementation available".to_string())
}

fn wlcopy(s: &str) -> Result<(), String> {
    let mut child = std::process::Command::new("wl-copy")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .arg(s)
        .spawn()
        .map_err(|_| "wl-copy not available".to_string())?;
    child
        .wait()
        .map_err(|_| "unable to run wl-copy".to_string())?;
    Ok(())
}

impl ModuleImpl for Launcher {
    fn draw(
        &self,
        buf: &mut Buffer,
        bg: &Color,
        _time: &DateTime<Local>,
    ) -> Result<Vec<(i32, i32, i32, i32)>, ::std::io::Error> {
        buf.memset(bg);

        match self.input.chars().next() {
            Some('=') => self.draw_calc(buf, bg),
            Some('!') => self.draw_shell(buf, bg),
            _ => self.draw_launcher(buf, bg),
        }?;

        Ok(vec![buf.get_signed_bounds()])
    }

    fn update(&mut self, _time: &DateTime<Local>, force: bool) -> Result<bool, ::std::io::Error> {
        if self.dirty || force {
            match self.input.chars().next() {
                Some('=') => {
                    if self.input.len() > 1 {
                        match calc(&self.input.chars().skip(1).collect::<String>()) {
                            Ok(v) => self.result = Some(v),
                            Err(_) => self.result = None,
                        }
                    }
                }
                Some('!') => (),
                _ => {
                    let mut m = self
                        .options
                        .iter()
                        .map(|x| {
                            (
                                fuzzy_match(&x.name.to_lowercase(), &self.input.to_lowercase()),
                                x,
                            )
                        })
                        .filter(|(x, _)| x.is_some())
                        .map(|(x, y)| (x.unwrap(), y.clone()))
                        .collect::<Vec<(i64, Desktop)>>();

                    m.sort_by(|(x1, y1), (x2, y2)| {
                        if x1 > x2 {
                            Ordering::Less
                        } else if x1 < x2 {
                            Ordering::Greater
                        } else if y1.name.len() < y2.name.len() {
                            Ordering::Less
                        } else if y1.name.len() > y2.name.len() {
                            Ordering::Greater
                        } else {
                            y1.cmp(y2)
                        }
                    });

                    self.matches = m.into_iter().map(|(_, x)| x).collect();
                }
            };

            self.dirty = false;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn input(&mut self, input: Input) {
        match input {
            Input::Keypress { key, interpreted } => match key {
                keysyms::XKB_KEY_BackSpace => {
                    if self.input.len() > 0 {
                        self.input = self.input[..self.input.len() - 1].to_string();
                        self.offset = 0;
                        self.result = None;
                        self.dirty = true
                    }
                }
                keysyms::XKB_KEY_Return => {
                    match self.input.chars().next() {
                        Some('=') => match self.result {
                            Some(ref v) => {
                                let _ = wlcopy(&v);
                            }
                            None => (),
                        },
                        Some('!') => {
                            let _ = Command::new("sh")
                                    .arg("-c")
                                    .arg(self.input.chars().skip(1).collect::<String>())
                                    .spawn();
                            self.tx.send(Cmd::Exit).unwrap();
                        }
                        _ => {
                            if self.matches.len() > self.offset {
                                let d = &self.matches[self.offset];
                                if let Some(exec) = &d.exec {
                                    let exec = exec
                                        .replace("%f", "")
                                        .replace("%F", "")
                                        .replace("%u", "")
                                        .replace("%U", "");
                                    let prefix = if d.term {
                                        &self.term_opener
                                    } else {
                                        &self.app_opener
                                    };

                                    let lexed = if prefix.len() > 0 {
                                        let mut prefix = shlex::split(prefix).unwrap();
                                        prefix.push(exec);
                                        prefix
                                    } else {
                                        shlex::split(&exec).unwrap()
                                    };
                                    if lexed.len() > 0 {
                                        let _ = Command::new(lexed[0].clone())
                                            .args(&lexed[1..])
                                            .spawn();
                                        self.tx.send(Cmd::Exit).unwrap();
                                    }
                                }
                                if let Some(url) = &d.url {
                                    if self.url_opener.len() > 0 {
                                        let mut lexed = shlex::split(&self.url_opener).unwrap();
                                        lexed.push(url.to_string());
                                        if lexed.len() > 0 {
                                            let _ = Command::new(lexed[0].clone())
                                                .args(&lexed[1..])
                                                .spawn();
                                            self.tx.send(Cmd::Exit).unwrap();
                                        }
                                    }
                                }
                            }
                        }
                    };
                }
                keysyms::XKB_KEY_Right => {
                    if self.matches.len() > 0 && self.offset < self.matches.len() - 1 {
                        self.offset += 1;
                        self.dirty = true;
                    }
                }
                keysyms::XKB_KEY_Left => {
                    if self.matches.len() > 0 && self.offset > 0 {
                        self.offset -= 1;
                        self.dirty = true;
                    }
                }
                _ => match interpreted {
                    Some(v) => {
                        self.input += &v;
                        self.offset = 0;
                        self.result = None;
                        self.dirty = true;
                    }
                    None => {}
                },
            },
            _ => {}
        }
    }
}
