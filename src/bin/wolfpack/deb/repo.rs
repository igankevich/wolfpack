use command_error::CommandExt;
use deko::bufread::AnyDecoder;
use elf::abi::EI_NIDENT;
use elf::abi::ET_DYN;
use elf::abi::ET_EXEC;
use elf::endian::AnyEndian;
use indicatif::MultiProgress;
use indicatif::ProgressBar;
use indicatif::ProgressDrawTarget;
use indicatif::ProgressStyle;
use parking_lot::Mutex;
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
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;
use uname_rs::Uname;
use wolfpack::deb;
use wolfpack::sign::VerifierV2;

use crate::db::Connection;
use crate::db::ConnectionArc;
use crate::db::Id;
use crate::deb::DebDependencyMatch;
use crate::deb::DebMatch;
use crate::download_file;
use crate::print_table;
use crate::Config;
use crate::DebConfig;
use crate::Error;
use crate::Repo;
use crate::Row;
use crate::ToRow;

pub struct DebRepo {
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

    #[allow(clippy::too_many_arguments)]
    fn index_packages(
        packages_file: &Path,
        arch_dir: &Path,
        base_url: String,
        component_id: Id,
        db_conn: ConnectionArc,
        dependency_resolution_tasks: Arc<Mutex<Vec<Task>>>,
        repo_name: String,
        indexing_progress_bar: Arc<Mutex<ProgressBar>>,
        progress_bar: Arc<Mutex<ProgressBar>>,
    ) -> Result<(), Error> {
        let mut packages_str = String::new();
        let mut file = AnyDecoder::new(BufReader::new(File::open(packages_file)?));
        file.read_to_string(&mut packages_str)?;
        let packages: deb::PerArchPackages = packages_str.parse()?;
        let packages = packages.into_inner();
        indexing_progress_bar
            .lock()
            .inc_length(packages.len() as u64);
        // Insert the packages into the database.
        for package in packages.iter() {
            let url = format!("{}/{}", base_url, package.filename.display());
            let package_file = arch_dir.join(&package.filename);
            db_conn
                .lock()
                .insert_deb_package(package, &url, &package_file, component_id)?;
            indexing_progress_bar.lock().inc(1);
        }
        //log::info!("Indexed {:?}", packages_file);
        // Resolve dependencies in batches.
        progress_bar.lock().inc_length(packages.len() as u64);
        let batch_size = 1_000;
        let mut packages = packages;
        while !packages.is_empty() {
            let batch = packages.split_off(packages.len() - batch_size.min(packages.len()));
            let repo_name = repo_name.clone();
            let db_conn = db_conn.clone();
            let progress_bar = progress_bar.clone();
            dependency_resolution_tasks.lock().push(Box::new(move || {
                if let Err(e) = Self::resolve_dependencies(
                    &batch,
                    repo_name.clone(),
                    db_conn.clone(),
                    progress_bar.clone(),
                ) {
                    log::error!("Failed to resolve dependencies: {e}");
                }
            }));
        }
        Ok(())
    }

    fn resolve_dependencies(
        packages: &[deb::ExtendedPackage],
        repo_name: String,
        db_conn: ConnectionArc,
        progress_bar: Arc<Mutex<ProgressBar>>,
    ) -> Result<(), Error> {
        // Per-task read-only connection to make queries in parallel.
        let ro_conn = db_conn.lock().clone_read_only()?;
        let ro_conn = ro_conn.lock();
        for package in packages.iter() {
            for dep in package.inner.depends.iter() {
                let matches = ro_conn.select_deb_dependencies(&repo_name, dep)?;
                if matches.len() != 1 {
                    continue;
                }
                db_conn.lock().insert_deb_dependency(
                    &repo_name,
                    &package.inner.name,
                    matches[0].id,
                )?;
            }
            progress_bar.lock().inc(1);
        }
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
        let progress = MultiProgress::new();
        let downloading_progress_bar = Arc::new(Mutex::new(
            progress.add(
                ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::stderr())
                    .with_message("Downloading metadata")
                    .with_style(
                        ProgressStyle::with_template(
                            "{msg} {wide_bar} {binary_bytes}/{binary_total_bytes}",
                        )
                        .expect("Template is correct"),
                    ),
            ),
        ));
        let indexing_progress_bar = Arc::new(Mutex::new(
            progress.add(
                ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::stderr())
                    .with_message("Indexing packages")
                    .with_style(
                        ProgressStyle::with_template("{msg} {wide_bar} {pos}/{len}")
                            .expect("Template is correct"),
                    ),
            ),
        ));
        let progress_bar = Arc::new(Mutex::new(
            ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::hidden())
                .with_message("Resolving dependencies")
                .with_style(
                    ProgressStyle::with_template("{msg} {wide_bar} {pos}/{len}")
                        .expect("Template is correct"),
                ),
        ));
        let dependency_resolution_tasks = Arc::new(Mutex::new(Vec::new()));
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
                    Some(downloading_progress_bar.clone()),
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
                        Some(downloading_progress_bar.clone()),
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
                                Some(downloading_progress_bar.clone()),
                            )
                            .await
                            {
                                Ok(..) => {
                                    let db_conn = db_conn.clone();
                                    let base_url = base_url.into();
                                    let name = name.to_string();
                                    let tasks = dependency_resolution_tasks.clone();
                                    let progress_bar = progress_bar.clone();
                                    let indexing_progress_bar = indexing_progress_bar.clone();
                                    threads.execute(move || {
                                        if let Err(e) = Self::index_packages(
                                            &packages_file,
                                            &arch_dir,
                                            base_url,
                                            component_id,
                                            db_conn,
                                            tasks,
                                            name,
                                            indexing_progress_bar,
                                            progress_bar,
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
        downloading_progress_bar.lock().finish();
        threads.join();
        indexing_progress_bar.lock().finish();
        {
            let progress_bar = progress_bar.lock();
            progress_bar.reset_elapsed();
            progress_bar.set_draw_target(ProgressDrawTarget::stderr());
            //progress_bar.set_message("Resolving dependencies");
        }
        for task in Arc::into_inner(dependency_resolution_tasks)
            .expect("All indexing threads have finished")
            .into_inner()
            .into_iter()
        {
            //task();
            threads.execute(task);
        }
        threads.join();
        let progress_bar = progress_bar.lock();
        progress_bar.finish();
        log::info!(
            "Resolved dependencies of {} packages in {:.2}s",
            progress_bar.length().unwrap_or(0),
            progress_bar.elapsed().as_secs_f32()
        );
        Ok(())
    }

    fn resolve(
        &mut self,
        config: &Config,
        name: &str,
        dependencies: Vec<String>,
    ) -> Result<(), Error> {
        let db_conn = Connection::new(config)?;
        let db_conn = db_conn.lock();
        let mut matches = Vec::new();
        for dep in dependencies.into_iter() {
            let dep: deb::DependencyChoice = dep.parse()?;
            matches.extend(db_conn.select_deb_dependencies(name, &dep)?);
        }
        print_table(matches.iter(), std::io::stdout())?;
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
            // Select dependencies that has already been resolved on repository pull.
            let resolved_dependencies = db_conn
                .lock()
                .select_resolved_deb_dependencies(name, &package_name)?;
            // Remove the resolved dependencies from the package dependencies.
            let mut depends = matches[0].depends.clone().into_inner();
            for resolved in resolved_dependencies.into_iter() {
                let version: deb::Version = resolved.version.parse()?;
                let i = depends
                    .iter()
                    .position(|dep| dep.version_matches(&resolved.name, &version));
                if let Some(i) = i {
                    let dep = depends.remove(i);
                    log::info!(
                        "Already resolved \"{}\" as {}({})",
                        dep,
                        resolved.name,
                        resolved.version
                    );
                    matches.push(resolved);
                }
            }
            // Add the remaining unresolved dependencies to the queue.
            dependencies.extend(depends);
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
                    None,
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
                                Command::new("./patchelf.sh")
                                    .arg(&path)
                                    .arg(&dst)
                                    .status_checked()?;
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

impl ToRow<3> for DebDependencyMatch {
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

type Task = Box<dyn FnMut() + Send>;
