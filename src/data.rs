use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::{create_dir_all, File, OpenOptions};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Default)]
pub struct Data {
    pub entries: HashMap<String, i64>,
}

impl Data {
    pub fn load() -> Result<Data, Box<dyn Error>> {
        Ok(serde_yaml::from_reader(Self::read_file()?)?)
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        Ok(serde_yaml::to_writer(Self::write_file()?, self)?)
    }

    fn read_file() -> Result<File, Box<dyn Error>> {
        Ok(OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(Self::path()?)?)
    }

    fn write_file() -> Result<File, Box<dyn Error>> {
        Ok(OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(Self::path()?)?)
    }

    fn path() -> Result<PathBuf, Box<dyn Error>> {
        let xdg_cache = match env::var_os("XDG_CACHE_HOME") {
            Some(s) => s
                .into_string()
                .map_err(|_| "Unable to resolve $XDG_CACHE_HOME")?,
            None => format!(
                "{}/.cache",
                env::var_os("HOME")
                    .ok_or("Unable to resolve $HOME")?
                    .into_string()
                    .map_err(|_| "Unable to resolve $HOME")?
            ),
        };

        let cache_dir = PathBuf::from(xdg_cache).join("wldash");
        create_dir_all(&cache_dir)?;

        Ok(cache_dir.join("data.yaml"))
    }
}
