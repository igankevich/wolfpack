use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuildTarget {
    Executable,
    Library,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub prefix: PathBuf,
    pub sysroot: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prefix: "/opt".into(),
            sysroot: "/".into(),
        }
    }
}
