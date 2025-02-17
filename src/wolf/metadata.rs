use serde::Deserialize;
use serde::Serialize;

use crate::wolf::Arch;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub homepage: String,
    pub license: String,
    pub arch: Arch,
}
