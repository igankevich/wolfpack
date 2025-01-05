use futures_util::StreamExt;
use reqwest::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::io::Error;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs::create_dir_all;
use tokio::io::AsyncWriteExt;
use uname_rs::Uname;
use wolfpack::deb;
use wolfpack::hash::AnyHash;
use wolfpack::hash::Hasher;

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub store_dir: PathBuf,
    pub cache_dir: PathBuf,
    #[serde(rename = "repo")]
    pub repos: BTreeMap<String, RepoConfig>,
}

impl Config {
    pub fn open<P: AsRef<Path>>(config_dir: P) -> Result<Self, Error> {
        match std::fs::read_to_string(config_dir.as_ref().join("config.toml")) {
            Ok(s) => toml::from_str(&s).map_err(Error::other),
            Err(ref e) if e.kind() == ErrorKind::NotFound => Ok(Default::default()),
            Err(e) => Err(e),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cache_dir: "/var/cache/wolfpack".into(),
            store_dir: "/wp/store".into(),
            repos: Default::default(),
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
    pub public_key: String,
}

#[async_trait::async_trait]
pub trait Repo {
    async fn pull(&mut self, prefix: &Path, name: &str) -> Result<(), Error>;
}

impl dyn Repo {
    pub fn new(config: RepoConfig) -> Box<dyn Repo> {
        use RepoConfig::*;
        match config {
            Deb(config) => Box::new(DebRepo::new(config)),
        }
    }
}

struct DebRepo {
    config: DebConfig,
}

impl DebRepo {
    pub fn new(config: DebConfig) -> Self {
        Self { config }
    }

    fn native_arch() -> Result<String, Error> {
        let uts = Uname::new()?;
        match uts.machine.as_str() {
            "x86_64" => Ok("amd64".into()),
            other => Err(Error::other(format!("unsupported architecture: {}", other))),
        }
    }
}

#[async_trait::async_trait]
impl Repo for DebRepo {
    async fn pull(&mut self, prefix: &Path, name: &str) -> Result<(), Error> {
        let arch = Self::native_arch()?;
        for base_url in self.config.base_urls.iter() {
            for suite in self.config.suites.iter() {
                let suite_url = format!("{}/dists/{}", base_url, suite);
                let suite_dir = prefix.join(name).join(suite);
                create_dir_all(&suite_dir).await?;
                let release_file = suite_dir.join("Release");
                download_file(&format!("{}/Release", suite_url), &release_file, None).await?;
                maybe_download_file(
                    &format!("{}/Release.gpg", suite_url),
                    suite_dir.join("Release.gpg"),
                    None,
                )
                .await;
                let release: deb::Release = tokio::fs::read_to_string(&release_file)
                    .await?
                    .parse()
                    .map_err(Error::other)?;
                for component in release.components().intersection(&self.config.components) {
                    let component_dir = suite_dir.join(component.as_str());
                    create_dir_all(&component_dir).await?;
                    for arch in [arch.as_str(), "all"] {
                        let packages_prefix = format!("{}/binary-{}", component, arch);
                        let files = release.get_files(&packages_prefix, "Packages");
                        for (candidate, hash, _file_size) in files.into_iter() {
                            let file_name = candidate.file_name().unwrap();
                            let packages_url = format!(
                                "{}/{}/{}",
                                suite_url,
                                packages_prefix,
                                file_name.to_str().unwrap()
                            );
                            let packages_file = component_dir.join(file_name);
                            match download_file(&packages_url, &packages_file, Some(hash)).await {
                                Ok(..) => break,
                                Err(ref e) if e.kind() == ErrorKind::NotFound => continue,
                                Err(e) => return Err(e),
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

async fn maybe_download_file<P: AsRef<Path>>(url: &str, path: P, hash: Option<AnyHash>) {
    let _ = download_file(url, path, hash).await;
}

async fn download_file<P: AsRef<Path>>(
    url: &str,
    path: P,
    hash: Option<AnyHash>,
) -> Result<(), Error> {
    log::info!("Downloading {} to {}", url, path.as_ref().display());
    do_download_file(url, path, hash)
        .await
        .inspect_err(|e| log::error!("Failed to download {}: {}", url, e))
}

async fn do_download_file<P: AsRef<Path>>(
    url: &str,
    path: P,
    hash: Option<AnyHash>,
) -> Result<(), Error> {
    // TODO etag, if-match
    // TODO last-modified, If-Modified-Since
    // TODO user-agent
    let response = reqwest::get(url)
        .await
        .map_err(Error::other)?
        .error_for_status()
        .map_err(|e| {
            if e.status() == Some(StatusCode::NOT_FOUND) {
                ErrorKind::NotFound.into()
            } else {
                Error::other(e)
            }
        })?;
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(path).await?;
    let mut hasher = hash.as_ref().map(|h| h.hasher());
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(Error::other)?;
        if let Some(ref mut hasher) = hasher {
            hasher.update(&chunk);
        }
        file.write_all(&chunk).await?;
    }
    if let (Some(hash), Some(hasher)) = (hash, hasher) {
        let actual_hash = hasher.finalize();
        if hash != actual_hash {
            return Err(Error::other("hash mismatch"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_gen() {
        let config = Config {
            store_dir: "/wolfpack".into(),
            repos: [
                (
                    "debian".into(),
                    RepoConfig::Deb(DebConfig {
                        base_urls: vec!["https://deb.debian.org/debian".into()],
                        suites: vec!["bookworm".into(), "bookworm-updates".into()],
                        components: ["main".try_into().unwrap()].into(),
                        public_key: "".into(), // TODO
                    }),
                ),
                (
                    "debian-security".into(),
                    RepoConfig::Deb(DebConfig {
                        base_urls: vec!["https://deb.debian.org/debian-security".into()],
                        suites: vec!["bookworm-security".into()],
                        components: ["main".try_into().unwrap()].into(),
                        public_key: "".into(), // TODO
                    }),
                ),
            ]
            .into(),
        };
        eprintln!("{}", toml::to_string(&config).unwrap());
    }

    #[test]
    fn test_date() {
        let now = std::time::SystemTime::now();
        let now: chrono::DateTime<chrono::Utc> = now.into();
        eprintln!("now {}", now.to_rfc2822());
        let s = "Sat, 09 Nov 2024 10:10:58 UTC";
        chrono::DateTime::parse_from_rfc2822(s).unwrap();
    }
}
