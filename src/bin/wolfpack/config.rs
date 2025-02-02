use deko::bufread::AnyDecoder;
use elf::abi::EI_NIDENT;
use elf::abi::ET_DYN;
use elf::abi::ET_EXEC;
use elf::endian::AnyEndian;
use futures_util::StreamExt;
use reqwest::header::HeaderValue;
use reqwest::header::ETAG;
use reqwest::header::IF_NONE_MATCH;
use reqwest::header::USER_AGENT;
use reqwest::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Error;
use std::io::ErrorKind;
use std::io::ErrorKind::InvalidData;
use std::io::Read;
use std::ops::RangeInclusive;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
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

    pub fn take_repos(&mut self) -> BTreeMap<String, Box<dyn Repo>> {
        std::mem::take(&mut self.repos)
            .into_iter()
            .map(|(name, repo_config)| (name, <dyn Repo>::new(repo_config)))
            .collect()
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
    async fn pull(&mut self, config: &Config, name: &str) -> Result<(), Error>;

    async fn install(
        &mut self,
        config: &Config,
        name: &str,
        packages: Vec<String>,
    ) -> Result<(), Error>;

    fn search(&mut self, config: &Config, name: &str, keyword: &str) -> Result<(), Error>;
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

    fn for_each_packages_db<F>(
        &self,
        config: &Config,
        name: &str,
        mut callback: F,
    ) -> Result<(), Error>
    where
        F: FnMut(deb::PerArchPackages, &Path) -> Result<(), Error>,
    {
        let native_arch = Self::native_arch()?;
        for suite in self.config.suites.iter() {
            let suite_dir = config.cache_dir.join(name).join(suite);
            let release_file = suite_dir.join("Release");
            let release: deb::Release = std::fs::read_to_string(&release_file)?
                .parse()
                .map_err(Error::other)?;
            for component in release.components().intersection(&self.config.components) {
                let component_dir = suite_dir.join(component.as_str());
                for arch in [native_arch.as_str(), "all"] {
                    let arch_dir = component_dir.join(format!("binary-{}", arch));
                    let packages_prefix = format!("{}/binary-{}", component, arch);
                    let files = release.get_files(&packages_prefix, "Packages");
                    // Read the first file only. This should be the one with the highest
                    // compression ratio.
                    if let Some((candidate, _hash, _file_size)) = files.into_iter().next() {
                        let file_name = candidate.file_name().ok_or(InvalidData)?;
                        let packages_file = arch_dir.join(file_name);
                        let mut packages_str = String::new();
                        let file = match File::open(&packages_file) {
                            Ok(file) => file,
                            Err(ref e) if e.kind() == ErrorKind::NotFound => continue,
                            Err(e) => return Err(e),
                        };
                        let mut file = AnyDecoder::new(BufReader::new(file));
                        file.read_to_string(&mut packages_str)?;
                        let packages: deb::PerArchPackages =
                            packages_str.parse().map_err(Error::other)?;
                        callback(packages, arch_dir.as_path())?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Repo for DebRepo {
    async fn pull(&mut self, config: &Config, name: &str) -> Result<(), Error> {
        let arch = Self::native_arch()?;
        #[allow(clippy::never_loop)]
        for base_url in self.config.base_urls.iter() {
            for suite in self.config.suites.iter() {
                let suite_url = format!("{}/dists/{}", base_url, suite);
                let suite_dir = config.cache_dir.join(name).join(suite);
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
                            let file_name = candidate.file_name().ok_or(ErrorKind::InvalidData)?;
                            let packages_url = format!(
                                "{}/{}/{}",
                                suite_url,
                                packages_prefix,
                                file_name.to_str().ok_or(ErrorKind::InvalidData)?,
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
            // TODO Only one URL is used.
            break;
        }
        Ok(())
    }

    async fn install(
        &mut self,
        config: &Config,
        name: &str,
        packages: Vec<String>,
    ) -> Result<(), Error> {
        let mut matches: HashMap<String, Vec<(deb::ExtendedPackage, PathBuf)>> = Default::default();
        self.for_each_packages_db(config, name, |packages_db, arch_dir| {
            for package_name in packages.iter() {
                matches.entry(package_name.to_string()).or_default().extend(
                    packages_db
                        .find_by_name(package_name)
                        .into_iter()
                        .map(|package| {
                            let package_file = arch_dir.join(&package.filename);
                            (package, package_file)
                        }),
                );
            }
            Ok(())
        })?;
        for (package_name, mut matches) in matches.into_iter() {
            for (i, (package, package_file)) in matches.iter().enumerate() {
                println!(
                    "{}. {}  -  {}  -  {}",
                    i + 1,
                    package_file.display(),
                    package.inner.version,
                    package
                        .inner
                        .description
                        .as_str()
                        .lines()
                        .next()
                        .unwrap_or_default()
                );
            }
            match matches.len() {
                0 => {
                    return Err(Error::other(format!(
                        "Package `{}` not found",
                        package_name
                    )));
                }
                1 => {
                    // Ok. One match.
                }
                n => {
                    // Multiple matches.
                    loop {
                        use std::io::Write;
                        print!("Which package do you want to install? Number: ");
                        std::io::stdout().lock().flush()?;
                        let mut line = String::new();
                        std::io::stdin().lock().read_line(&mut line)?;
                        let Ok(i) = line.trim().parse() else {
                            continue;
                        };
                        if !(1..=n).contains(&i) {
                            continue;
                        }
                        let x = matches.remove(i - 1);
                        matches.clear();
                        matches.push(x);
                        break;
                    }
                }
            }
            // Add dependencies.
            let mut dependencies = VecDeque::new();
            dependencies.extend(matches[0].0.inner.pre_depends.clone().into_inner());
            dependencies.extend(matches[0].0.inner.depends.clone().into_inner());
            let mut visited = HashSet::new();
            self.for_each_packages_db(config, name, |packages, arch_dir| {
                while let Some(dep) = dependencies.pop_front() {
                    log::info!("Resolving {}", dep);
                    let mut candidates = packages.find_dependency(&dep);
                    if candidates.is_empty() {
                        return Err(Error::other(format!(
                            "Failed to resolve dependency: {}",
                            dep
                        )));
                    }
                    if candidates.len() > 1 {
                        let unique_names = candidates
                            .iter()
                            .map(|p| &p.inner.name)
                            .collect::<HashSet<_>>();
                        if unique_names.len() > 1 {
                            for (i, package) in candidates.iter().enumerate() {
                                println!(
                                    "{}. {}  -  {}  -  {}",
                                    i + 1,
                                    package.inner.name,
                                    package.inner.version,
                                    package
                                        .inner
                                        .description
                                        .as_str()
                                        .lines()
                                        .next()
                                        .unwrap_or_default()
                                );
                            }
                            let i = ask_number(
                                "Which dependency do you want to install? Number: ",
                                1..=candidates.len(),
                            )?;
                            let x = candidates.remove(i - 1);
                            candidates.clear();
                            candidates.push(x);
                        } else {
                            // Highest version goes first.
                            candidates
                                .sort_unstable_by(|a, b| b.inner.version.cmp(&a.inner.version));
                            candidates.drain(1..);
                        }
                    }
                    // Recurse into dependencies of the dependency.
                    for package in candidates.into_iter() {
                        let hash = package.hash();
                        if visited.insert(hash) {
                            log::info!("Recurse into {}", package.inner.name);
                            dependencies.extend(package.inner.pre_depends.clone().into_inner());
                            dependencies.extend(package.inner.depends.clone().into_inner());
                            let package_file = arch_dir.join(&package.filename);
                            matches.push((package, package_file));
                        }
                    }
                }
                Ok(())
            })?;
            log::info!("Installing...");
            // Install in topological (reverse) order.
            for (package, package_file) in matches.into_iter().rev() {
                for base_url in self.config.base_urls.iter() {
                    let package_url = format!("{}/{}", base_url, package.filename.display());
                    if let Some(dirname) = package_file.parent() {
                        create_dir_all(dirname).await?;
                    }
                    match download_file(&package_url, &package_file, package.hash()).await {
                        Ok(..) => {
                            let verifier = deb::PackageVerifier::none();
                            let (_control, data) =
                                deb::Package::read(File::open(&package_file)?, &verifier)
                                    .map_err(Error::other)?;
                            log::info!("Installing {}", package_file.display());
                            let mut tar_archive = tar::Archive::new(AnyDecoder::new(&data[..]));
                            let dst = config.store_dir.join(name);
                            create_dir_all(&dst).await?;
                            tar_archive.unpack(&dst)?;
                            drop(tar_archive);
                            let mut tar_archive = tar::Archive::new(AnyDecoder::new(&data[..]));
                            for entry in tar_archive.entries()? {
                                let entry = entry?;
                                let path = dst.join(entry.path()?);
                                match get_elf_type(&path) {
                                    Ok(..) => {
                                        log::info!("patching {:?}", path);
                                        let status = Command::new("./patchelf.sh")
                                            .arg(&path)
                                            .arg(&dst)
                                            .status()?;
                                        if !status.success() {
                                            return Err(Error::other(format!(
                                                "Failed to patch {:?}",
                                                path
                                            )));
                                        }
                                    }
                                    _ => {
                                        // TODO
                                    }
                                }
                            }
                            break;
                        }
                        Err(..) => continue,
                    }
                }
            }
        }
        Ok(())
    }

    fn search(&mut self, config: &Config, name: &str, keyword: &str) -> Result<(), Error> {
        let arch = Self::native_arch()?;
        for suite in self.config.suites.iter() {
            let suite_dir = config.cache_dir.join(name).join(suite);
            let release_file = suite_dir.join("Release");
            let release: deb::Release = std::fs::read_to_string(&release_file)?
                .parse()
                .map_err(Error::other)?;
            for component in release.components().intersection(&self.config.components) {
                let component_dir = suite_dir.join(component.as_str());
                for arch in [arch.as_str(), "all"] {
                    let arch_dir = component_dir.join(format!("binary-{}", arch));
                    let packages_prefix = format!("{}/binary-{}", component, arch);
                    let files = release.get_files(&packages_prefix, "Packages");
                    for (candidate, _hash, _file_size) in files.into_iter() {
                        let file_name = candidate.file_name().ok_or(InvalidData)?;
                        let packages_file = arch_dir.join(file_name);
                        let mut packages_str = String::new();
                        let file = match File::open(&packages_file) {
                            Ok(file) => file,
                            Err(ref e) if e.kind() == ErrorKind::NotFound => continue,
                            Err(e) => return Err(e),
                        };
                        let mut file = AnyDecoder::new(BufReader::new(file));
                        file.read_to_string(&mut packages_str)?;
                        let packages: deb::PerArchPackages =
                            packages_str.parse().map_err(Error::other)?;
                        let matches = packages.find(keyword);
                        if !matches.is_empty() {
                            println!(
                                "Source {} / {} / {} / {:?}",
                                name, suite, component, packages_file
                            );
                        }
                        for package in matches {
                            println!(
                                "{}  -  {}  -  {}  -  {}",
                                package.inner.name,
                                package.inner.version,
                                package.hash().map(|h| h.to_string()).unwrap_or_default(),
                                package
                                    .inner
                                    .description
                                    .as_str()
                                    .lines()
                                    .next()
                                    .unwrap_or_default()
                            );
                        }
                        break;
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
    let etag_path = path.parent().ok_or(InvalidData)?.join(format!(
        ".{}.etag",
        path.file_name()
            .ok_or(InvalidData)?
            .to_str()
            .ok_or(InvalidData)?
    ));
    let etag = match std::fs::read(&etag_path) {
        Ok(etag) => etag,
        Err(e) if e.kind() == ErrorKind::NotFound => Default::default(),
        Err(e) => return Err(e),
    };
    let client = reqwest::Client::builder().build().map_err(Error::other)?;
    log::info!("Downloading {} to {}", url, path.display());
    let builder = client.get(url).header(USER_AGENT, &WOLFPACK_UA);
    let builder = if !etag.is_empty() && path.exists() {
        builder.header(
            IF_NONE_MATCH,
            HeaderValue::from_bytes(&etag).map_err(Error::other)?,
        )
    } else {
        builder
    };
    let response = builder.send().await.map_err(Error::other)?;
    if response.status() == StatusCode::NOT_MODIFIED {
        log::info!("Up-to-date {}", url);
        // Up-to-date.
        return Ok(());
    }
    let response = response.error_for_status().map_err(|e| {
        if e.status() == Some(StatusCode::NOT_FOUND) {
            ErrorKind::NotFound.into()
        } else {
            Error::other(e)
        }
    })?;
    if let Some(etag) = response.headers().get(ETAG) {
        std::fs::write(&etag_path, etag.as_bytes())?;
    }
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&path).await?;
    let mut hasher = hash.as_ref().map(|h| h.hasher());
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(Error::other)?;
        if let Some(ref mut hasher) = hasher {
            hasher.update(&chunk);
        }
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    if let (Some(hash), Some(hasher)) = (hash, hasher) {
        let actual_hash = hasher.finalize();
        if hash != actual_hash {
            tokio::fs::remove_file(&path).await?;
            return Err(Error::other("Hash mismatch"));
        }
    }
    Ok(())
}

fn get_elf_type(path: &Path) -> Result<ElfType, Error> {
    let mut file = File::open(path)?;
    let mut buf = [0; 64];
    let n = file.read(&mut buf[..])?;
    let buf = &mut buf[..n];
    drop(file);
    let ident = elf::file::parse_ident::<AnyEndian>(buf).map_err(Error::other)?;
    let header = elf::file::FileHeader::<AnyEndian>::parse_tail(ident, &buf[EI_NIDENT..])
        .map_err(Error::other)?;
    match header.e_type {
        ET_EXEC => Ok(ElfType::Executable),
        ET_DYN => Ok(ElfType::Library),
        t => Err(Error::other(format!("unknown elf type: {}", t))),
    }
}

enum ElfType {
    Executable,
    Library,
}

fn ask_number(prompt: &str, valid_range: RangeInclusive<usize>) -> Result<usize, Error> {
    loop {
        use std::io::Write;
        print!("{}", prompt);
        std::io::stdout().lock().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        let Ok(i) = line.trim().parse() else {
            continue;
        };
        if valid_range.contains(&i) {
            return Ok(i);
        }
    }
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
