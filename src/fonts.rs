//! Utility module for fonts

use crate::draw;
use fontconfig::Fontconfig as FontConfig;
use rusttype::Font;
use std::{
    collections::HashMap,
    fs::File,
    hash,
    io::Read,
    path::{Path, PathBuf},
};

/// FontRef is used to store Fonts on widgets.
pub type FontRef<'a> = &'a rusttype::Font<'a>;

/// FontSeeker is a marker struct that is used to look up fonts
pub(crate) struct FontSeeker;

impl FontSeeker {
    /// Acts like fc-match.
    /// Given a string, it matches it to a font file and returns its path.
    pub(crate) fn from_string(name: &str) -> PathBuf {
        let fc = FontConfig::new().unwrap();
        fc.find(name, None).unwrap().path
    }
}

/// FontLoader is a marker struct that is used to load files
pub(crate) struct FontLoader;

impl FontLoader {
    /// Given a path, loads it as a Font, which can be rendered to the screen.
    pub(crate) fn from_path<'a, P>(path: P) -> Option<Font<'a>>
    where
        P: AsRef<Path>,
    {
        let mut file = File::open(path).expect("Font file not found");
        let mut data = match file.metadata() {
            Ok(metadata) => Vec::with_capacity(metadata.len() as usize),
            Err(_) => vec![],
        };
        file.read_to_end(&mut data).unwrap();
        Font::try_from_vec(data)
    }
}

#[derive(Debug, Copy, Clone)]
struct ComparableF32(f32);
impl ComparableF32 {
    fn key(&self) -> u64 {
        self.0.to_bits() as u64
    }
}

impl hash::Hash for ComparableF32 {
    fn hash<H>(&self, state: &mut H)
    where
        H: hash::Hasher,
    {
        self.key().hash(state)
    }
}

impl PartialEq for ComparableF32 {
    fn eq(&self, other: &ComparableF32) -> bool {
        self.key() == other.key()
    }
}

impl Eq for ComparableF32 {}

pub struct FontMap {
    fonts: HashMap<(&'static str, ComparableF32), draw::Font<'static>>,
    required_fonts: HashMap<&'static str, (&'static str, Vec<f32>)>,
}

impl FontMap {
    pub fn new() -> FontMap {
        FontMap {
            fonts: HashMap::new(),
            required_fonts: HashMap::new(),
        }
    }

    pub fn queue_font_name(&mut self, font_name: &'static str, size: f32) {
        match self.required_fonts.get_mut(font_name) {
            Some(v) => v.1.push(size),
            None => {
                self.required_fonts
                    .insert(font_name, (font_name, vec![size]));
            }
        }
    }

    pub fn queue_font_path(&mut self, font_name: &'static str, font_path: &'static str, size: f32) {
        match self.required_fonts.get_mut(font_name) {
            Some(v) => v.1.push(size),
            None => {
                self.required_fonts
                    .insert(font_name, (font_path, vec![size]));
            }
        }
    }

    pub fn load_fonts(&mut self) {
        for (font_name, v) in self.required_fonts.iter() {
            let path = if v.0.starts_with("/") {
                std::path::PathBuf::from(v.0)
            } else {
                FontSeeker::from_string(v.0)
            };
            let fontref = Box::leak(Box::new(
                FontLoader::from_path(path).expect("unable to load font"),
            ));
            for size in &v.1 {
                let font = draw::Font::new(fontref, *size);
                self.fonts.insert((font_name, ComparableF32(*size)), font);
            }
        }
    }

    pub fn get_font(&mut self, font_name: &'static str, size: f32) -> &mut draw::Font<'static> {
        self.fonts
            .get_mut(&(font_name, ComparableF32(size)))
            .expect("no font at specified size")
    }
}
