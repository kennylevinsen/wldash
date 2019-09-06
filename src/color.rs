use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default)]
pub struct Color {
    red: f32,
    green: f32,
    blue: f32,
    opacity: f32,
}

impl Color {
    pub fn new(red: f32, green: f32, blue: f32, opacity: f32) -> Color {
        Color {
            red: if red > 1.0 {
                1.0
            } else if red < 0.0 {
                0.0
            } else {
                red
            },
            green: if green > 1.0 {
                1.0
            } else if green < 0.0 {
                0.0
            } else {
                green
            },
            blue: if blue > 1.0 {
                1.0
            } else if blue < 0.0 {
                0.0
            } else {
                blue
            },
            opacity: if opacity > 1.0 {
                1.0
            } else if opacity < 0.0 {
                0.0
            } else {
                opacity
            },
        }
    }

    pub fn blend(&self, other: &Color, ratio: f32) -> Color {
        let ratio = if ratio > 1.0 {
            1.0
        } else if ratio < 0.0 {
            0.0
        } else {
            ratio
        };

        Color {
            red: self.red + ((other.red - self.red) * ratio),
            green: self.green + ((other.green - self.green) * ratio),
            blue: self.blue + ((other.blue - self.blue) * ratio),
            opacity: self.opacity + ((other.opacity - self.opacity) * ratio),
        }
    }

    pub fn as_argb8888(&self) -> u32 {
        ((255.0 * self.opacity) as u32 & 0xFF) << 24
            | ((255.0 * self.red) as u32 & 0xFF) << 16
            | ((255.0 * self.green) as u32 & 0xFF) << 8
            | ((255.0 * self.blue) as u32 & 0xFF)
    }
}
