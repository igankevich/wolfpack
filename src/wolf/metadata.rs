use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub homepage: String,
    pub license: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoMetadata {
    pub name: String,
}

impl Default for RepoMetadata {
    fn default() -> Self {
        Self {
            name: "main".into(),
        }
    }
}
