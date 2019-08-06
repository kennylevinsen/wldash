use crate::buffer::Buffer;
use crate::color::Color;

use std::collections::HashMap;

use rusttype::{point, Font as RustFont, Scale};
use lazy_static::lazy_static;

pub static DEJAVUSANS_MONO_FONT_DATA: &'static [u8] = include_bytes!("../fonts/dejavu/DejaVuSansMono.ttf");
pub static ROBOTO_REGULAR_FONT_DATA: &'static [u8] = include_bytes!("../fonts/Roboto-Regular.ttf");

lazy_static! {
    pub static ref DEJAVUSANS_MONO: RustFont<'static> = RustFont::from_bytes(DEJAVUSANS_MONO_FONT_DATA as &[u8]).expect("error constructing DejaVuSansMono");
    pub static ref ROBOTO_REGULAR: RustFont<'static> = RustFont::from_bytes(ROBOTO_REGULAR_FONT_DATA as &[u8]).expect("error constructing Roboto-Regular");
}

struct CachedGlyph {
    dimensions: (u32, u32),
    origin: (i32, i32),
    render: Vec<f32>,
}

impl CachedGlyph {
    fn new(font: &RustFont, size: f32, ch: char) -> CachedGlyph {
        let scale = Scale::uniform(size);
        let v_metrics = font.v_metrics(scale);
        let glyph = font
            .glyph(ch)
            .scaled(scale)
            .positioned(point(0.0, v_metrics.ascent));


        if let Some(bounding_box) = glyph.pixel_bounding_box() {
            let origin = (bounding_box.min.x, bounding_box.min.y);

            let dimensions = ((bounding_box.max.x - bounding_box.min.x) as u32, (bounding_box.max.y - bounding_box.min.y) as u32);
            let mut render = vec![0.0; (dimensions.0 * dimensions.1) as usize];
            glyph.draw(|x, y, o| {
                let pos = x + (y * dimensions.0);
                render[pos as usize] = o;
            });
            CachedGlyph{
                origin: origin,
                dimensions: dimensions,
                render: render,
            }
        } else {
            CachedGlyph{
                origin: (0,0),
                dimensions: ((size/4.0) as u32, 0),
                render: Vec::new(),
            }
        }
    }

    fn draw(&self, buf: &mut Buffer, pos: (i32, i32), bg: &Color, c: &Color) {
        let mut x = 0;
        let mut y = 0;
        for v in &self.render {
            let _ = buf.put(((x + pos.0 + self.origin.0) as u32, (y + pos.1 + self.origin.1) as u32), &bg.blend(&c, *v));

            if x == self.dimensions.0 as i32 -1 {
                y += 1;
                x = 0;
            } else {
                x += 1;
            }
        }

    }
}

pub struct Font {
    glyphs: HashMap<char, CachedGlyph>,
    font: &'static RustFont<'static>,
    size: f32,
}

impl Font {
    pub fn new(font: &'static RustFont, size: f32) -> Font {
        Font{
            glyphs: HashMap::new(),
            font: font,
            size: size,
        }
    }

    pub fn draw_text(&mut self, buf: &mut Buffer, bg: &Color, c: &Color, s: &str) -> Result<(u32, u32), ::std::io::Error> {
        let mut x_off = 0;
        let mut off = 0;
        for ch in s.chars() {
            let glyph = match self.glyphs.get(&ch) {
                Some(glyph) => glyph,
                None => {
                    let glyph = CachedGlyph::new(self.font, self.size, ch);
                    self.glyphs.insert(ch, glyph);
                    self.glyphs.get(&ch).unwrap()
                }
            };
            if glyph.origin.1 < off {
                off = glyph.origin.1
            }
        }
        for ch in s.chars() {
            let glyph = &self.glyphs[&ch];
            glyph.draw(buf, (x_off, -off), bg, c);
            x_off += glyph.dimensions.0 as i32 + glyph.origin.0;
        }

        Ok((x_off as u32, self.size as u32))
    }

    pub fn draw_text_fixed_width(&mut self, buf: &mut Buffer, bg: &Color, c: &Color, distances: &[u32], s: &str) -> Result<(u32, u32), ::std::io::Error> {
        let mut x_off = 0;
        let mut off = 0;
        for ch in s.chars() {
            let glyph = match self.glyphs.get(&ch) {
                Some(glyph) => glyph,
                None => {
                    let glyph = CachedGlyph::new(self.font, self.size, ch);
                    self.glyphs.insert(ch, glyph);
                    self.glyphs.get(&ch).unwrap()
                }
            };
            if glyph.origin.1 < off {
                off = glyph.origin.1
            }
        }
        for (idx, ch) in s.chars().enumerate() {
            let glyph = &self.glyphs[&ch];
            glyph.draw(buf, (x_off, -off), bg, c);
            x_off += distances[idx] as i32;
        }

        Ok((x_off as u32, self.size as u32))
    }

    pub fn draw_text_individual_colors(&mut self, buf: &mut Buffer, bg: &Color, color: &[Color], s: &str) -> Result<(u32, u32), ::std::io::Error> {
        let mut x_off = 0;
        let mut off = 0;
        for ch in s.chars() {
            let glyph = match self.glyphs.get(&ch) {
                Some(glyph) => glyph,
                None => {
                    let glyph = CachedGlyph::new(self.font, self.size, ch);
                    self.glyphs.insert(ch, glyph);
                    self.glyphs.get(&ch).unwrap()
                }
            };
            if glyph.origin.1 < off {
                off = glyph.origin.1
            }
        }
        for (idx, ch) in s.chars().enumerate() {
            let glyph = &self.glyphs[&ch];
            glyph.draw(buf, (x_off, -off), bg, &color[idx]);
            x_off += glyph.dimensions.0 as i32 + glyph.origin.0;
        }

        Ok((x_off as u32, self.size as u32))
    }
}

pub fn draw_box(buf: &mut Buffer, c: &Color, dim: (u32, u32)) -> Result<(), ::std::io::Error> {
    let mut buf = buf.subdimensions((0, 0, dim.0, dim.1))?;

    for x in 0..dim.0 {
        let _ = buf.put((x, 0), c);
        let _ = buf.put((x, dim.1 - 1), c);
    }
    for y in 0..dim.1 {
        buf.put((0, y), c)?;
        buf.put((dim.0 - 1, y), c)?;
    }

    Ok(())
}

pub fn draw_bar(
    buf: &mut Buffer,
    color: &Color,
    length: u32,
    height: u32,
    fill: f32,
) -> Result<(), ::std::io::Error> {
    let mut buf = buf.subdimensions((0, 0, length, height))?;

    let mut fill_pos = ((length as f32) * fill) as u32;
    if fill_pos > length {
        fill_pos = length;
    }
    for y in 0..height {
        for x in 0..fill_pos {
            let _ = buf.put((x, y), color);
        }
    }

    Ok(())
}
