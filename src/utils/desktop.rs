use std::{
    cmp::Ordering,
    collections::HashSet,
    error::Error,
    fs::{read_to_string, File},
    io::{Error as io_error, ErrorKind},
};

use serde::{self, Deserialize, Serialize};
use simd_json;
use walkdir::WalkDir;

use crate::utils::{inish, xdg};

#[derive(Serialize, Deserialize, Clone, Debug, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub struct Desktop {
    pub entry_type: String,
    pub name: String,
    pub no_display: bool,
    pub hidden: bool,
    pub exec: Option<String>,
    pub url: Option<String>,
    pub term: bool,
    pub keywords: Vec<String>,
}

impl Desktop {
    fn parse(f: &str) -> Result<Desktop, Box<dyn Error>> {
        let s = read_to_string(f)?;
        let config = inish::parse(&s)?;
        match config.get("Desktop Entry") {
            Some(section) => Ok(Desktop {
                entry_type: section.get("Type").unwrap_or(&"").to_string(),
                name: section.get("Name").unwrap_or(&"").to_string(),
                term: section.get("Terminal").unwrap_or(&"") == &"true",
                no_display: section.get("NoDisplay").unwrap_or(&"") == &"true",
                hidden: section.get("Hidden").unwrap_or(&"") == &"true",
                exec: section.get("Exec").map(|x| x.to_string()),
                url: section.get("URL").map(|x| x.to_string()),
                keywords: section
                    .get("Keywords")
                    .map(|x| {
                        x.split(';')
                            .map(|y| y.trim().to_string())
                            .filter(|z| z != "")
                            .collect()
                    })
                    .unwrap_or_else(|| vec![]),
            }),
            None => Err(Box::new(io_error::new(
                ErrorKind::NotFound,
                "no desktop entry in file",
            ))),
        }
    }
}

impl Ord for Desktop {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for Desktop {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Desktop {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

pub fn load_desktop_cache() -> Result<Vec<Desktop>, Box<dyn Error>> {
    match File::open(format!("{}/wldash/desktop.json", xdg::cache_folder())) {
        Ok(f) => simd_json::from_reader(f).map_err(|e| e.into()),
        Err(e) => Err(e.into())
    }
}

pub fn write_desktop_cache(v: &Vec<Desktop>) -> Result<(), Box<dyn Error>> {
    match File::create(format!("{}/wldash/desktop.json", xdg::cache_folder())) {
        Ok(f) => simd_json::to_writer(f, v).map_err(|e| e.into()),
        Err(e) => Err(e.into())
    }
}

pub fn load_desktop_files() -> Vec<Desktop> {
    let dirs = xdg::data_folders();

    let mut desktop: Vec<Desktop> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for dir in dirs {
        for entry in WalkDir::new(format!("{}/applications", dir)) {
            if let Ok(entry) = entry {
                if let Ok(d) = Desktop::parse(entry.path().to_str().unwrap()) {
                    if d.hidden
                        || d.no_display
                        || (d.entry_type != "Application" && d.entry_type != "Link")
                    {
                        continue;
                    }
                    if !seen.contains(&d.name) {
                        seen.insert(d.name.to_string());
                        desktop.push(d);
                    }
                }
            }
        }
    }

    desktop
}
