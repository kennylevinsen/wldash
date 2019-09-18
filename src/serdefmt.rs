
use std::io::BufRead;
use serde::{ de::DeserializeOwned, Serialize };

pub enum SerdeFmt {
    #[cfg(feature="yaml-cfg")]
    Yaml,
    #[cfg(feature="json-cfg")]
    Json,
}

pub const CONFIG_NAMES: &'static [&'static str] = &[
    #[cfg(feature="yaml-cfg")]
    "config.yaml",
    #[cfg(feature="json-cfg")]
    "config.json"
];

impl SerdeFmt {
    #[inline]
    pub fn new(ext: &str) -> Option<Self> {
        match ext {
            #[cfg(feature="yaml-cfg")]
            "yaml" => Some(SerdeFmt::Yaml),
            #[cfg(feature="json-cfg")]
            "json" => Some(SerdeFmt::Json),
            _ => None,
        }
    }
    #[inline]
    pub fn from_reader<B: BufRead, T: DeserializeOwned>(&self, r: B) -> T {
        match self {
            #[cfg(feature="yaml-cfg")]
            SerdeFmt::Yaml => serde_yaml::from_reader(r).unwrap(),
            #[cfg(feature="json-cfg")]
            SerdeFmt::Json => serde_json::from_reader(r).unwrap()
        }
    }
    #[inline]
    pub fn to_string<T: Serialize + ?Sized>(&self, src: &T) -> String {
        match self {
            #[cfg(feature="yaml-cfg")]
            SerdeFmt::Yaml => serde_yaml::to_string(&src).unwrap(),
            #[cfg(feature="json-cfg")]
            SerdeFmt::Json => serde_json::to_string_pretty(&src).unwrap(),
        }
    }
}

impl Default for SerdeFmt {
    fn default() -> Self { // attributes require curly braces to work
        #[cfg(feature="yaml-cfg")]
        { SerdeFmt::Yaml }
        #[cfg(all(feature="json-cfg", not(feature="yaml-cfg")))]
        { SerdeFmt::Json }
    }
}
