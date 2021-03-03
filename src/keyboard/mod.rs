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

use std::{cell::RefCell, os::unix::io::RawFd, rc::Rc};

use byteorder::{ByteOrder, NativeEndian};

pub use wayland_client::protocol::wl_keyboard::KeyState;
use wayland_client::{
    protocol::{wl_keyboard, wl_seat, wl_surface},
    Attached,
};

mod ffi;
pub mod keysyms;
mod state;

use self::state::KbState;
pub use self::state::{ModifiersState, RMLVO};

#[derive(Debug)]
/// An error that occurred while trying to initialize a mapped keyboard
pub enum Error {
    /// libxkbcommon is not available
    XKBNotFound,
    /// Provided RMLVO specified a keymap that would not be loaded
    BadNames,
}

/// Events received from a mapped keyboard
pub enum Event<'a> {
    /// The keyboard focus has entered a surface
    Enter {
        /// serial number of the event
        serial: u32,
        /// surface that was entered
        surface: wl_surface::WlSurface,
        /// raw values of the currently pressed keys
        rawkeys: &'a [u32],
        /// interpreted symbols of the currently pressed keys
        keysyms: &'a [u32],
    },
    /// The keyboard focus has left a surface
    Leave {
        /// serial number of the event
        serial: u32,
        /// surface that was left
        surface: wl_surface::WlSurface,
    },
    /// The key modifiers have changed state
    Modifiers {
        /// current state of the modifiers
        modifiers: ModifiersState,
    },
    /// A key event occurred
    Key {
        /// serial number of the event
        serial: u32,
        /// time at which the keypress occurred
        time: u32,
        /// raw value of the key
        rawkey: u32,
        /// interpreted symbol of the key
        keysym: u32,
        /// new state of the key
        state: KeyState,
        /// utf8 interpretation of the entered text
        ///
        /// will always be `None` on key release events
        utf8: Option<String>,
        repeats: bool,
    },
    RepeatInfo {
        rate: i32,
        delay: i32,
    },
}

/// Implement a keyboard for keymap translation with key repetition
///
/// This requires you to provide a callback to receive the events after they
/// have been interpreted with the keymap.
///
/// The keymap will be loaded from the provided RMLVO rules, or from the compositor
/// provided keymap if `None`.
///
/// Returns an error if xkbcommon could not be initialized, the RMLVO specification
/// contained invalid values, or if the provided seat does not have keyboard capability.
///
/// **Note:** This adapter does not handle key repetition. See `map_keyboard_repeat` for that.
pub fn map_keyboard<F>(
    seat: &Attached<wl_seat::WlSeat>,
    rmlvo: Option<RMLVO>,
    callback: F,
) -> Result<wl_keyboard::WlKeyboard, Error>
where
    F: FnMut(Event<'_>, wl_keyboard::WlKeyboard, wayland_client::DispatchData<'_>) + 'static,
{
    let keyboard = seat.get_keyboard();

    let state = Rc::new(RefCell::new(
        rmlvo
            .map(KbState::from_rmlvo)
            .unwrap_or_else(KbState::new)?,
    ));

    let callback = Rc::new(RefCell::new(callback));

    // prepare the handler
    let mut kbd_handler = KbdHandler { callback, state };

    keyboard.quick_assign(move |keyboard, event, data| {
        kbd_handler.event(keyboard.detach(), event, data)
    });

    Ok(keyboard.detach())
}

/*
 * Classic handling
 */

type KbdCallback = dyn FnMut(Event<'_>, wl_keyboard::WlKeyboard, wayland_client::DispatchData<'_>);

struct KbdHandler {
    state: Rc<RefCell<KbState>>,
    callback: Rc<RefCell<KbdCallback>>,
}

impl KbdHandler {
    fn event(
        &mut self,
        kbd: wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        dispatch_data: wayland_client::DispatchData,
    ) {
        use wl_keyboard::Event;

        match event {
            Event::Keymap { format, fd, size } => self.keymap(kbd, format, fd, size),
            Event::Enter {
                serial,
                surface,
                keys,
            } => self.enter(kbd, serial, surface, keys, dispatch_data),
            Event::Leave { serial, surface } => self.leave(kbd, serial, surface, dispatch_data),
            Event::Key {
                serial,
                time,
                key,
                state,
            } => self.key(kbd, serial, time, key, state, dispatch_data),
            Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => self.modifiers(
                kbd,
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                dispatch_data,
            ),
            Event::RepeatInfo { rate, delay } => self.repeat_info(kbd, rate, delay, dispatch_data),
            _ => {}
        }
    }

    fn keymap(
        &mut self,
        _: wl_keyboard::WlKeyboard,
        format: wl_keyboard::KeymapFormat,
        fd: RawFd,
        size: u32,
    ) {
        let mut state = self.state.borrow_mut();
        if state.locked() {
            // state is locked, ignore keymap updates
            return;
        }
        if state.ready() {
            // new keymap, we first deinit to free resources
            unsafe {
                state.de_init();
            }
        }
        match format {
            wl_keyboard::KeymapFormat::XkbV1 => unsafe {
                state.init_with_fd(fd, size as usize);
            },
            wl_keyboard::KeymapFormat::NoKeymap => {
                // TODO: how to handle this (hopefully never occuring) case?
            }
            _ => unreachable!(),
        }
    }

    fn enter(
        &mut self,
        object: wl_keyboard::WlKeyboard,
        serial: u32,
        surface: wl_surface::WlSurface,
        keys: Vec<u8>,
        dispatch_data: wayland_client::DispatchData,
    ) {
        let mut state = self.state.borrow_mut();
        let rawkeys = keys
            .chunks_exact(4)
            .map(NativeEndian::read_u32)
            .collect::<Vec<_>>();
        let keys: Vec<u32> = rawkeys.iter().map(|k| state.get_one_sym_raw(*k)).collect();
        (&mut *self.callback.borrow_mut())(
            Event::Enter {
                serial,
                surface,
                rawkeys: &rawkeys,
                keysyms: &keys,
            },
            object,
            dispatch_data,
        );
    }

    fn leave(
        &mut self,
        object: wl_keyboard::WlKeyboard,
        serial: u32,
        surface: wl_surface::WlSurface,
        dispatch_data: wayland_client::DispatchData,
    ) {
        (&mut *self.callback.borrow_mut())(Event::Leave { serial, surface }, object, dispatch_data);
    }

    fn key(
        &mut self,
        object: wl_keyboard::WlKeyboard,
        serial: u32,
        time: u32,
        key: u32,
        key_state: wl_keyboard::KeyState,
        dispatch_data: wayland_client::DispatchData,
    ) {
        let (sym, utf8, repeats) = {
            let mut state = self.state.borrow_mut();
            // Get the values to generate a key event
            let sym = state.get_one_sym_raw(key);
            let utf8 = if key_state == wl_keyboard::KeyState::Pressed {
                match state.compose_feed(sym) {
                    Some(ffi::xkb_compose_feed_result::XKB_COMPOSE_FEED_ACCEPTED) => {
                        if let Some(status) = state.compose_status() {
                            match status {
                                ffi::xkb_compose_status::XKB_COMPOSE_COMPOSED => {
                                    state.compose_get_utf8()
                                }
                                ffi::xkb_compose_status::XKB_COMPOSE_NOTHING => {
                                    state.get_utf8_raw(key)
                                }
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
            let repeats = unsafe { state.key_repeats(key + 8) };
            (sym, utf8, repeats)
        };

        (&mut *self.callback.borrow_mut())(
            Event::Key {
                serial,
                time,
                rawkey: key,
                keysym: sym,
                state: key_state,
                utf8,
                repeats,
            },
            object,
            dispatch_data,
        );
    }

    fn modifiers(
        &mut self,
        object: wl_keyboard::WlKeyboard,
        mods_depressed: u32,
        mods_latched: u32,
        mods_locked: u32,
        group: u32,
        dispatch_data: wayland_client::DispatchData,
    ) {
        {
            let mut state = self.state.borrow_mut();
            state.update_modifiers(mods_depressed, mods_latched, mods_locked, group);
            (&mut *self.callback.borrow_mut())(
                Event::Modifiers {
                    modifiers: state.mods_state(),
                },
                object,
                dispatch_data,
            );
        }
    }

    fn repeat_info(
        &mut self,
        object: wl_keyboard::WlKeyboard,
        rate: i32,
        delay: i32,
        dispatch_data: wayland_client::DispatchData,
    ) {
        (&mut *self.callback.borrow_mut())(
            Event::RepeatInfo { rate, delay },
            object,
            dispatch_data,
        );
    }
}
