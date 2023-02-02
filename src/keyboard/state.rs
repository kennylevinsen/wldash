use std::{env, os::unix::io::RawFd};

use xkbcommon::xkb;

pub struct KbState {
    xkb_context: xkb::Context,
    xkb_keymap: xkb::Keymap,
    xkb_state: xkb::State,
    xkb_compose_table: Option<xkb::compose::Table>,
    xkb_compose_state: Option<xkb::compose::State>,
    mods_state: ModifiersState,
}

/// Represents the current state of the keyboard modifiers
///
/// Each field of this struct represents a modifier and is `true` if this modifier is active.
///
/// For some modifiers, this means that the key is currently pressed, others are toggled
/// (like caps lock).
#[derive(Copy, Clone, Debug, Default)]
pub struct ModifiersState {
    /// The "control" key
    pub ctrl: bool,
    /// The "alt" key
    pub alt: bool,
    /// The "shift" key
    pub shift: bool,
    /// The "Caps lock" key
    pub caps_lock: bool,
    /// The "logo" key
    ///
    /// Also known as the "windows" key on most keyboards
    pub logo: bool,
    /// The "Num lock" key
    pub num_lock: bool,
}

impl ModifiersState {
    fn new() -> ModifiersState {
        ModifiersState::default()
    }

    fn update_with(&mut self, state: &xkb::State) {
        self.ctrl = state.mod_name_is_active("Control", xkb::STATE_MODS_EFFECTIVE);
        self.alt = state.mod_name_is_active("Mod1", xkb::STATE_MODS_EFFECTIVE);
        self.shift = state.mod_name_is_active("Shift", xkb::STATE_MODS_EFFECTIVE);
        self.caps_lock = state.mod_name_is_active("Lock", xkb::STATE_MODS_EFFECTIVE);
        self.num_lock = state.mod_name_is_active("Mod2", xkb::STATE_MODS_EFFECTIVE);
        self.logo = state.mod_name_is_active("Mod4", xkb::STATE_MODS_EFFECTIVE);
    }
}

// Safety: No.
unsafe impl Send for KbState {}

impl KbState {
    pub(crate) fn update_modifiers(
        &mut self,
        mods_depressed: u32,
        mods_latched: u32,
        mods_locked: u32,
        group: u32,
    ) {
        let mask =
            self.xkb_state
                .update_mask(mods_depressed, mods_latched, mods_locked, 0, 0, group);
        if (mask & xkb::STATE_MODS_EFFECTIVE) > 0 {
            // effective value of mods have changed, we need to update our state
            self.mods_state.update_with(&self.xkb_state);
        }
    }

    pub(crate) fn get_one_sym_raw(&mut self, keycode: u32) -> u32 {
        self.xkb_state.key_get_one_sym(keycode + 8)
    }

    pub(crate) fn get_utf8_raw(&mut self, keycode: u32) -> Option<String> {
        Some(self.xkb_state.key_get_utf8(keycode + 8))
    }

    pub(crate) fn compose_feed(&mut self, keysym: u32) -> Option<xkb::compose::FeedResult> {
        match &mut self.xkb_compose_state {
            Some(compose_state) => Some(compose_state.feed(keysym)),
            None => None,
        }
    }

    pub(crate) fn compose_status(&mut self) -> Option<xkb::compose::Status> {
        match &mut self.xkb_compose_state {
            Some(compose_state) => Some(compose_state.status()),
            None => None,
        }
    }

    pub(crate) fn compose_get_utf8(&mut self) -> Option<String> {
        match &mut self.xkb_compose_state {
            Some(compose_state) => compose_state.utf8(),
            None => None,
        }
    }

    pub(crate) fn new_from_fd(fd: RawFd, size: usize) -> Result<KbState, std::io::Error> {
        let xkb_context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let xkb_keymap = unsafe {
            xkb::Keymap::new_from_fd(
                &xkb_context,
                fd,
                size,
                xkb::KEYMAP_FORMAT_TEXT_V1,
                xkb::KEYMAP_COMPILE_NO_FLAGS,
            )?
            .unwrap()
        };
        let xkb_state = xkb::State::new(&xkb_keymap);
        let mut me = KbState {
            xkb_context,
            xkb_keymap,
            xkb_state,
            xkb_compose_table: None,
            xkb_compose_state: None,
            mods_state: ModifiersState::new(),
        };

        me.init_compose();

        Ok(me)
    }

    pub(crate) fn init_compose(&mut self) {
        let locale = env::var_os("LC_ALL")
            .and_then(|v| if v.is_empty() { None } else { Some(v) })
            .or_else(|| env::var_os("LC_CTYPE"))
            .and_then(|v| if v.is_empty() { None } else { Some(v) })
            .or_else(|| env::var_os("LANG"))
            .and_then(|v| if v.is_empty() { None } else { Some(v) })
            .unwrap_or_else(|| "C".into());

        let compose_table = match xkb::compose::Table::new_from_locale(
            &self.xkb_context,
            &locale,
            xkb::compose::COMPILE_NO_FLAGS,
        ) {
            Ok(t) => t,
            Err(_) => return,
        };

        let compose_state = xkb::compose::State::new(&compose_table, xkb::compose::STATE_NO_FLAGS);

        self.xkb_compose_table = Some(compose_table);
        self.xkb_compose_state = Some(compose_state);
    }

    pub(crate) fn key_repeats(&mut self, keycode: xkb::Keycode) -> bool {
        self.xkb_keymap.key_repeats(keycode)
    }

    #[inline]
    pub(crate) fn mods_state(&self) -> ModifiersState {
        self.mods_state
    }
}
