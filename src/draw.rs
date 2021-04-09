use crate::buffer::Buffer;
use crate::{color::Color, fonts::FontRef};

use rusttype::{point, Scale};
use std::collections::HashMap;

struct CachedGlyph {
    dimensions: (u32, u32),
    origin: (i32, i32),
    render: Vec<f32>,
    advance: i32,
}

impl CachedGlyph {
    fn new(font: FontRef, size: f32, ch: char) -> CachedGlyph {
        let scale = Scale::uniform(size);
        let v_metrics = font.v_metrics(scale);
        let glyph = font.glyph(ch).scaled(scale);
        let advance = glyph.h_metrics().advance_width as i32;
        let glyph = glyph.positioned(point(0.0, v_metrics.ascent));

        if let Some(bounding_box) = glyph.pixel_bounding_box() {
            let origin = (bounding_box.min.x, bounding_box.min.y);

            let dimensions = (
                (bounding_box.max.x - bounding_box.min.x) as u32,
                (bounding_box.max.y - bounding_box.min.y) as u32,
            );
            let mut render = vec![0.0; (dimensions.0 * dimensions.1) as usize];
            glyph.draw(|x, y, o| {
                let pos = x + (y * dimensions.0);
                render[pos as usize] = o;
            });
            CachedGlyph {
                origin,
                dimensions,
                render,
                advance,
            }
        } else {
            CachedGlyph {
                origin: (0, 0),
                dimensions: (0, 0),
                render: Vec::new(),
                advance: (size / 4.0) as i32,
            }
        }
    }

    fn draw(&self, buf: &mut Buffer, pos: (i32, i32), bg: &Color, c: &Color) {
        let mut x = 0;
        let mut y = 0;
        for v in &self.render {
            let _ = buf.put(
                (
                    (x + pos.0 + self.origin.0) as u32,
                    (y + pos.1 + self.origin.1) as u32,
                ),
                &bg.blend(&c, *v),
            );

            if x == self.dimensions.0 as i32 - 1 {
                y += 1;
                x = 0;
            } else {
                x += 1;
            }
        }
    }
}

pub struct Font<'a> {
    glyphs: HashMap<char, CachedGlyph>,
    font: FontRef<'a>,
    size: f32,
}

impl<'a> Font<'a> {
    pub fn new(font: FontRef<'a>, size: f32) -> Font {
        Font {
            glyphs: HashMap::new(),
            font,
            size,
        }
    }

    pub fn add_str_to_cache(&mut self, s: &str) {
        for ch in s.chars() {
            if self.glyphs.get(&ch).is_none() {
                let glyph = CachedGlyph::new(self.font, self.size, ch);
                self.glyphs.insert(ch, glyph);
            }
        }
    }

    pub fn draw_text(
        &self,
        buf: &mut Buffer,
        bg: &Color,
        c: &Color,
        s: &str,
    ) -> Result<(u32, u32), ::std::io::Error> {
        let mut x_off = 0;
        let mut off = 0;
        let mut glyphs = Vec::with_capacity(s.len());
        for ch in s.chars() {
            let glyph = match self.glyphs.get(&ch) {
                Some(glyph) => glyph,
                None => {
                    return Err(::std::io::Error::new(
                        ::std::io::ErrorKind::Other,
                        format!("glyph for {:} not in cache", ch),
                    ))
                }
            };
            glyphs.push(glyph);
            if glyph.origin.1 < off {
                off = glyph.origin.1
            }
        }
        for glyph in glyphs {
            glyph.draw(buf, (x_off, -off), bg, c);
            x_off += glyph.advance;
        }

        Ok((x_off as u32, self.size as u32))
    }

    pub fn draw_text_with_cursor(
        &self,
        buf: &mut Buffer,
        bg: &Color,
        c: &Color,
        s: &str,
        cursor: usize,
    ) -> Result<(u32, u32), ::std::io::Error> {
        let mut x_off: i32 = 0;
        let mut off: i32 = 0;
        let mut glyphs = Vec::with_capacity(s.len());
        for ch in s.chars() {
            let glyph = match self.glyphs.get(&ch) {
                Some(glyph) => glyph,
                None => {
                    return Err(::std::io::Error::new(
                        ::std::io::ErrorKind::Other,
                        format!("glyph for {:} not in cache", ch),
                    ))
                }
            };
            glyphs.push(glyph);
            if glyph.origin.1 < off {
                off = glyph.origin.1
            }
        }

        let height = buf.get_bounds().3;
        for (i, glyph) in glyphs.iter().enumerate() {
            if i == cursor {
                self.draw_cursor(buf, c, x_off as u32, height)?;
            }
            glyph.draw(buf, (x_off, -off), bg, c);
            x_off += glyph.advance;
        }
        if cursor == glyphs.len() {
            self.draw_cursor(buf, c, x_off as u32, height)?;
        }

        Ok((x_off as u32, self.size as u32))
    }

    pub fn draw_cursor(
        &self,
        buf: &mut Buffer,
        c: &Color,
        offset: u32,
        height: u32,
    ) -> Result<(), ::std::io::Error> {
        // draw cursor
        for i in 1..height {
            buf.put((offset, i), c)?
        }
        Ok(())
    }

    pub fn auto_widest(&mut self, s: &str) -> Result<u32, ::std::io::Error> {
        self.add_str_to_cache(s);
        let mut max = 0;
        for ch in s.chars() {
            let glyph = match self.glyphs.get(&ch) {
                Some(glyph) => glyph,
                None => {
                    return Err(::std::io::Error::new(
                        ::std::io::ErrorKind::Other,
                        format!("glyph for {:} not in cache", ch),
                    ))
                }
            };
            let width = glyph.advance;
            if width > max {
                max = width
            }
        }
        Ok(max as u32)
    }

    pub fn auto_draw_text(
        &mut self,
        buf: &mut Buffer,
        bg: &Color,
        c: &Color,
        s: &str,
    ) -> Result<(u32, u32), ::std::io::Error> {
        self.add_str_to_cache(s);
        self.draw_text(buf, bg, c, s)
    }

    pub fn auto_draw_text_with_cursor(
        &mut self,
        buf: &mut Buffer,
        bg: &Color,
        c: &Color,
        s: &str,
        cursor: usize,
    ) -> Result<(u32, u32), ::std::io::Error> {
        self.add_str_to_cache(s);
        self.draw_text_with_cursor(buf, bg, c, s, cursor)
    }

    pub fn draw_text_fixed_width(
        &self,
        buf: &mut Buffer,
        bg: &Color,
        c: &Color,
        distances: &[u32],
        s: &str,
    ) -> Result<(u32, u32), ::std::io::Error> {
        let mut x_off = 0;
        let mut off = 0;
        let mut glyphs = Vec::with_capacity(s.len());
        for ch in s.chars() {
            let glyph = match self.glyphs.get(&ch) {
                Some(glyph) => glyph,
                None => {
                    return Err(::std::io::Error::new(
                        ::std::io::ErrorKind::Other,
                        format!("glyph for {:} not in cache", ch),
                    ))
                }
            };
            glyphs.push(glyph);
            if glyph.origin.1 < off {
                off = glyph.origin.1
            }
        }
        for (idx, glyph) in glyphs.into_iter().enumerate() {
            glyph.draw(buf, (x_off, -off), bg, c);
            x_off += distances[idx] as i32;
        }

        Ok((x_off as u32, self.size as u32))
    }

    pub fn draw_text_individual_colors(
        &self,
        buf: &mut Buffer,
        bg: &Color,
        color: &[Color],
        s: &str,
    ) -> Result<(u32, u32), ::std::io::Error> {
        let mut x_off = 0;
        let mut off = 0;
        let mut glyphs = Vec::with_capacity(s.len());
        for ch in s.chars() {
            let glyph = match self.glyphs.get(&ch) {
                Some(glyph) => glyph,
                None => {
                    return Err(::std::io::Error::new(
                        ::std::io::ErrorKind::Other,
                        format!("glyph for {:} not in cache", ch),
                    ))
                }
            };
            glyphs.push(glyph);
            if glyph.origin.1 < off {
                off = glyph.origin.1
            }
        }
        for (idx, glyph) in glyphs.into_iter().enumerate() {
            glyph.draw(buf, (x_off, -off), bg, &color[idx]);
            x_off += glyph.advance;
        }

        Ok((x_off as u32, self.size as u32))
    }

    pub fn auto_draw_text_individual_colors(
        &mut self,
        buf: &mut Buffer,
        bg: &Color,
        color: &[Color],
        s: &str,
    ) -> Result<(u32, u32), ::std::io::Error> {
        self.add_str_to_cache(s);
        self.draw_text_individual_colors(buf, bg, color, s)
    }
}

pub fn draw_box(buf: &mut Buffer, c: &Color, dim: (u32, u32)) -> Result<(), ::std::io::Error> {
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
