use memmap::MmapMut;

use crate::color::Color;

pub struct Buffer<'a> {
    buf: &'a mut MmapMut,
    dimensions: (u32, u32),
    subdimensions: Option<(u32, u32, u32, u32)>,
}

impl<'a> Buffer<'a> {
    pub fn new(buf: &'a mut MmapMut, dimensions: (u32, u32)) -> Buffer {
        Buffer {
            buf: buf,
            dimensions: dimensions,
            subdimensions: None,
        }
    }

    pub fn get_bounds(&self) -> (u32, u32, u32, u32) {
        if let Some(subdim) = self.subdimensions {
            subdim
        } else {
            (0, 0, self.dimensions.0, self.dimensions.1)
        }
    }

    pub fn get_signed_bounds(&self) -> (i32, i32, i32, i32) {
        if let Some(subdim) = self.subdimensions {
            (
                subdim.0 as i32,
                subdim.1 as i32,
                subdim.2 as i32,
                subdim.3 as i32,
            )
        } else {
            (0, 0, self.dimensions.0 as i32, self.dimensions.1 as i32)
        }
    }

    pub fn subdimensions(&mut self, subdimensions: (u32, u32, u32, u32)) -> Buffer {
        let bounds = self.get_bounds();
        if subdimensions.0 + subdimensions.2 > bounds.2
            || subdimensions.1 + subdimensions.3 > bounds.3
        {
            panic!(
                "cannot create subdimensions larger than buffer: {:?} > {:?}",
                subdimensions, bounds
            );
        }

        Buffer {
            buf: self.buf,
            dimensions: self.dimensions,
            subdimensions: Some((
                subdimensions.0 + bounds.0,
                subdimensions.1 + bounds.1,
                subdimensions.2,
                subdimensions.3,
            )),
        }
    }

    pub fn memset(&mut self, c: &Color) {
        if let Some(subdim) = self.subdimensions {
            unsafe {
                let ptr = self.buf.as_mut_ptr();
                for y in subdim.1..(subdim.1 + subdim.3) {
                    for x in subdim.0..(subdim.0 + subdim.2) {
                        *((ptr as *mut u32).offset((x + y * self.dimensions.0) as isize)) =
                            c.as_argb8888();
                    }
                }
            }
        } else {
            unsafe {
                let ptr = self.buf.as_mut_ptr();
                for p in 0..(self.dimensions.0 * self.dimensions.1) {
                    *((ptr as *mut u32).offset(p as isize)) = c.as_argb8888();
                }
            }
        }
    }

    pub fn highlight_bounds(&mut self, c: &Color) {
        let end = if let Some(subdim) = self.subdimensions {
            (subdim.2, subdim.3)
        } else {
            (self.dimensions.0, self.dimensions.1)
        };

        for x in 0..end.0 {
            self.put((x, 0), c);
            self.put((x, end.1 - 1), c);
        }
        for y in 0..end.1 {
            self.put((0, y), c);
            self.put((end.0 - 1, y), c);
        }
    }

    pub fn put(&mut self, pos: (u32, u32), c: &Color) {
        let true_pos = if let Some(subdim) = self.subdimensions {
            if pos.0 > subdim.2 || pos.1 > subdim.3 {
                panic!(
                    "put({:?}) is not within subdimensions of buffer ({:?})",
                    pos, subdim
                );
            }
            (pos.0 + subdim.0, pos.1 + subdim.1)
        } else {
            if pos.0 >= self.dimensions.0 || pos.1 >= self.dimensions.1 {
                panic!(
                    "put({:?}) is not within dimensions of buffer ({:?})",
                    pos, self.dimensions
                );
            }
            pos
        };

        unsafe {
            let ptr = self
                .buf
                .as_mut_ptr()
                .offset(4 * (true_pos.0 + (true_pos.1 * self.dimensions.0)) as isize);
            *(ptr as *mut u32) = c.as_argb8888();
        }
    }
}
