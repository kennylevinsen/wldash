use smithay_client_toolkit::keyboard::{KeyState, ModifiersState};

pub enum Cmd {
    Exit,
    Draw,
    ForceDraw,
    ToggleVisible,
    MouseClick {
        btn: u32,
        pos: (u32, u32),
    },
    MouseScroll {
        scroll: (f64, f64),
        pos: (u32, u32),
    },
    Keyboard {
        key: u32,
        key_state: KeyState,
        modifiers_state: ModifiersState,
        interpreted: Option<String>,
    },
}
