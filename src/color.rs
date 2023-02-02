//use serde::{Deserialize, Serialize};
/*
//#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default)]
pub struct Color {
    red: f32,
    green: f32,
    blue: f32,
    opacity: f32,
}

impl Color {
    pub fn new(red: f32, green: f32, blue: f32, opacity: f32) -> Color {
        let (red, green, blue, opacity) = clamp(red, green, blue, opacity);
        Color {
            red,
            green,
            blue,
            opacity,
        }
    }

    pub fn blend(&self, other: &Color, ratio: f32) -> Color {
        let ratio = clamp_f32(ratio, 0.0, 1.0);

        Color {
            red: blend_f32(self.red, other.red, ratio),
            green: blend_f32(self.green, other.green, ratio),
            blue: blend_f32(self.blue, other.blue, ratio),
            opacity: blend_f32(self.opacity, other.opacity, ratio),
        }
    }

    #[inline]
    pub fn as_argb8888(&self) -> u32 {
        ((255.0 * self.opacity) as u32 & 0xFF) << 24
            | ((255.0 * self.red) as u32 & 0xFF) << 16
            | ((255.0 * self.green) as u32 & 0xFF) << 8
            | ((255.0 * self.blue) as u32 & 0xFF)
    }

    #[inline]
    pub fn as_integer(&self) -> IntegerColor {
        IntegerColor(self.as_argb8888())
    }
}

#[inline]
fn clamp(r: f32, g: f32, b: f32, o: f32) -> (f32, f32, f32, f32) {
    clamp_naive(r, g, b, o)
}

#[inline]
fn clamp_naive(r: f32, g: f32, b: f32, o: f32) -> (f32, f32, f32, f32) {
    (
        clamp_f32(r, 0.0, 1.0),
        clamp_f32(g, 0.0, 1.0),
        clamp_f32(b, 0.0, 1.0),
        clamp_f32(o, 0.0, 1.0),
    )
}

#[inline]
fn clamp_f32(v: f32, a: f32, b: f32) -> f32 {
    if v > b {
        b
    } else if v < a {
        a
    } else {
        v
    }
}

#[inline]
fn blend_f32(a: f32, b: f32, r: f32) -> f32 {
    a + ((b - a) * r)
}
*/
#[inline]
fn blend_u8(a: u8, b: u8, r: f32) -> u32 {
    let af = a as f32;
    let bf = b as f32;
    (af + ((bf - af) * r)) as u32
}

#[derive(Copy, Clone)]
pub struct Color(pub u32);

impl Color {
    pub const BLACK:       Color = Color(0xFF000000);
    pub const WHITE:       Color = Color(0xFFFFFFFF);
    pub const GREY50:      Color = Color(0xFF7F7F7F);
    pub const GREY75:      Color = Color(0xFFBFBFBF);
    pub const GREY80:      Color = Color(0xFFCCCCCC);
    pub const RED:         Color = Color(0xFFFF0000);
    pub const YELLOW:      Color = Color(0xFFFFFF00);
    pub const LIGHTGREEN:  Color = Color(0xFF7FFF7F);
    pub const LIGHTRED:    Color = Color(0xFFFF7F7F);
    pub const LIGHTORANGE: Color = Color(0xFFFFBF00);
    pub const DARKORANGE:  Color = Color(0xFFFF7F00);
    pub const BUFF:        Color = Color(0xFFFFBF7F);

    #[allow(unused)]
    pub fn new(red: u8, green: u8, blue: u8, opacity: u8) -> Color {
        Color((opacity as u32) << 24 | (red as u32) << 16 | (green as u32) << 8 | (blue as u32))
    }

    fn opacity(self) -> u8 {((self.0 >> 24) & 0xFF) as u8 }
    fn red(self) -> u8 {((self.0 >> 16) & 0xFF) as u8 }
    fn green(self) -> u8 {((self.0 >> 8) & 0xFF) as u8 }
    fn blue(self) -> u8 {(self.0 & 0xFF) as u8 }

    pub fn blend(self, other: Color, ratio: f32) -> Color {
        Color(
            blend_u8(self.opacity(), other.opacity(), ratio) << 24 |
            blend_u8(self.red(), other.red(), ratio) << 16 |
            blend_u8(self.green(), other.green(), ratio) << 8 |
            blend_u8(self.blue(), other.blue(), ratio)
        )
    }
}
