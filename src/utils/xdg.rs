use std::env;

pub fn cache_folder() -> String {
    match env::var_os("XDG_CACHE_HOME") {
        Some(s) => s.into_string().unwrap(),
        None => {
            let home = env::var_os("HOME").unwrap().into_string().unwrap();
            format!("{}/.cache", home)
        }
    }
}


pub fn config_folder() -> String {
    match env::var_os("XDG_CONFIG_HOME") {
        Some(s) => s.into_string().unwrap(),
        None => {
            let home = env::var_os("HOME").unwrap().into_string().unwrap();
            format!("{}/.config", home)
        }
    }
}

pub fn data_folders() -> Vec<String> {
    let xdg_data_home = match env::var_os("XDG_DATA_HOME") {
        Some(s) => s.into_string().unwrap(),
        None => {
            let home = env::var_os("HOME").unwrap().into_string().unwrap();
            format!("{}/.local/share", home)
        }
    };
    let xdg_data_dirs = match env::var_os("XDG_DATA_DIRS") {
        Some(s) => s.into_string().unwrap(),
        None => "/usr/local/share:/usr/share".to_string(),
    };

    // Eww.
    std::iter::once(xdg_data_home.to_string()).chain(xdg_data_dirs.split(':').map(|x| x.to_string())).collect()
}
