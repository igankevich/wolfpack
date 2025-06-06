use std::path::PathBuf;

use crate::deb;
use crate::Config;
use crate::Error;
use crate::RepoConfig;
use crate::SearchBy;

#[async_trait::async_trait]
pub trait Repo {
    async fn pull(&mut self, config: &Config, name: &str) -> Result<(), Error>;

    async fn install(
        &mut self,
        config: &Config,
        name: &str,
        packages: Vec<String>,
    ) -> Result<(), Error>;

    async fn download(
        &mut self,
        config: &Config,
        name: &str,
        packages: Vec<String>,
    ) -> Result<Vec<PathBuf>, Error>;

    fn search(
        &mut self,
        config: &Config,
        name: &str,
        by: SearchBy,
        keyword: &str,
    ) -> Result<(), Error>;

    fn resolve(
        &mut self,
        config: &Config,
        name: &str,
        dependencies: Vec<String>,
    ) -> Result<(), Error>;
}

impl dyn Repo {
    pub fn new(config: RepoConfig) -> Box<dyn Repo> {
        use RepoConfig::*;
        match config {
            Deb(config) => Box::new(deb::DebRepo::new(config)),
        }
    }
}
