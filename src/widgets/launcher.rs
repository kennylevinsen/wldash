use crate::buffer::Buffer;
use crate::cmd::Cmd;
use crate::color::Color;
use crate::desktop::{load_desktop_files, Desktop};
use crate::draw::{Font, ROBOTO_REGULAR};
use crate::widget::{DrawContext, DrawReport, KeyState, ModifiersState, Widget, WaitContext};

use std::cell::RefCell;
use std::cmp::Ordering;
use std::process::Command;
use std::sync::mpsc::Sender;

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
    font_size: u32,
    length: u32,
    dirty: bool,
    tx: Sender<Cmd>,
}

impl Launcher {
    pub fn new(
        font_size: f32,
        length: u32,
        listener: Sender<Cmd>,
        app: String,
        term: String,
        url: String,
    ) -> Box<Launcher> {
        Box::new(Launcher {
            options: load_desktop_files(),
            term_opener: term,
            app_opener: app,
            url_opener: url,
            matches: vec![],
            input: "".to_string(),
            result: None,
            offset: 0,
            font: RefCell::new(Font::new(&ROBOTO_REGULAR, font_size)),
            font_size: font_size as u32,
            length: length,
            dirty: true,
            tx: listener,
        })
    }

    fn draw_launcher(&self, buf: &mut Buffer, bg: &Color) -> Result<(), ::std::io::Error> {
        let mut x_off = if self.input.len() > 0 {
            let c = if self.matches.len() == 0 {
                Color::new(1.0, 0.5, 0.5, 1.0)
            } else {
                Color::new(1.0, 1.0, 1.0, 1.0)
            };
            let dim = self
                .font
                .borrow_mut()
                .auto_draw_text(buf, bg, &c, &self.input)?;

            dim.0 + self.font_size / 4
        } else {
            0
        };

        let mut width_remaining: i32 = (self.length - x_off) as i32;
        for (idx, m) in self.matches.iter().enumerate() {
            let mut b = match buf.offset((x_off, 0)) {
                Ok(b) => b,
                Err(_) => break,
            };
            let size = if idx == self.offset && self.input.len() > 0 {
                let (_, indices) =
                    fuzzy_indices(&m.name.to_lowercase(), &self.input.to_lowercase())
                    .unwrap_or((0, vec![]));
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

            x_off += size.0 + self.font_size / 2;
            width_remaining -= (size.0 + self.font_size / 2) as i32;

            if width_remaining < 0 {
                break;
            }
        }

        Ok(())
    }

    fn draw_calc(&self, buf: &mut Buffer, bg: &Color) -> Result<(), ::std::io::Error> {
        let x_off = self
            .font
            .borrow_mut()
            .auto_draw_text(buf, bg, &Color::new(1.0, 1.0, 0.0, 1.0), "=")?
            .0
            + self.font_size / 4;

        let x_off = if self.input.len() > 0 {
            let dim = self.font.borrow_mut().auto_draw_text(
                &mut buf.offset((x_off, 0))?,
                bg,
                &Color::new(1.0, 1.0, 1.0, 1.0),
                &self.input[1..],
            )?;

            x_off + dim.0 + self.font_size / 4
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

        Ok(())
    }

    fn draw_shell(&self, buf: &mut Buffer, bg: &Color) -> Result<(), ::std::io::Error> {
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
            + self.font_size / 4;

        if self.input.len() > 0 {
            self.font.borrow_mut().auto_draw_text(
                &mut buf.offset((x_off, 0))?,
                bg,
                &Color::new(1.0, 1.0, 1.0, 1.0),
                &self.input[1..],
            )?;
        };

        Ok(())
    }
}

fn calc(s: &str) -> Result<String, String> {
    rcalc_lib::parse::eval(s, &mut rcalc_lib::parse::CalcState::new())
        .map(|x| format!("{}", x).to_string())
        .map_err(|x| format!("{}", x).to_string())
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

impl Widget for Launcher {
    fn wait(&mut self, _: &mut WaitContext) {}
    fn enter(&mut self) {}
    fn leave(&mut self) {
        self.input = "".to_string();
        self.offset = 0;
        self.result = None;
        self.dirty = true;
    }

    fn size(&self) -> (u32, u32) {
        (self.length, self.font_size)
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext,
        pos: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        let (width, height) = self.size();
        if !self.dirty && !ctx.force {
            return Ok(DrawReport::empty(width, height));
        }
        self.dirty = false;

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
                let mut m = vec![];

                for desktop in self.options.iter() {
                    let d = desktop.clone();
                    if let Some(ma) = fuzzy_match(&desktop.name.to_lowercase(), &self.input.to_lowercase()) {
                        m.push((ma, d.clone(), 100));
                    }
                    for keyword in desktop.keywords.iter() {
                        if let Some(ma) = fuzzy_match(&keyword.to_lowercase(), &self.input.to_lowercase()) {
                            m.push((ma, d.clone(), 90));
                        }
                    }
                }

                m.sort_by(|(x1, y1, z1), (x2, y2, z2)| {
                    if z1 > z2 {
                        Ordering::Less
                    } else if z2 > z1 {
                        Ordering::Greater
                    } else if x1 > x2 {
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

                self.matches = m.into_iter().map(|(_, x, _)| x).collect();
            }
        };

        let buf = &mut ctx.buf.subdimensions((pos.0, pos.1, width, height))?;
        buf.memset(ctx.bg);

        match self.input.chars().next() {
            Some('=') => self.draw_calc(buf, ctx.bg),
            Some('!') => self.draw_shell(buf, ctx.bg),
            _ => self.draw_launcher(buf, ctx.bg),
        }?;

        Ok(DrawReport {
            width: width,
            height: height,
            damage: vec![buf.get_signed_bounds()],
            full_damage: false,
        })
    }

    fn keyboard_input(
        &mut self,
        key: u32,
        modifiers: ModifiersState,
        _: KeyState,
        interpreted: Option<String>,
    ) {
        match key {
            keysyms::XKB_KEY_u if modifiers.ctrl => self.leave(),
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
                                    let _ =
                                        Command::new(lexed[0].clone()).args(&lexed[1..]).spawn();
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
        }
    }
    fn mouse_click(&mut self, _: u32, _: (u32, u32)) {}
    fn mouse_scroll(&mut self, _: (f64, f64), _: (u32, u32)) {}
}
