use futures_util::StreamExt;
use reqwest::header::HeaderValue;
use reqwest::header::IF_MATCH;
use reqwest::header::USER_AGENT;
use reqwest::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs::File;
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
use wolfpack::sign::VerifierV2;

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub store_dir: PathBuf,
    #[serde(default)]
    pub cache_dir: PathBuf,
    #[serde(rename = "repo", default)]
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
                if self.config.verify {
                    let release_gpg_file = suite_dir.join("Release.gpg");
                    download_file(
                        &format!("{}/Release.gpg", suite_url),
                        &release_gpg_file,
                        None,
                    )
                    .await?;
                    let message = std::fs::read(&release_file)?;
                    let signature =
                        deb::Signature::read_armored_one(File::open(&release_gpg_file)?)?;
                    let verifying_keys = deb::VerifyingKey::read_binary_all(File::open(
                        &self.config.public_key_file,
                    )?)?;
                    deb::VerifyingKey::verify_against_any(
                        verifying_keys.iter(),
                        &message,
                        &signature,
                    )
                    .map_err(|_| {
                        Error::other(format!("Failed to verify {}", release_gpg_file.display()))
                    })?;
                    log::info!(
                        "Verified {} against {}",
                        release_file.display(),
                        release_gpg_file.display()
                    );
                }
                let release: deb::Release = tokio::fs::read_to_string(&release_file)
                    .await?
                    .parse()
                    .map_err(Error::other)?;
                for component in release.components().intersection(&self.config.components) {
                    let component_dir = suite_dir.join(component.as_str());
                    for arch in [arch.as_str(), "all"] {
                        let packages_prefix = format!("{}/binary-{}", component, arch);
                        let files = release.get_files(&packages_prefix, "Packages");
                        let arch_dir = component_dir.join(format!("binary-{}", arch));
                        create_dir_all(&arch_dir).await?;
                        for (candidate, hash, _file_size) in files.into_iter() {
                            let file_name = candidate.file_name().unwrap();
                            let packages_url = format!(
                                "{}/{}/{}",
                                suite_url,
                                packages_prefix,
                                file_name.to_str().unwrap()
                            );
                            let packages_file = arch_dir.join(file_name);
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

async fn download_file<P: AsRef<Path>>(
    url: &str,
    path: P,
    hash: Option<AnyHash>,
) -> Result<(), Error> {
    do_download_file(url, path, hash)
        .await
        .inspect_err(|e| log::error!("Failed to download {}: {}", url, e))
}

async fn do_download_file<P: AsRef<Path>>(
    url: &str,
    path: P,
    hash: Option<AnyHash>,
) -> Result<(), Error> {
    // TODO last-modified, If-Modified-Since
    let path = path.as_ref();
    let etag_path = path.parent().unwrap().join(format!(
        ".{}.etag",
        path.file_name().unwrap().to_str().unwrap()
    ));
    let etag = match std::fs::read(&etag_path) {
        Ok(etag) => etag,
        Err(e) if e.kind() == ErrorKind::NotFound => Default::default(),
        Err(e) => return Err(e),
    };
    let client = reqwest::Client::builder().build().map_err(Error::other)?;
    if !etag.is_empty() && path.exists() {
        let response = client
            .head(url)
            .header(USER_AGENT, &WOLFPACK_UA)
            .header(
                IF_MATCH,
                HeaderValue::from_bytes(&etag).map_err(Error::other)?,
            )
            .send()
            .await
            .map_err(Error::other)?;
        if response.status() != StatusCode::PRECONDITION_FAILED {
            log::info!("Up-to-date {}", url);
            // Up-to-date.
            return Ok(());
        }
    }
    log::info!("Downloading {} to {}", url, path.display());
    let response = client
        .get(url)
        .header(USER_AGENT, &WOLFPACK_UA)
        .send()
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
    if let Some(etag) = response.headers().get("ETag") {
        std::fs::write(&etag_path, etag.as_bytes())?;
    }
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

const WOLFPACK_UA: HeaderValue =
    HeaderValue::from_static(concat!("Wolfpack/", env!("CARGO_PKG_VERSION")));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_gen() {
        let config = Config {
            store_dir: "/wolfpack".into(),
            cache_dir: "/tmp/wolfpack".into(),
            repos: [
                (
                    "debian".into(),
                    RepoConfig::Deb(DebConfig {
                        base_urls: vec!["https://deb.debian.org/debian".into()],
                        suites: vec!["bookworm".into(), "bookworm-updates".into()],
                        components: ["main".try_into().unwrap()].into(),
                        public_key_file: "".into(), // TODO
                        verify: true,
                    }),
                ),
                (
                    "debian-security".into(),
                    RepoConfig::Deb(DebConfig {
                        base_urls: vec!["https://deb.debian.org/debian-security".into()],
                        suites: vec!["bookworm-security".into()],
                        components: ["main".try_into().unwrap()].into(),
                        public_key_file: "".into(), // TODO
                        verify: true,
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
