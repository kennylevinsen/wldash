#[derive(Copy, Clone)]
pub struct Color(pub u32);

impl Color {
    pub const WHITE: Color = Color(0xFFFFFFFF);
    pub const GREY35: Color = Color(0xFF595959);
    pub const GREY50: Color = Color(0xFF7F7F7F);
    pub const GREY75: Color = Color(0xFFBFBFBF);
    pub const GREY80: Color = Color(0xFFCCCCCC);
    pub const RED: Color = Color(0xFFFF0000);
    pub const YELLOW: Color = Color(0xFFFFFF00);
    pub const LIGHTGREEN: Color = Color(0xFF7FFF7F);
    pub const LIGHTORANGE: Color = Color(0xFFFFBF00);
    pub const DARKORANGE: Color = Color(0xFFFF7F00);
    pub const BUFF: Color = Color(0xFFFFBF7F);

    #[allow(unused)]
    pub fn new(red: u8, green: u8, blue: u8, opacity: u8) -> Color {
        Color((opacity as u32) << 24 | (red as u32) << 16 | (green as u32) << 8 | (blue as u32))
    }

    fn red(self) -> u8 {
        ((self.0 >> 16) & 0xFF) as u8
    }
    fn green(self) -> u8 {
        ((self.0 >> 8) & 0xFF) as u8
    }
    fn blue(self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    pub fn alpha(self, alpha: f32) -> Color {
        unsafe {
            Color(
                (255. * alpha).to_int_unchecked::<u32>() << 24
                    | (self.red() as f32 * alpha).to_int_unchecked::<u32>() << 16
                    | (self.green() as f32 * alpha).to_int_unchecked::<u32>() << 8
                    | (self.blue() as f32 * alpha).to_int_unchecked::<u32>(),
            )
        }
    }
}
