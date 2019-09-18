
use std::io::BufRead;
use serde::{ de::DeserializeOwned, Serialize };

pub enum SerdeFmt {
    Json,
    Yaml
}

impl SerdeFmt {
    #[inline]
    pub fn try_new_with_ext(ext: &str) -> Option<Self> {
        match ext {
            "json" => Some(SerdeFmt::Json),
            "yaml" => Some(SerdeFmt::Yaml),
            _ => None,
        }
    }
    #[inline]
    pub fn from_reader<B: BufRead, T: DeserializeOwned>(&self, r: B) -> T {
        match self {
            SerdeFmt::Yaml => serde_yaml::from_reader(r).unwrap(),
            SerdeFmt::Json => serde_json::from_reader(r).unwrap()
        }
    }
    #[inline]
    pub fn to_string<T: Serialize + ?Sized>(&self, src: &T) -> String {
        match self {
            SerdeFmt::Yaml => serde_yaml::to_string(&src).unwrap(),
            SerdeFmt::Json => serde_json::to_string_pretty(&src).unwrap(),
        }
    }
}

impl Default for SerdeFmt {
    fn default() -> Self {
        SerdeFmt::Yaml
    }
}
