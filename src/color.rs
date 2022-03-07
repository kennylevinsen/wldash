use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default)]
pub struct Color {
    red: f32,
    green: f32,
    blue: f32,
    opacity: f32,
}

#[inline]
fn validate_color(color: &Color) {
    validate_color_range(color.red);
    validate_color_range(color.green);
    validate_color_range(color.blue);
    validate_color_range(color.opacity);
}

#[inline]
fn validate_color_range(value: f32) {
    if value < 0.0 || value > 1.0 {
        panic!("Invalid color value! Valid values are in the [0.0; 1.0] range.");
    }
}

impl Color {
    pub fn new(red: f32, green: f32, blue: f32, opacity: f32) -> Color {
        validate_color_range(red);
        validate_color_range(green);
        validate_color_range(blue);
        validate_color_range(opacity);
        Color {
            red,
            green,
            blue,
            opacity,
        }
    }

    pub fn blend(&self, other: &Color, ratio: f32) -> Color {
        validate_color(self);
        validate_color(other);
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
        validate_color(self);
        ((255.0 * self.opacity) as u32 & 0xFF) << 24
            | ((255.0 * self.red) as u32 & 0xFF) << 16
            | ((255.0 * self.green) as u32 & 0xFF) << 8
            | ((255.0 * self.blue) as u32 & 0xFF)
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
