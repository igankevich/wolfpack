use std::collections::HashMap;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

use crate::deb::PackageName;
use crate::deb::PackageVersion;

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(test, derive(arbitrary::Arbitrary, PartialEq, Eq))]
pub struct CompactManifest {
    name: PackageName,
    origin: String,
    version: PackageVersion,
    comment: String,
    maintainer: String,
    www: String,
    abi: String,
    arch: String,
    prefix: PathBuf,
    pub flatsize: u32,
    licenselogic: LicenseLogic,
    licenses: Vec<String>,
    desc: String,
    #[serde(default)]
    deps: HashMap<PackageName, Dependency>,
    categories: Vec<String>,
    shlibs_required: Vec<PathBuf>,
    shlibs_provided: Vec<PathBuf>,
    annotations: HashMap<String, String>,
}

impl ToString for CompactManifest {
    fn to_string(&self) -> String {
        // TODO
        serde_json::to_string_pretty(self).unwrap()
    }
}

impl Debug for CompactManifest {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(serde_json::to_string_pretty(self).unwrap().as_str())
    }
}

impl FromStr for CompactManifest {
    type Err = serde_json::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
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
        // TODO
        serde_json::to_string_pretty(self).unwrap()
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
#[cfg_attr(test, derive(arbitrary::Arbitrary, PartialEq, Eq))]
pub struct Dependency {
    origin: String,
    version: String,
}

impl Debug for Dependency {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(serde_json::to_string_pretty(self).unwrap().as_str())
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub enum LicenseLogic {
    Single,
}
