//! Utility module for fonts

use fontconfig::Fontconfig as FontConfig;
use rusttype::Font;
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

/// FontMap is used to store different font configurations
pub type FontMap = HashMap<String, rusttype::Font<'static>>;

/// FontRef is used to store Fonts on widgets.
pub type FontRef = &'static rusttype::Font<'static>;

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
    pub(crate) fn from_path<P>(path: P) -> Option<Font<'static>>
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
