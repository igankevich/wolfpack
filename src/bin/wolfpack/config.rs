use std::collections::BTreeMap;
use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use serde::Serialize;
use wolfpack::deb;

use crate::Error;
use crate::Repo;

pub struct Config {
    pub store_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub repos: BTreeMap<String, RepoConfig>,
    max_age: u64,
}

impl Config {
    pub fn open<P: AsRef<Path>>(config_dir: P) -> Result<Self, Error> {
        match fs_err::read_to_string(config_dir.as_ref().join("config.toml")) {
            Ok(s) => Ok(toml::from_str::<ConfigToml>(&s)?.into()),
            Err(ref e) if e.kind() == ErrorKind::NotFound => Ok(Default::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn take_repos(&mut self) -> BTreeMap<String, Box<dyn Repo>> {
        std::mem::take(&mut self.repos)
            .into_iter()
            .map(|(name, repo_config)| (name, <dyn Repo>::new(repo_config)))
            .collect()
    }

    pub fn database_path(&self) -> PathBuf {
        self.cache_dir.join("wolfpack.sqlite3")
    }

    pub fn packages_index_dir(&self) -> PathBuf {
        let mut path = self.cache_dir.clone();
        path.push("index");
        path.push("packages");
        path
    }

    pub fn files_index_dir(&self) -> PathBuf {
        let mut path = self.cache_dir.clone();
        path.push("index");
        path.push("files");
        path
    }

    pub fn max_age(&self) -> Duration {
        Duration::from_secs(self.max_age)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cache_dir: "/var/cache/wolfpack".into(),
            store_dir: "/wolfpack/store".into(),
            repos: Default::default(),
            max_age: 60 * 60 * 24,
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct ConfigToml {
    pub store_dir: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
    #[serde(rename = "repo", default)]
    pub repos: BTreeMap<String, RepoConfig>,
    max_age: Option<u64>,
}

impl From<ConfigToml> for Config {
    fn from(other: ConfigToml) -> Self {
        let def = Self::default();
        Self {
            store_dir: other.store_dir.unwrap_or(def.store_dir),
            cache_dir: other.cache_dir.unwrap_or(def.cache_dir),
            repos: other.repos,
            max_age: other.max_age.unwrap_or(def.max_age),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "format", rename_all = "lowercase")]
#[serde(deny_unknown_fields)]
pub enum RepoConfig {
    Deb(DebConfig),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct DebConfig {
    pub base_urls: Vec<String>,
    pub suites: Vec<String>,
    pub components: HashSet<deb::SimpleValue>,
    pub public_key_file: PathBuf,
    #[serde(default)]
    pub verify: bool,
}

impl Default for DebConfig {
    fn default() -> Self {
        Self {
            base_urls: Default::default(),
            suites: Default::default(),
            components: Default::default(),
            public_key_file: Default::default(),
            verify: true,
        }
    }
}
