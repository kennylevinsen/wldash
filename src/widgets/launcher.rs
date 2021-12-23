use crate::buffer::Buffer;
use crate::cmd::Cmd;
use crate::color::Color;
use crate::desktop::{load_desktop_files, Desktop};
use crate::draw::Font;
use crate::{
    fonts::FontRef,
    widget::{DrawContext, DrawReport, KeyState, ModifiersState, WaitContext, Widget},
};

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::process::Command;
use std::sync::mpsc::Sender;

use crate::keyboard::keysyms;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use unicode_segmentation::UnicodeSegmentation;

pub struct Launcher<'a> {
    cursor: usize,
    options: Vec<Desktop>,
    term_opener: String,
    app_opener: String,
    url_opener: String,
    matches: Vec<Desktop>,
    input: String,
    result: Option<String>,
    offset: usize,
    font: RefCell<Font<'a>>,
    font_size: u32,
    length: u32,
    dirty: bool,
    tx: Sender<Cmd>,
}

impl<'a> Launcher<'a> {
    pub fn new(
        font: FontRef,
        font_size: f32,
        length: u32,
        listener: Sender<Cmd>,
        app: String,
        term: String,
        url: String,
    ) -> Box<Launcher> {
        Box::new(Launcher {
            cursor: 0,
            options: load_desktop_files(),
            term_opener: term,
            app_opener: app,
            url_opener: url,
            matches: vec![],
            input: "".to_string(),
            result: None,
            offset: 0,
            font: RefCell::new(Font::new(font, font_size)),
            font_size: font_size as u32,
            length,
            dirty: true,
            tx: listener,
        })
    }

    fn draw_launcher(
        &self,
        buf: &mut Buffer,
        bg: &Color,
        width: u32,
    ) -> Result<(), ::std::io::Error> {
        let mut x_off = if !self.input.is_empty() {
            let c = if self.matches.is_empty() {
                Color::new(1.0, 0.5, 0.5, 1.0)
            } else {
                Color::new(1.0, 1.0, 1.0, 1.0)
            };

            let dim = self.font.borrow_mut().auto_draw_text_with_cursor(
                buf,
                bg,
                &c,
                &self.input,
                self.cursor,
            )?;

            dim.0 + self.font_size / 4
        } else {
            0
        };

        let mut width_remaining: i32 = (width - x_off) as i32;
        let fuzzy_matcher = SkimMatcherV2::default();
        for (idx, m) in self.matches.iter().enumerate() {
            let mut b = match buf.offset((x_off, 0)) {
                Ok(b) => b,
                Err(_) => break,
            };
            let size = if idx == self.offset && !self.input.is_empty() {
                let (_, indices) = fuzzy_matcher
                    .fuzzy_indices(&m.name.to_lowercase(), &self.input.to_lowercase())
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

        let x_off = if !self.input.is_empty() {
            let dim = self.font.borrow_mut().auto_draw_text_with_cursor(
                &mut buf.offset((x_off, 0))?,
                bg,
                &Color::new(1.0, 1.0, 1.0, 1.0),
                &self.input[1..],
                self.cursor - 1,
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

        if !self.input.is_empty() {
            self.font.borrow_mut().auto_draw_text_with_cursor(
                &mut buf.offset((x_off, 0))?,
                bg,
                &Color::new(1.0, 1.0, 1.0, 1.0),
                &self.input[1..],
                self.cursor - 1,
            )?;
        };

        Ok(())
    }
}

fn calc(s: &str) -> Result<String, String> {
    rcalc_lib::parse::eval(s, &mut rcalc_lib::parse::CalcState::new())
        .map(|x| format!("{}", x))
        .map_err(|x| format!("{}", x))
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

struct Matcher {
    matches: HashMap<Desktop, i64>,
}

impl Matcher {
    fn new() -> Self {
        Self {
            matches: HashMap::new(),
        }
    }

    fn try_match(&mut self, dtop: Desktop, val: &str, input: &str, prio: f32) {
        let fuzzy_matcher = SkimMatcherV2::default();
        if let Some(ma) = fuzzy_matcher.fuzzy_match(val, input) {
            let ma = ((ma as f32) * prio) as i64;
            if let Some(ma_old) = self.matches.get(&dtop) {
                // Skip over new matches for the same program that are worse
                // than the one we already have.
                if ma_old > &ma {
                    return;
                }
            }

            self.matches.insert(dtop, ma);
        }
    }

    fn matches(&self) -> Vec<Desktop> {
        let mut m = self
            .matches
            .iter()
            .map(|(key, ma)| (*ma, key.clone()))
            .collect::<Vec<(i64, Desktop)>>();

        m.sort_by(|(ma1, d1), (ma2, d2)| {
            if ma1 > ma2 {
                Ordering::Less
            } else if ma1 < ma2 {
                Ordering::Greater
            } else if d1.name.len() < d2.name.len() {
                Ordering::Less
            } else if d1.name.len() > d2.name.len() {
                Ordering::Greater
            } else {
                d1.cmp(d2)
            }
        });

        m.into_iter().map(|(_, x)| x).collect()
    }
}

impl<'a> Widget for Launcher<'a> {
    fn wait(&mut self, _: &mut WaitContext) {}
    fn enter(&mut self) {}
    fn leave(&mut self) {
        self.input = "".to_string();
        self.cursor = 0;
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
        expansion: (u32, u32),
    ) -> Result<DrawReport, ::std::io::Error> {
        if self.length == 0 {
            self.length = expansion.0;
        }
        let (width, height) = (self.length, self.font_size);
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
                let mut matcher = Matcher::new();

                for desktop in self.options.iter() {
                    matcher.try_match(
                        desktop.clone(),
                        &desktop.name.to_lowercase(),
                        &self.input.to_lowercase(),
                        1.0,
                    );
                    for keyword in desktop.keywords.iter() {
                        matcher.try_match(
                            desktop.clone(),
                            &keyword.to_lowercase(),
                            &self.input.to_lowercase(),
                            0.5,
                        );
                    }
                }

                self.matches = matcher.matches();
            }
        };

        let buf = &mut ctx.buf.subdimensions((pos.0, pos.1, width, height))?;
        buf.memset(ctx.bg);

        match self.input.chars().next() {
            Some('=') => self.draw_calc(buf, ctx.bg),
            Some('!') => self.draw_shell(buf, ctx.bg),
            _ => self.draw_launcher(buf, ctx.bg, width),
        }?;

        Ok(DrawReport {
            width,
            height,
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
            keysyms::XKB_KEY_a if modifiers.ctrl => {
                self.cursor = 0;
                self.dirty = true;
            }
            keysyms::XKB_KEY_e if modifiers.ctrl => {
                self.cursor = self.input.len();
                self.dirty = true;
            }
            keysyms::XKB_KEY_Home => {
                self.cursor = 0;
                self.dirty = true;
            }
            keysyms::XKB_KEY_End => {
                self.cursor = self.input.len();
                self.dirty = true;
            }
            keysyms::XKB_KEY_BackSpace => {
                let mut indices: Vec<(usize, &str)> = self.input.grapheme_indices(true).collect();
                if !indices.is_empty() && self.cursor > 0 {
                    self.cursor -= 1;
                    indices.remove(self.cursor);
                    self.input = indices.iter().fold("".into(), |acc, el| acc + el.1);
                    self.offset = 0;
                    self.result = None;
                    self.dirty = true
                }
            }
            keysyms::XKB_KEY_Delete => {
                let mut indices: Vec<(usize, &str)> = self.input.grapheme_indices(true).collect();
                if !indices.is_empty() && self.cursor < indices.len() {
                    indices.remove(self.cursor);
                    self.input = indices.iter().fold("".into(), |acc, el| acc + el.1);
                    self.dirty = true;
                }
            }
            keysyms::XKB_KEY_Return => {
                match self.input.chars().next() {
                    Some('=') => {
                        if let Some(ref v) = self.result {
                            let _ = wlcopy(&v);
                        }
                    }
                    Some('!') => {
                        self.cursor = 0;
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

                                let mut lexed = shlex::split(&exec).unwrap();
                                let lexed = if !prefix.is_empty() {
                                    let mut prefix = shlex::split(prefix).unwrap();
                                    prefix.append(&mut lexed);
                                    prefix
                                } else {
                                    lexed
                                };
                                if !lexed.is_empty() {
                                    let _ =
                                        Command::new(lexed[0].clone()).args(&lexed[1..]).spawn();
                                    self.tx.send(Cmd::Exit).unwrap();
                                }
                            }
                            if let Some(url) = &d.url {
                                if !self.url_opener.is_empty() {
                                    let mut lexed = shlex::split(&self.url_opener).unwrap();
                                    lexed.push(url.to_string());
                                    if !lexed.is_empty() {
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
            keysyms::XKB_KEY_Tab => {
                if !self.matches.is_empty() && self.offset < self.matches.len() - 1 {
                    self.offset += 1;
                    self.dirty = true;
                }
            }
            keysyms::XKB_KEY_ISO_Left_Tab => {
                if !self.matches.is_empty() && self.offset > 0 {
                    self.offset -= 1;
                    self.dirty = true;
                }
            }
            keysyms::XKB_KEY_Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.dirty = true;
                }
            }
            keysyms::XKB_KEY_Right => {
                if self.cursor < self.input.len() {
                    self.cursor += 1;
                    self.dirty = true;
                }
            }
            _ => {
                if let Some(v) = interpreted {
                    let indices: Vec<(usize, &str)> = self.input.grapheme_indices(true).collect();
                    if self.cursor == indices.len() {
                        self.input += &v;
                    } else {
                        let index_at = indices[self.cursor].0;
                        self.input.insert_str(index_at, &v);
                    }
                    self.cursor += 1;
                    self.offset = 0;
                    self.result = None;
                    self.dirty = true;
                }
            }
        }
    }
    fn mouse_click(&mut self, _: u32, _: (u32, u32)) {}
    fn mouse_scroll(&mut self, _: (f64, f64), _: (u32, u32)) {}
}
