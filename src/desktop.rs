extern crate ini;
use ini::{Ini, ParseOption};
use std::cmp::Ordering;
use std::env;
use std::error::Error;
use std::io::Error as io_error;
use std::io::ErrorKind;
use walkdir::WalkDir;

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
        let file = Ini::load_from_file_opt(
            f,
            ParseOption {
                enabled_quote: false,
                enabled_escape: false,
            },
        )?;
        match file.section(Some("Desktop Entry")) {
            Some(desktop) => Ok(Desktop {
                entry_type: desktop.get("Type").unwrap_or(&"".to_string()).to_string(),
                name: desktop.get("Name").unwrap_or(&"".to_string()).to_string(),
                term: desktop.get("Terminal").unwrap_or(&"".to_string()) == "true",
                no_display: desktop.get("NoDisplay").unwrap_or(&"".to_string()) == "true",
                hidden: desktop.get("Hidden").unwrap_or(&"".to_string()) == "true",
                exec: desktop.get("Exec").map(|x| x.to_string()),
                url: desktop.get("URL").map(|x| x.to_string()),
                keywords: desktop
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

    fn parse_dir(d: &str) -> Result<Vec<Desktop>, Box<dyn Error>> {
        let mut files: Vec<Desktop> = Vec::with_capacity(16);
        for entry in WalkDir::new(d) {
            let entry = entry?;
            let path = entry.path();

            if let Ok(d) = Desktop::parse(path.to_str().unwrap()) {
                files.push(d)
            }
        }

        Ok(files)
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

    std::iter::once(xdg_data_home.as_str())
        .chain(xdg_data_dirs.split(':'))
        .map(|p| Desktop::parse_dir(&format!("{}/applications", p)))
        .filter_map(Result::ok)
        .flatten()
        .filter(|d| {
            !d.hidden && !d.no_display && (d.entry_type == "Application" || d.entry_type == "Link")
        })
        .collect()
}
