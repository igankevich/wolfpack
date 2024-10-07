use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize, Deserialize, Clone)]
pub struct CompactManifest {
    name: String,
    origin: String,
    version: String,
    comment: String,
    maintainer: String,
    www: String,
    abi: String,
    arch: String,
    prefix: PathBuf,
    flatsize: u64,
    licenselogic: String,
    licenses: Vec<String>,
    desc: String,
    #[serde(default)]
    deps: HashMap<String, Dependency>,
    categories: Vec<String>,
    shlibs_required: Vec<PathBuf>,
    shlibs_provided: Vec<PathBuf>,
    annotations: HashMap<String, String>,
}

impl ToString for CompactManifest {
    fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

impl FromStr for CompactManifest {
    type Err = serde_json::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        eprintln!("parse");
        serde_json::from_str(value)
    }
}

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    #[serde(flatten)]
    pub(crate) compact: CompactManifest,
    // TODO hashes
    pub(crate) files: HashMap<PathBuf, String>,
    pub(crate) config: Vec<PathBuf>,
    pub(crate) directories: HashMap<PathBuf, String>,
}

impl ToString for Manifest {
    fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

impl FromStr for Manifest {
    type Err = serde_json::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(value)
    }
}

/// This metadata is stored in `data.pkg` and `packagesite.pkg` files.
#[derive(Serialize, Deserialize)]
pub struct PackageMeta {
    #[serde(flatten)]
    pub(crate) compact: CompactManifest,
    // sha256
    pub(crate) sum: String,
    pub(crate) path: PathBuf,
    pub(crate) repopath: PathBuf,
    pub(crate) pkgsize: u64,
}

impl PackageMeta {
    pub fn to_vec(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

impl ToString for PackageMeta {
    fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

impl FromStr for PackageMeta {
    type Err = serde_json::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(value)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Dependency {
    origin: String,
    version: String,
}
