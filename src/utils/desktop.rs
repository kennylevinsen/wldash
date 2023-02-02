use std::{
    cmp::Ordering,
    collections::HashSet,
    env,
    error::Error,
    fs::read_to_string,
    io::{Error as io_error, ErrorKind},
};

use walkdir::WalkDir;

use crate::utils::inish;

#[derive(Clone, Debug, Eq, Hash)]
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

pub fn load_desktop_files() -> Vec<Desktop> {
    let home = env::var_os("HOME").unwrap().into_string().unwrap();

    let xdg_data_home = match env::var_os("XDG_DATA_HOME") {
        Some(s) => s.into_string().unwrap(),
        None => format!("{}/.local/share", home),
    };
    let xdg_data_dirs = match env::var_os("XDG_DATA_DIRS") {
        Some(s) => s.into_string().unwrap(),
        None => "/usr/local/share:/usr/share".to_string(),
    };

    let dirs = std::iter::once(xdg_data_home.as_str()).chain(xdg_data_dirs.split(':'));

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
