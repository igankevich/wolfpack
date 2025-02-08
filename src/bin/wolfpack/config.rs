use deko::bufread::AnyDecoder;
use elf::abi::EI_NIDENT;
use elf::abi::ET_DYN;
use elf::abi::ET_EXEC;
use elf::endian::AnyEndian;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::fs::create_dir_all;
use std::fs::read_to_string;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::io::Read;
use std::ops::RangeInclusive;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;
use uname_rs::Uname;
use wolfpack::deb;
use wolfpack::sign::VerifierV2;

use crate::download_file;
use crate::print_table;
use crate::Connection;
use crate::ConnectionArc;
use crate::DebDependencyMatch;
use crate::DebMatch;
use crate::Error;
use crate::Id;
use crate::Row;
use crate::ToRow;

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub store_dir: PathBuf,
    #[serde(default)]
    pub cache_dir: PathBuf,
    #[serde(rename = "repo", default)]
    pub repos: BTreeMap<String, RepoConfig>,
    #[serde(default)]
    pub max_age: u64,
}

impl Config {
    pub fn open<P: AsRef<Path>>(config_dir: P) -> Result<Self, Error> {
        match std::fs::read_to_string(config_dir.as_ref().join("config.toml")) {
            Ok(s) => Ok(toml::from_str(&s)?),
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
        self.cache_dir.join("cache.sqlite3")
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cache_dir: "/var/cache/wolfpack".into(),
            store_dir: "/wp/store".into(),
            repos: Default::default(),
            max_age: 60 * 60 * 24 * 365,
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
            other => Err(Error::UnsupportedArchitecture(other.into())),
        }
    }

    fn index_packages(
        packages_file: &Path,
        arch_dir: &Path,
        base_url: String,
        component_id: Id,
        db_conn: ConnectionArc,
    ) -> Result<(), Error> {
        let mut packages_str = String::new();
        let mut file = AnyDecoder::new(BufReader::new(File::open(packages_file)?));
        file.read_to_string(&mut packages_str)?;
        let packages: deb::PerArchPackages = packages_str.parse()?;
        let packages = packages.into_inner();
        for package in packages.iter() {
            let url = format!("{}/{}", base_url, package.filename.display());
            let package_file = arch_dir.join(&package.filename);
            db_conn
                .lock()
                .insert_deb_package(package, &url, &package_file, component_id)?;
        }
        log::info!("Indexed {:?}", packages_file);
        Ok(())
    }
}

#[async_trait::async_trait]
impl Repo for DebRepo {
    async fn pull(&mut self, config: &Config, name: &str) -> Result<(), Error> {
        // TODO dynamic thread count
        let threads = threadpool::Builder::new()
            .thread_name("Indexer".into())
            .build();
        let db_conn = Connection::new(config)?;
        let arch = Self::native_arch()?;
        #[allow(clippy::never_loop)]
        for base_url in self.config.base_urls.iter() {
            for suite in self.config.suites.iter() {
                let suite_url = format!("{}/dists/{}", base_url, suite);
                let suite_dir = config.cache_dir.join(name).join(suite);
                create_dir_all(&suite_dir)?;
                let release_file = suite_dir.join("Release");
                download_file(
                    &format!("{}/Release", suite_url),
                    &release_file,
                    None,
                    db_conn.clone(),
                    config,
                )
                .await?;
                if self.config.verify {
                    let release_gpg_file = suite_dir.join("Release.gpg");
                    download_file(
                        &format!("{}/Release.gpg", suite_url),
                        &release_gpg_file,
                        None,
                        db_conn.clone(),
                        config,
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
                    .map_err(|_| Error::Verify(release_gpg_file.clone()))?;
                    log::info!(
                        "Verified {} against {}",
                        release_file.display(),
                        release_gpg_file.display()
                    );
                }
                let release: deb::Release = read_to_string(&release_file)?.parse()?;
                for component in release.components().intersection(&self.config.components) {
                    let component_dir = suite_dir.join(component.as_str());
                    for arch in [arch.as_str(), "all"] {
                        let packages_prefix = format!("{}/binary-{}", component, arch);
                        let files = release.get_files(&packages_prefix, "Packages");
                        let arch_dir = component_dir.join(format!("binary-{}", arch));
                        create_dir_all(&arch_dir)?;
                        for (candidate, hash, _file_size) in files.into_iter() {
                            let file_name = candidate.file_name().ok_or(ErrorKind::InvalidData)?;
                            let component_url = format!("{}/{}", suite_url, packages_prefix);
                            let component_id = db_conn.lock().insert_deb_component(
                                &component_url,
                                name,
                                base_url,
                                suite,
                                component.as_str(),
                                arch,
                            )?;
                            let packages_url = format!(
                                "{}/{}",
                                component_url,
                                file_name.to_str().ok_or(ErrorKind::InvalidData)?,
                            );
                            let packages_file = arch_dir.join(file_name);
                            match download_file(
                                &packages_url,
                                &packages_file,
                                Some(hash),
                                db_conn.clone(),
                                config,
                            )
                            .await
                            {
                                Ok(..) => {
                                    let db_conn = db_conn.clone();
                                    let base_url = base_url.into();
                                    threads.execute(move || {
                                        if let Err(e) = Self::index_packages(
                                            &packages_file,
                                            &arch_dir,
                                            base_url,
                                            component_id,
                                            db_conn,
                                        ) {
                                            log::error!("Failed to index packages: {}", e)
                                        }
                                    });
                                    break;
                                }
                                Err(Error::ResourceNotFound(..)) => continue,
                                Err(e) => return Err(e),
                            }
                        }
                    }
                }
            }
            // TODO Only one URL is used.
            break;
        }
        threads.join();
        Ok(())
    }

    async fn install(
        &mut self,
        config: &Config,
        name: &str,
        packages: Vec<String>,
    ) -> Result<(), Error> {
        let db_conn = Connection::new(config)?;
        let mut matches: HashMap<String, Vec<DebDependencyMatch>> = Default::default();
        for package_name in packages.iter() {
            let candidates = db_conn
                .lock()
                .find_deb_packages_by_name(name, package_name)?
                .into_iter()
                .collect();
            matches.insert(package_name.clone(), candidates);
        }
        for (package_name, mut matches) in matches.into_iter() {
            for (i, package) in matches.iter().enumerate() {
                println!(
                    "{}. {}  -  {}  -  {}",
                    i + 1,
                    package.filename.display(),
                    package.version,
                    package
                        .description
                        .as_str()
                        .lines()
                        .next()
                        .unwrap_or_default()
                );
            }
            match matches.len() {
                0 => {
                    return Err(Error::NotFound(package_name));
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
            //dependencies.extend(matches[0].0.inner.pre_depends.clone().into_inner());
            dependencies.extend(matches[0].depends.clone().into_inner());
            let mut visited = HashSet::new();
            'outer: while let Some(dep) = dependencies.pop_front() {
                log::info!("Resolving {}", dep);
                let t = Instant::now();
                let mut candidates = db_conn.lock().select_deb_dependencies(name, &dep)?;
                log::info!("{}s", t.elapsed().as_secs_f32());
                if candidates.is_empty() {
                    return Err(Error::DependencyNotFound(dep.to_string()));
                }
                if candidates.len() > 1 {
                    let unique_names = candidates.iter().map(|p| &p.name).collect::<HashSet<_>>();
                    if unique_names.len() > 1 {
                        for package in candidates.iter() {
                            if visited.contains(&package.hash) {
                                // We already made the decision to install this package.
                                continue 'outer;
                            }
                        }
                        for (i, package) in candidates.iter().enumerate() {
                            println!(
                                "{}. {}  -  {}  -  {}",
                                i + 1,
                                package.name,
                                package.version,
                                package
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
                        candidates.sort_unstable_by(|a, b| b.version.cmp(&a.version));
                        candidates.drain(1..);
                    }
                }
                // Recurse into dependencies of the dependency.
                for package in candidates.into_iter() {
                    // TODO unique `dependencies`
                    if visited.insert(package.hash.clone()) {
                        log::info!("Recurse into {}", package.name);
                        dependencies.extend(package.depends.clone().into_inner());
                        matches.push(package);
                    }
                }
            }
            log::info!("Installing...");
            // Install in topological (reverse) order.
            for package in matches.into_iter().rev() {
                if let Some(dirname) = package.filename.parent() {
                    create_dir_all(dirname)?;
                }
                match download_file(
                    &package.url,
                    &package.filename,
                    package.hash.clone(),
                    db_conn.clone(),
                    config,
                )
                .await
                {
                    Ok(..) => {
                        let verifier = deb::PackageVerifier::none();
                        let (_control, data) =
                            deb::Package::read(File::open(&package.filename)?, &verifier)?;
                        log::info!("Installing {}", package.filename.display());
                        let mut tar_archive = tar::Archive::new(AnyDecoder::new(&data[..]));
                        let dst = config.store_dir.join(name);
                        create_dir_all(&dst)?;
                        tar_archive.unpack(&dst)?;
                        drop(tar_archive);
                        let mut tar_archive = tar::Archive::new(AnyDecoder::new(&data[..]));
                        for entry in tar_archive.entries()? {
                            let entry = entry?;
                            let path = dst.join(entry.path()?);
                            if get_elf_type(&path).is_some() {
                                log::info!("patching {:?}", path);
                                let status = Command::new("./patchelf.sh")
                                    .arg(&path)
                                    .arg(&dst)
                                    .status()?;
                                if !status.success() {
                                    return Err(Error::Patch(path));
                                }
                            }
                        }
                    }
                    Err(..) => continue,
                }
            }
        }
        Ok(())
    }

    fn search(&mut self, config: &Config, name: &str, keyword: &str) -> Result<(), Error> {
        let db_conn = Connection::new(config)?;
        let architecture = Self::native_arch()?;
        let matches = db_conn
            .lock()
            .find_deb_packages(name, &architecture, keyword)?;
        print_table(matches.iter(), std::io::stdout())?;
        Ok(())
    }
}

fn get_elf_type(path: &Path) -> Option<ElfType> {
    let mut file = File::open(path).ok()?;
    let mut buf = [0; 64];
    let n = file.read(&mut buf[..]).ok()?;
    let buf = &mut buf[..n];
    drop(file);
    if buf.len() < 4 {
        return None;
    }
    let ident = elf::file::parse_ident::<AnyEndian>(buf).ok()?;
    let header = elf::file::FileHeader::<AnyEndian>::parse_tail(ident, &buf[EI_NIDENT..]).ok()?;
    match header.e_type {
        ET_EXEC => Some(ElfType::Executable),
        ET_DYN => Some(ElfType::Library),
        _ => None,
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

impl ToRow<3> for DebMatch {
    fn to_row(&self) -> Row<'_, 3> {
        Row([
            self.name.as_str().into(),
            self.version.as_str().into(),
            self.description
                .as_str()
                .lines()
                .next()
                .map(|line| line.trim())
                .unwrap_or_default()
                .into(),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_gen() {
        let config = Config {
            store_dir: "/wolfpack".into(),
            cache_dir: "/tmp/wolfpack".into(),
            max_age: 1000,
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
