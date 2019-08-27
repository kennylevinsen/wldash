use crate::modules::module::Input;

pub enum Cmd {
    Exit,
    Draw,
    ForceDraw,
    ToggleVisible,
    MouseInput { pos: (u32, u32), input: Input },
    KeyboardInput { input: Input },
}
