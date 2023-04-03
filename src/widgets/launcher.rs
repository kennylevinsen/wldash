use std::{
    cmp::{max, min, Ordering},
    collections::HashMap,
    default::Default,
    fs::{read_to_string, File},
    io::Write,
    process::{exit, Command},
    sync::{Arc, Mutex},
    thread,
};

use crate::{
    buffer::BufferView,
    color::Color,
    event::{Event, Events, PointerButton, PointerEvent},
    fonts::FontMap,
    keyboard::{keysyms, KeyEvent},
    utils::{
        desktop::{load_desktop_files, load_desktop_cache, write_desktop_cache, Desktop},
        xdg,
    },
    widgets::{Geometry, Widget},
};

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use unicode_segmentation::UnicodeSegmentation;
use wayland_client::{protocol::wl_keyboard, WEnum};

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

    fn set(&mut self, v: &str) {
        self.input = v.to_string();
        self.cursor = self.input.len();
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
    fn trigger(&mut self, intf: &mut InnerInterface);
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
    fn new(events: Arc<Mutex<Events>>) -> Launcher {
        let options = Arc::new(Mutex::new(Vec::new()));
        {
            let options = Arc::clone(&options);
            thread::Builder::new()
                .name("desktopini".to_string())
                .spawn(move || {
                    let mut options = options.lock().unwrap();
                    *options = match load_desktop_cache() {
                        Ok(v) => v,
                        Err(_) => {
                            let v = load_desktop_files();
                            write_desktop_cache(&v).unwrap();
                            v
                        }
                    };
                    drop(options);
                    let mut events = events.lock().unwrap();
                    events.add_event(Event::LauncherUpdate);
                })
                .unwrap();
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
    fn trigger(&mut self, intf: &mut InnerInterface) {
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
        let fg = Color::WHITE;

        let line_height = intf.size.ceil() as u32;

        // Draw line
        let mut prompt_offset = intf.geometry.height - line_height;
        let mut prompt_line = view.offset((0, prompt_offset));

        let font = fonts.get_font(intf.font, intf.size);
        let mut x_max = font
            .auto_draw_text_with_cursor(
                &mut prompt_line,
                fg,
                &format!("   {}", &intf.prompt.input),
                intf.prompt.cursor + 3,
            )
            .unwrap();

        font.auto_draw_text(&mut prompt_line, fg, ">").unwrap();

        // Draw entries
        let dimfg = Color::GREY50;
        prompt_offset -= 8;

        for (idx, m) in self.matches.iter().enumerate() {
            if prompt_offset < line_height {
                break;
            }
            prompt_offset -= line_height;
            let mut line = view.offset((0, prompt_offset));

            if idx == intf.selection {
                let fuzzy_matcher = SkimMatcherV2::default();
                let (_, indices) = fuzzy_matcher
                    .fuzzy_indices(&m.name.to_lowercase(), &intf.prompt.input.to_lowercase())
                    .unwrap_or((0, vec![]));

                let mut colors = Vec::with_capacity(m.name.len());
                for pos in 0..m.name.len() {
                    if indices.contains(&pos) {
                        colors.push(Color::LIGHTORANGE);
                    } else {
                        colors.push(Color::GREY75);
                    }
                }
                x_max = max(
                    x_max,
                    font.auto_draw_text_individual_colors(&mut line, &colors, &m.name)
                        .unwrap(),
                );
            } else {
                x_max = max(
                    x_max,
                    font.auto_draw_text(&mut line, dimfg, &m.name).unwrap(),
                );
            }
        }

        let content_height = min(
            intf.geometry.height,
            (self.matches.len() + 1) as u32 * line_height + 8,
        );
        let content_width = min(intf.geometry.width, x_max.0 + 1);
        Geometry {
            x: intf.geometry.x,
            y: intf.geometry.y + intf.geometry.height - content_height,
            width: content_width,
            height: content_height,
        }
    }
}

struct Shell {
    next_token: Option<String>,
}


impl Shell {
    fn new() -> Shell {
        Shell {
            next_token: None,
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

impl InterfaceWidget for Shell {
    fn trigger(&mut self, intf: &mut InnerInterface) {
        let args = vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            intf.prompt.input.clone(),
        ];
        self.exec(args);
    }

    fn update(&mut self, _intf: &InnerInterface) {}

    fn draw(
        &mut self,
        intf: &InnerInterface,
        fonts: &mut FontMap,
        view: &mut BufferView,
    ) -> Geometry {
        let fg = Color::WHITE;

        let line_height = intf.size.ceil() as u32;

        // Draw line
        let prompt_offset = intf.geometry.height - line_height;
        let mut prompt_line = view.offset((0, prompt_offset));

        // Draw prompt
        let font = fonts.get_font(intf.font, intf.size);
        let x_max = font
            .auto_draw_text_with_cursor(
                &mut prompt_line,
                fg,
                &format!("   {}", &intf.prompt.input),
                intf.prompt.cursor + 3,
            )
            .unwrap();
        font.auto_draw_text(&mut prompt_line, Color::BUFF, "!")
            .unwrap();

        Geometry {
            x: intf.geometry.x,
            y: intf.geometry.y + intf.geometry.height - line_height,
            width: x_max.0 + 1,
            height: line_height,
        }
    }
}

struct Calc {
    old: Arc<Mutex<Vec<(String, String)>>>,
    result: Option<String>,
}

impl Calc {
    fn new() -> Calc {
        let old: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let old_local = old.clone();
        thread::Builder::new()
            .name("calc".to_string())
            .spawn(move || {
                let s = match read_to_string(format!("{}/wldash/calc", xdg::cache_folder())) {
                    Ok(s) => s,
                    _ => return,
                };

                let mut old = old_local.lock().unwrap();
                for line in s.lines() {
                    let pos = match line.find("=") {
                        Some(v) => v,
                        _ => continue,
                    };
                    let (input, res) = line.split_at(pos);
                    let (input, res) = (input.trim(), res[1..].trim());
                    old.push((input.to_string(), res.to_string()));
                }
            })
            .unwrap();

        Calc { old, result: None }
    }

    fn sync(&self) {
        let mut f = match File::create(format!("{}/wldash/calc", xdg::cache_folder())) {
            Ok(f) => f,
            _ => return,
        };

        let old = self.old.lock().unwrap();
        for (input, res) in old.iter() {
            write!(f, "{}={}\n", input, res).unwrap();
        }
        f.sync_data().unwrap();
    }
}

impl InterfaceWidget for Calc {
    fn trigger(&mut self, intf: &mut InnerInterface) {
        let mut old = self.old.lock().unwrap();
        if intf.selection == 0 {
            if let Some(res) = &self.result {
                old.push((intf.prompt.input.to_string(), res.to_string()));
                while old.len() > 32 {
                    old.remove(0);
                }
                drop(old);
                self.sync();
            }
        } else if old.len() >= intf.selection {
            let (input, res) = &old[old.len() - intf.selection];
            self.result = Some(res.to_string());
            intf.prompt.set(input);
            intf.selection = 0;
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
        let old = self.old.lock().unwrap();
        let fg = Color::WHITE;

        let line_height = intf.size.ceil() as u32;

        // Draw line
        let mut prompt_offset = intf.geometry.height - (line_height);
        let mut prompt_line = view.offset((0, prompt_offset));

        // Draw prompt
        let font = fonts.get_font(intf.font, intf.size);
        font.auto_draw_text_with_cursor(
            &mut prompt_line,
            fg,
            &format!("   {} ", &intf.prompt.input),
            intf.prompt.cursor + 3,
        )
        .unwrap();

        font.auto_draw_text(&mut prompt_line, Color::BUFF, "=")
            .unwrap();

        prompt_offset -= 8;
        prompt_offset -= line_height;
        if let Some(res) = &self.result {
            let c = if intf.selection == 0 {
                Color::LIGHTORANGE
            } else {
                Color::GREY50
            };
            let mut result_line = view.offset((0, prompt_offset));
            font.auto_draw_text(&mut result_line, c, res).unwrap();
        }

        for (idx, (input, res)) in old.iter().rev().enumerate() {
            if prompt_offset < line_height {
                break;
            }
            let c = if idx + 1 == intf.selection {
                Color::GREY75
            } else {
                Color::GREY50
            };
            prompt_offset -= line_height;
            let mut result_line = view.offset((0, prompt_offset));
            font.auto_draw_text(&mut result_line, c, &format!("{} = {}", input, res))
                .unwrap();
        }

        let content_height = min(
            intf.geometry.height,
            (old.len() + 2) as u32 * line_height + 8,
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
    font: &'static str,
    size: f32,
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
    pub fn new(
        events: Arc<Mutex<Events>>,
        fm: &mut FontMap,
        font: &'static str,
        size: f32,
    ) -> Interface {
        fm.queue_font(
            font,
            size,
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 >=!,.-/()",
        );

        Interface {
            launcher: Launcher::new(events),
            shell: Shell::new(),
            calc: Calc::new(),
            inner: InnerInterface {
                font,
                size,
                dirty: false,
                geometry: Default::default(),
                selection: 0,
                prompt: Prompt::new(),
            },
        }
    }

    fn pointer_input(&mut self, event: &PointerEvent) {
        if let PointerButton::Left = event.button {
            let line_height = self.inner.size.ceil() as u32;
            let height = self.inner.geometry.height - line_height - 8;
            let lines = height / line_height;

            let offset = height % line_height;
            let pos = if event.pos.1 >= offset {
                event.pos.1 - offset
            } else {
                return;
            };
            self.inner.selection = (lines - pos / line_height - 1) as usize;
            self.inner.dirty = true;
        }
    }

    fn exit(&mut self) {
        if self.inner.prompt.input.len() == 0 {
            std::process::exit(0);
        }

        if let Ok(mut f) = File::create(format!("{}/wldash/prompt", xdg::cache_folder())) {
            write!(f, "{}", self.inner.prompt.input).unwrap();
            f.sync_data().unwrap();
        }
        std::process::exit(0);
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
            keysyms::XKB_KEY_r if event.modifiers.ctrl => {
                if let Ok(s) = read_to_string(format!("{}/wldash/prompt", xdg::cache_folder())) {
                    self.inner.prompt.set(&s);
                }
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
            keysyms::XKB_KEY_Escape => self.exit(),
            keysyms::XKB_KEY_Return => widget.trigger(&mut self.inner),
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
}

impl Widget for Interface {
    fn get_dirty(&self) -> bool {
        self.inner.dirty
    }

    fn geometry(&self) -> Geometry {
        self.inner.geometry
    }

    fn draw(&mut self, fonts: &mut FontMap, view: &mut BufferView) -> Geometry {
        self.inner.dirty = false;
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

    fn minimum_size(&mut self, _fonts: &mut FontMap) -> Geometry {
        Geometry {
            x: 0,
            y: 0,
            width: 256,
            height: (self.inner.size.ceil() as u32) * 4 + 8,
        }
    }

    fn event(&mut self, event: &Event) {
        match event {
            Event::KeyEvent(e) => self.keyboard_input(e),
            Event::PointerEvent(e) => self.pointer_input(e),
            Event::FocusLost => self.exit(),
            Event::LauncherUpdate => {
                self.inner.dirty = true;
                self.launcher.update(&self.inner);
            }
            Event::TokenUpdate(t) => {
                self.launcher.next_token = Some(t.to_string());
                self.shell.next_token = Some(t.to_string());
            }
            _ => (),
        }
    }
}
