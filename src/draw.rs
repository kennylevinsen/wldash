use crate::buffer::Buffer;
use crate::color::Color;
use rusttype::{point, Font, Scale};

pub static DEJAVUSANS_MONO: &'static [u8] = include_bytes!("../fonts/dejavu/DejaVuSansMono.ttf");
pub static ROBOTO_REGULAR: &'static [u8] = include_bytes!("../fonts/Roboto-Regular.ttf");

pub fn draw_text(
    font_data: &'static [u8],
    buf: &mut Buffer,
    background_color: &Color,
    color: &Color,
    size: f32,
    s: &str,
) -> Result<(), ::std::io::Error> {
    // Load the font
    // This only succeeds if collection consists of one font
    let font = Font::from_bytes(font_data as &[u8]).expect("Error constructing Font");

    // The font size to use
    let scale = Scale::uniform(size);

    let v_metrics = font.v_metrics(scale);

    // layout the glyphs in a line with 20 pixels padding
    let glyphs: Vec<_> = font
        .layout(s, scale, point(0.0, v_metrics.ascent))
        .collect();

    // Loop through the glyphs in the text, positing each one on a line
    for glyph in glyphs {
        if let Some(bounding_box) = glyph.pixel_bounding_box() {
            // Draw the glyph into the image per-pixel by using the draw closure
            glyph.draw(|x, y, o| {
                let x = x + bounding_box.min.x as u32;
                let y = y + bounding_box.min.y as u32;
                let o = if o > 1.0 {
                    1.0
                } else if o < 0.0 {
                    0.0
                } else {
                    o
                };
                buf.put((x, y), &background_color.blend(color, o));
            });
        }
    }

    Ok(())
}

pub fn draw_text_fixed_width(
    font_data: &'static [u8],
    buf: &mut Buffer,
    background_color: &Color,
    color: &Color,
    size: f32,
    distances: Vec<u32>,
    s: &str,
) -> Result<(), ::std::io::Error> {
    // Load the font
    // This only succeeds if collection consists of one font
    let font = Font::from_bytes(font_data as &[u8]).expect("Error constructing Font");

    // The font size to use
    let scale = Scale::uniform(size);

    let v_metrics = font.v_metrics(scale);

    // layout the glyphs in a line with 20 pixels padding
    let glyphs: Vec<_> = font
        .layout(s, scale, point(0.0, v_metrics.ascent))
        .collect();

    // Loop through the glyphs in the text, positing each one on a line
    let mut x_pos: usize = 0;
    let mut x_off: u32 = 0;
    for glyph in glyphs {
        if let Some(bounding_box) = glyph.pixel_bounding_box() {
            let x_dist = distances[x_pos];
            let width = (bounding_box.max.x - bounding_box.min.x) as u32;
            let offset = (x_dist - width) / 2;
            // Draw the glyph into the image per-pixel by using the draw closure
            glyph.draw(|x, y, o| {
                let off = x_off + offset + 20;
                let x = x + off as u32;
                let y = y + bounding_box.min.y as u32;
                let o = if o > 1.0 {
                    1.0
                } else if o < 0.0 {
                    0.0
                } else {
                    o
                };
                buf.put((x, y), &background_color.blend(color, o));
            });
            x_pos += 1;
            x_off += x_dist;
        }
    }

    Ok(())
}
