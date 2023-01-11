use std::cmp::{min, Ordering};
use std::collections::HashMap;
use std::default::Default;
use std::process::{exit, Command};

use crate::{
    buffer::BufferView,
    color::Color,
    fonts::FontMap,
    keyboard::{keysyms, KeyEvent},
    utils::desktop::{load_desktop_files, Desktop},
    widgets::{Geometry, Widget},
};

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use unicode_segmentation::UnicodeSegmentation;
use wayland_client::{protocol::wl_keyboard, WEnum};

use std::sync::{Arc, Mutex};
use std::thread;

const LAUNCHER_FONT: &str = "monospace";
const LAUNCHER_SIZE: f32 = 32.;

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
            } else {
                d1.cmp(d2)
            }
        });

        m.into_iter().map(|(_, x)| x).collect()
    }
}

enum PromptMode {
    Normal,
    Shell,
    Calc,
}

struct Prompt {
    input: String,
    mode: PromptMode,
    cursor: usize,
}

impl Prompt {
    fn new() -> Prompt {
        Prompt {
            input: String::new(),
            mode: PromptMode::Normal,
            cursor: 0,
        }
    }

    fn move_cursor(&mut self, distance: isize) {
        let new_cursor = self.cursor as isize + distance;
        if new_cursor < 0 {
            self.cursor = 0;
            return;
        } else if new_cursor > self.input.len() as isize {
            self.cursor = self.input.len()
        } else {
            self.cursor = new_cursor as usize;
        }
    }

    fn append(&mut self, v: &str) {
        if self.input.len() == 0 {
            match v {
                "!" => {
                    self.mode = PromptMode::Shell;
                    return;
                }
                "=" => {
                    self.mode = PromptMode::Calc;
                    return;
                }
                _ => (),
            }
        }
        let indices: Vec<(usize, &str)> = self.input.grapheme_indices(true).collect();
        if self.cursor == indices.len() {
            self.input += &v;
        } else {
            let index_at = indices[self.cursor].0;
            self.input.insert_str(index_at, &v);
        }
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        let mut indices: Vec<(usize, &str)> = self.input.grapheme_indices(true).collect();
        if indices.is_empty() {
            self.mode = PromptMode::Normal;
            return;
        }
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        indices.remove(self.cursor);
        self.input = indices.iter().fold("".into(), |acc, el| acc + el.1);
    }

    fn delete(&mut self) {
        let mut indices: Vec<(usize, &str)> = self.input.grapheme_indices(true).collect();
        if indices.is_empty() {
            self.mode = PromptMode::Normal;
            return;
        }
        if self.cursor >= indices.len() {
            return;
        }

        indices.remove(self.cursor);
        self.input = indices.iter().fold("".into(), |acc, el| acc + el.1);
    }

    fn clear_right(&mut self) {
        let indices: Vec<(usize, &str)> = self.input.grapheme_indices(true).collect();
        if !indices.is_empty() {
            self.input = indices[0..self.cursor]
                .iter()
                .fold("".into(), |acc, el| acc + el.1);
        } else {
            self.mode = PromptMode::Normal;
        }
    }

    fn clear_left(&mut self) {
        let indices: Vec<(usize, &str)> = self.input.grapheme_indices(true).collect();
        if !indices.is_empty() {
            self.input = indices[self.cursor..indices.len()]
                .iter()
                .fold("".into(), |acc, el| acc + el.1);
            self.cursor = 0;
        } else {
            self.mode = PromptMode::Normal;
        }
    }

    fn home(&mut self) {
        self.cursor = 0;
    }

    fn end(&mut self) {
        self.cursor = self.input.len();
    }
}

trait InterfaceWidget {
    fn trigger(&mut self, intf: &InnerInterface);
    fn draw(
        &mut self,
        intf: &InnerInterface,
        fonts: &mut FontMap,
        view: &mut BufferView,
    ) -> Geometry;
    fn update(&mut self, intf: &InnerInterface);
}

struct Launcher {
    next_token: Option<String>,
    options: Arc<Mutex<Vec<Desktop>>>,
    matches: Vec<Desktop>,
}

impl Launcher {
    fn new() -> Launcher {
        let options = Arc::new(Mutex::new(Vec::new()));
        {
            let options = Arc::clone(&options);
            thread::spawn(move || {
                let mut options = options.lock().unwrap();
                *options = load_desktop_files();
            });
        }

        Launcher {
            next_token: None,
            options: options,
            matches: Vec::new(),
        }
    }

    fn exec(&self, args: Vec<String>) {
        let mut cmd = Command::new(args[0].clone());
        if let Some(token) = &self.next_token {
            cmd.env("XDG_ACTIVATION_TOKEN", token);
        }
        cmd.args(&args[1..]).spawn().unwrap();
        exit(0);
    }
}

impl InterfaceWidget for Launcher {
    fn trigger(&mut self, intf: &InnerInterface) {
        if self.matches.len() > intf.selection {
            let d = &self.matches[intf.selection];
            if let Some(exec) = &d.exec {
                let exec = exec
                    .replace("%f", "")
                    .replace("%F", "")
                    .replace("%u", "")
                    .replace("%U", "");

                let lexed = shlex::split(&exec).unwrap();
                if !lexed.is_empty() {
                    self.exec(lexed);
                }
            }
        }
    }

    fn update(&mut self, intf: &InnerInterface) {
        let mut matcher = Matcher::new();
        let options = self.options.lock().unwrap();
        for desktop in options.iter() {
            matcher.try_match(
                desktop.clone(),
                &desktop.name.to_lowercase(),
                &intf.prompt.input.to_lowercase(),
                1.0,
            );
            for keyword in desktop.keywords.iter() {
                matcher.try_match(
                    desktop.clone(),
                    &keyword.to_lowercase(),
                    &intf.prompt.input.to_lowercase(),
                    0.5,
                );
            }
        }

        self.matches = matcher.matches();
    }

    fn draw(
        &mut self,
        intf: &InnerInterface,
        fonts: &mut FontMap,
        view: &mut BufferView,
    ) -> Geometry {
        let fg = Color::new(1., 1., 1., 1.);
        let bg = Color::new(0., 0., 0., 1.);

        let line_height = LAUNCHER_SIZE.ceil() as u32;

        // Draw line
        let mut prompt_offset = intf.geometry.height - line_height;
        let mut prompt_line = view.offset((0, prompt_offset)).unwrap();
        let mut prompt_border = prompt_line.limit((intf.geometry.width, 1)).unwrap();
        prompt_border.memset(&fg);

        // Draw prompt
        let font = fonts.get_font(LAUNCHER_FONT, LAUNCHER_SIZE);
        font.auto_draw_text_with_cursor(
            &mut prompt_line,
            &bg,
            &fg,
            &format!("   {}", &intf.prompt.input),
            intf.prompt.cursor + 3,
        )
        .unwrap();

        let font = fonts.get_font(LAUNCHER_FONT, LAUNCHER_SIZE);
        font.auto_draw_text(&mut prompt_line, &bg, &fg, ">")
            .unwrap();

        // Draw entries
        let dimfg = Color::new(0.5, 0.5, 0.5, 1.0);
        prompt_offset -= 16;

        for (idx, m) in self.matches.iter().enumerate() {
            if prompt_offset < line_height {
                break;
            }
            prompt_offset -= line_height;
            let mut line = view.offset((0, prompt_offset)).unwrap();

            if idx == intf.selection {
                let fuzzy_matcher = SkimMatcherV2::default();
                let (_, indices) = fuzzy_matcher
                    .fuzzy_indices(&m.name.to_lowercase(), &intf.prompt.input.to_lowercase())
                    .unwrap_or((0, vec![]));

                let mut colors = Vec::with_capacity(m.name.len());
                for pos in 0..m.name.len() {
                    if indices.contains(&pos) {
                        colors.push(Color::new(1.0, 0.65, 0., 1.0));
                    } else {
                        colors.push(Color::new(0.75, 0.75, 0.75, 1.0));
                    }
                }
                font.auto_draw_text_individual_colors(&mut line, &bg, &colors, &m.name)
                    .unwrap();
            } else {
                font.auto_draw_text(&mut line, &bg, &dimfg, &m.name)
                    .unwrap();
            }
        }

        let content_height = min(
            intf.geometry.height,
            (self.matches.len() + 1) as u32 * line_height + 16,
        );
        Geometry {
            x: intf.geometry.x,
            y: intf.geometry.y + intf.geometry.height - content_height,
            width: intf.geometry.width,
            height: content_height,
        }
    }
}

struct Shell();

impl InterfaceWidget for Shell {
    fn trigger(&mut self, intf: &InnerInterface) {
        let args = vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            intf.prompt.input.clone(),
        ];
        Command::new(args[0].clone())
            .args(&args[1..])
            .spawn()
            .unwrap();
        exit(0);
    }

    fn update(&mut self, _intf: &InnerInterface) {}

    fn draw(
        &mut self,
        intf: &InnerInterface,
        fonts: &mut FontMap,
        view: &mut BufferView,
    ) -> Geometry {
        let fg = Color::new(1., 1., 1., 1.);
        let bg = Color::new(0., 0., 0., 1.);

        let line_height = LAUNCHER_SIZE.ceil() as u32;

        // Draw line
        let prompt_offset = intf.geometry.height - line_height;
        let mut prompt_line = view.offset((0, prompt_offset)).unwrap();
        let mut prompt_border = prompt_line.limit((intf.geometry.width, 1)).unwrap();
        prompt_border.memset(&fg);

        // Draw prompt
        let font = fonts.get_font(LAUNCHER_FONT, LAUNCHER_SIZE);
        font.auto_draw_text_with_cursor(
            &mut prompt_line,
            &bg,
            &fg,
            &format!("   {}", &intf.prompt.input),
            intf.prompt.cursor + 3,
        )
        .unwrap();
        let font = fonts.get_font(LAUNCHER_FONT, LAUNCHER_SIZE);
        font.auto_draw_text(&mut prompt_line, &bg, &Color::new(1., 0.75, 0.5, 1.), "!")
            .unwrap();

        Geometry {
            x: intf.geometry.x,
            y: intf.geometry.y + intf.geometry.height - line_height,
            width: intf.geometry.width,
            height: line_height,
        }
    }
}

struct Calc {
    old: Vec<String>,
    result: Option<String>,
}

impl InterfaceWidget for Calc {
    fn trigger(&mut self, intf: &InnerInterface) {
        if let Some(res) = &self.result {
            self.old.push(format!("{} = {}", &intf.prompt.input, res));
        }
    }

    fn update(&mut self, intf: &InnerInterface) {
        let res =
            rcalc_lib::parse::eval(&intf.prompt.input, &mut rcalc_lib::parse::CalcState::new())
                .map(|x| format!("{}", x))
                .map_err(|x| format!("{}", x));
        self.result = match res {
            Ok(v) => Some(v),
            Err(_) => None,
        };
    }

    fn draw(
        &mut self,
        intf: &InnerInterface,
        fonts: &mut FontMap,
        view: &mut BufferView,
    ) -> Geometry {
        let fg = Color::new(1., 1., 1., 1.);
        let bg = Color::new(0., 0., 0., 1.);

        let line_height = LAUNCHER_SIZE.ceil() as u32;

        // Draw line
        let mut prompt_offset = intf.geometry.height - (LAUNCHER_SIZE.ceil() as u32);
        let mut prompt_line = view.offset((0, prompt_offset)).unwrap();
        let mut prompt_border = prompt_line.limit((intf.geometry.width, 1)).unwrap();
        prompt_border.memset(&fg);

        // Draw prompt
        let font = fonts.get_font(LAUNCHER_FONT, LAUNCHER_SIZE);
        let res_off = font
            .auto_draw_text_with_cursor(
                &mut prompt_line,
                &bg,
                &fg,
                &format!("   {} ", &intf.prompt.input),
                intf.prompt.cursor + 3,
            )
            .unwrap();

        let font = fonts.get_font(LAUNCHER_FONT, LAUNCHER_SIZE);
        font.auto_draw_text(&mut prompt_line, &bg, &Color::new(1., 0.75, 0.5, 1.), "=")
            .unwrap();

        if let Some(res) = &self.result {
            let resfg = Color::new(1.0, 0.65, 0., 1.0);
            let mut result_line = view.offset((res_off.0, prompt_offset)).unwrap();
            font.auto_draw_text(&mut result_line, &bg, &resfg, &format!(" = {}", &res))
                .unwrap();
        }

        for m in self.old.iter().rev() {
            let dimfg = Color::new(0.5, 0.5, 0.5, 1.0);
            if prompt_offset < LAUNCHER_SIZE.ceil() as u32 {
                break;
            }
            prompt_offset -= LAUNCHER_SIZE.ceil() as u32;
            let mut result_line = view.offset((0, prompt_offset)).unwrap();
            font.auto_draw_text(&mut result_line, &bg, &dimfg, &m)
                .unwrap();
        }

        let content_height = min(
            intf.geometry.height,
            (self.old.len() + 1) as u32 * line_height + 16,
        );
        Geometry {
            x: intf.geometry.x,
            y: intf.geometry.y + intf.geometry.height - content_height,
            width: intf.geometry.width,
            height: content_height,
        }
    }
}

struct InnerInterface {
    dirty: bool,
    geometry: Geometry,
    selection: usize,
    prompt: Prompt,
}

pub struct Interface {
    launcher: Launcher,
    shell: Shell,
    calc: Calc,
    inner: InnerInterface,
}

impl Interface {
    pub fn new() -> Interface {
        Interface {
            launcher: Launcher::new(),
            shell: Shell {},
            calc: Calc {
                old: Vec::new(),
                result: None,
            },
            inner: InnerInterface {
                dirty: false,
                geometry: Default::default(),
                selection: 0,
                prompt: Prompt::new(),
            },
        }
    }
}

impl Widget for Interface {
    fn set_dirty(&mut self, dirty: bool) {
        self.inner.dirty = dirty;
    }
    fn get_dirty(&self) -> bool {
        self.inner.dirty
    }

    fn geometry(&self) -> Geometry {
        self.inner.geometry
    }

    fn draw(&mut self, fonts: &mut FontMap, view: &mut BufferView) -> Geometry {
        match self.inner.prompt.mode {
            PromptMode::Shell => self.shell.draw(&self.inner, fonts, view),
            PromptMode::Normal => self.launcher.draw(&self.inner, fonts, view),
            PromptMode::Calc => self.calc.draw(&self.inner, fonts, view),
        }
    }

    fn geometry_update(&mut self, _fonts: &mut FontMap, geometry: &Geometry) -> Geometry {
        self.inner.geometry = geometry.clone();
        self.inner.geometry
    }

    fn keyboard_input(&mut self, event: &KeyEvent) {
        if event.state != WEnum::Value(wl_keyboard::KeyState::Pressed) {
            return;
        }
        self.inner.dirty = true;
        let widget = match self.inner.prompt.mode {
            PromptMode::Shell => &mut self.shell as &mut dyn InterfaceWidget,
            PromptMode::Normal => &mut self.launcher as &mut dyn InterfaceWidget,
            PromptMode::Calc => &mut self.calc as &mut dyn InterfaceWidget,
        };
        match event.keysym {
            keysyms::XKB_KEY_a if event.modifiers.ctrl => self.inner.prompt.home(),
            keysyms::XKB_KEY_e if event.modifiers.ctrl => self.inner.prompt.end(),
            keysyms::XKB_KEY_u if event.modifiers.ctrl => {
                self.inner.prompt.clear_left();
                self.inner.selection = 0;
                widget.update(&self.inner);
            }
            keysyms::XKB_KEY_k if event.modifiers.ctrl => {
                self.inner.prompt.clear_right();
                self.inner.selection = 0;
                widget.update(&self.inner);
            }
            keysyms::XKB_KEY_Home => self.inner.prompt.home(),
            keysyms::XKB_KEY_End => self.inner.prompt.end(),
            keysyms::XKB_KEY_BackSpace => {
                self.inner.prompt.backspace();
                self.inner.selection = 0;
                widget.update(&self.inner);
            }
            keysyms::XKB_KEY_Delete => {
                self.inner.prompt.delete();
                self.inner.selection = 0;
                widget.update(&self.inner);
            }
            keysyms::XKB_KEY_Return => widget.trigger(&self.inner),
            keysyms::XKB_KEY_Left => self.inner.prompt.move_cursor(-1),
            keysyms::XKB_KEY_Right => self.inner.prompt.move_cursor(1),
            keysyms::XKB_KEY_Up => {
                self.inner.selection += 1;
            }
            keysyms::XKB_KEY_Down => {
                if self.inner.selection > 0 {
                    self.inner.selection -= 1;
                }
            }
            _ => {
                if let Some(utf8) = &event.utf8 {
                    self.inner.prompt.append(&utf8);
                    self.inner.selection = 0;
                    widget.update(&self.inner);
                } else {
                    self.inner.dirty = false;
                }
            }
        }
    }

    fn token_update(&mut self, token: &str) {
        self.launcher.next_token = Some(token.to_string());
    }
}
