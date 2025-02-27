//! Utilities for keymap interpretation of keyboard input
//!
//! This module provides an implementation for `wl_keyboard`
//! objects using `libxkbcommon` to interpret the keyboard input
//! given the user keymap.
//!
//! The entry point of this module is the [`map_keyboard`](fn.map_keyboard.html)
//! function which, given a `wl_seat` and a callback, setup keymap interpretation
//! and key repetition for the `wl_keyboard` of this seat.
//!
//! Key repetition relies on an event source, that needs to be inserted in your
//! calloop event loop. Not doing so will prevent key repetition to work
//! (but the rest of the functionnality will not be affected).

use std::{
    default::Default,
    os::unix::io::{AsRawFd, OwnedFd},
};

use wayland_client::{protocol::wl_keyboard, WEnum};
use xkbcommon::xkb;

pub use xkbcommon::xkb::keysyms;

mod repeat;
mod state;

pub use self::repeat::{KeyRepeatSource, RepeatMessage};
use self::state::KbState;
pub use self::state::ModifiersState;

#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub rawkey: u32,
    pub keysym: u32,
    pub state: WEnum<wl_keyboard::KeyState>,
    pub modifiers: ModifiersState,
    pub utf8: Option<String>,
    pub repeats: bool,
}

pub struct KeymapDescription {
    fd: OwnedFd,
    size: u32,
}

impl KeymapDescription {
    fn new(fd: OwnedFd, size: u32) -> KeymapDescription {
        KeymapDescription { fd: fd, size: size }
    }
}

impl Clone for KeymapDescription {
    fn clone(&self) -> Self {
        KeymapDescription::new(self.fd.try_clone().expect("unable to clone fd"), self.size)
    }
}

pub struct Keyboard {
    state: Option<KbState>,
    desc: Option<KeymapDescription>,
}

impl Keyboard {
    pub fn new() -> Keyboard {
        Keyboard {
            state: None,
            desc: None,
        }
    }

    pub fn keymap(&mut self, format: WEnum<wl_keyboard::KeymapFormat>, fd: OwnedFd, size: u32) {
        match format {
            WEnum::Value(wl_keyboard::KeymapFormat::XkbV1) => {
                self.desc = Some(KeymapDescription::new(fd, size));
            }
            WEnum::Value(wl_keyboard::KeymapFormat::NoKeymap) => {
                // TODO: how to handle this (hopefully never occuring) case?
            }
            _ => unreachable!(),
        }
    }

    pub fn resolve(&mut self) {
        if self.state.is_some() {
            return;
        }

        if let Some(desc) = self.desc.take() {
            self.state = Some(
                KbState::new_from_fd(desc.fd, desc.size as usize).expect("unable to load keymap"),
            );
        }
    }

    pub fn key(&mut self, key: u32, key_state: WEnum<wl_keyboard::KeyState>) -> KeyEvent {
        if let Some(state) = &mut self.state {
            let (sym, utf8, repeats) = {
                // Get the values to generate a key event
                let sym = state.get_one_sym_raw(key);
                let utf8 = if key_state == WEnum::Value(wl_keyboard::KeyState::Pressed) {
                    match state.compose_feed(sym) {
                        Some(xkb::compose::FeedResult::Accepted) => {
                            if let Some(status) = state.compose_status() {
                                match status {
                                    xkb::compose::Status::Composed => state.compose_get_utf8(),
                                    xkb::compose::Status::Nothing => state.get_utf8_raw(key),
                                    _ => None,
                                }
                            } else {
                                state.get_utf8_raw(key)
                            }
                        }
                        Some(_) => {
                            // XKB_COMPOSE_FEED_IGNORED
                            None
                        }
                        None => {
                            // XKB COMPOSE is not initialized
                            state.get_utf8_raw(key)
                        }
                    }
                } else {
                    None
                };
                let repeats = state.key_repeats(xkb::Keycode::new(key + 8));
                (sym, utf8, repeats)
            };
            KeyEvent {
                rawkey: key,
                keysym: sym,
                state: key_state,
                modifiers: state.mods_state(),
                utf8,
                repeats,
            }
        } else {
            KeyEvent {
                rawkey: key,
                keysym: 0,
                state: key_state,
                modifiers: Default::default(),
                utf8: None,
                repeats: false,
            }
        }
    }

    pub fn modifiers(
        &mut self,
        mods_depressed: u32,
        mods_latched: u32,
        mods_locked: u32,
        group: u32,
    ) {
        if let Some(state) = &mut self.state {
            state.update_modifiers(mods_depressed, mods_latched, mods_locked, group);
        }
    }
}
