use fs_err::copy;
use fs_err::create_dir_all;
use fs_err::set_permissions;
use std::collections::HashSet;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use elb_dl::DependencyTree;
use elb_dl::DynamicLoader;
use elb_dl::Libc;
use wolfpack::build;
use wolfpack::cargo;
use wolfpack::elf;
use wolfpack::wolf;

use crate::Error;
use crate::PACKAGE_CONFIG_FILE_NAME;

pub struct ProjectBuilder {}

impl ProjectBuilder {
    pub fn new() -> Self {
        Self {}
    }

    pub fn build(&self, project_dir: &Path, output_dir: &Path) -> Result<(), Error> {
        let packages = cargo::get_packages(project_dir)?;
        for package in packages.into_iter() {
            let output_dir = output_dir.join(&package.name);
            for (config_name, config) in package.metadata.wolfpack.into_iter() {
                log::trace!(
                    "Building package {:?} with configuration {:?}",
                    package.name,
                    config_name
                );
                if !config.common.prefix.is_absolute() {
                    return Err(Error::InstallationPrefix(config.common.prefix));
                }
                let output_dir = output_dir.join(config_name);
                let rootfs_dir = output_dir.join("rootfs");
                let app_dir = {
                    let mut dir = rootfs_dir.to_path_buf();
                    dir.push(
                        config
                            .common
                            .prefix
                            .strip_prefix("/")
                            .expect("Checked above"),
                    );
                    dir.push(&package.name);
                    dir
                };
                let build_output = cargo::build_package(&package.name, &config, project_dir)?;
                let mut dirs = DirMaker::new();
                let lib_dir = app_dir.join("lib");
                for (target, path) in build_output.files.into_iter() {
                    let subdir = match target {
                        build::BuildTarget::Executable => "bin",
                        build::BuildTarget::Library => "lib",
                    };
                    let dest_dir = app_dir.join(subdir);
                    dirs.create(&dest_dir)?;
                    let dest_file = dest_dir.join(path.file_name().expect("File name is present"));
                    copy(&path, &dest_file)?;
                    set_permissions(&dest_file, Permissions::from_mode(0o755))?;
                    let mut tree = DependencyTree::new();
                    let Some((old_interpreter, new_interpreter)) =
                        build_output.interpreter.as_ref()
                    else {
                        // No need to patch static binaries.
                        continue;
                    };
                    let search_dirs = elb_dl::glibc::get_search_dirs(&config.common.sysroot)?;
                    let loader = DynamicLoader::options()
                        .libc(Libc::Glibc)
                        .search_dirs(search_dirs)
                        .new_loader();
                    let dependencies = loader.resolve_dependencies(&path, &mut tree)?;
                    for path in dependencies.iter() {
                        let (path, mode, patch) = if path == new_interpreter {
                            (old_interpreter.as_path(), 0o755, false)
                        } else {
                            (path.as_path(), 0o644, true)
                        };
                        let file_name = path.file_name().expect("File name exists");
                        let dest_file = lib_dir.join(file_name);
                        dirs.create(&lib_dir)?;
                        copy(path, &dest_file)?;
                        set_permissions(&dest_file, Permissions::from_mode(mode))?;
                        if patch {
                            elf::patch(&dest_file, "$ORIGIN", None)?;
                        }
                    }
                    // TODO this is unrealiable without chroot
                    // Check that all dependencies can be resolved in the new root.
                    //{
                    //    let mut tree = DependencyTree::new();
                    //    let loader = DynamicLoader::options()
                    //        .root(&rootfs_dir)
                    //        .libc(Libc::Musl)
                    //        //.search_dirs_override(vec![app_dir.join("lib")])
                    //        .new_loader();
                    //    let mut queue = VecDeque::new();
                    //    queue.push_back(dest_file.to_path_buf());
                    //    while let Some(path) = queue.pop_front() {
                    //        let deps = loader
                    //            .resolve_dependencies(&path, &mut tree)
                    //            .map_err(|e| Error::ElfDependency(e.to_string()))?;
                    //        queue.extend(deps);
                    //    }
                    //}
                }
                let doc_dir = {
                    let mut dst = app_dir.to_path_buf();
                    dst.push("share");
                    dst.push("doc");
                    dst.push(package.name.as_str());
                    dst
                };
                for file in [package.license_file.as_ref(), package.readme.as_ref()]
                    .iter()
                    .flatten()
                {
                    let src = project_dir.join(file);
                    let dest_file = doc_dir.join(file);
                    dirs.create(&doc_dir)?;
                    copy(&src, &dest_file)?;
                }
                let metadata = wolf::Metadata {
                    name: package.name.clone(),
                    version: package.version.clone(),
                    description: package.description.clone().unwrap_or_default(),
                    homepage: [
                        package.homepage.clone(),
                        package.documentation.clone(),
                        package.repository.clone(),
                    ]
                    .into_iter()
                    .flatten()
                    .next()
                    .unwrap_or_default(),
                    license: package.license.clone().unwrap_or_default(),
                };
                let metadata_string = toml::to_string_pretty(&metadata)?;
                let metadata_file = output_dir.join(PACKAGE_CONFIG_FILE_NAME);
                fs_err::write(&metadata_file, metadata_string.as_bytes())?;
                // TODO spdx licenses
            }
        }
        Ok(())
    }
}

struct DirMaker {
    dirs: HashSet<PathBuf>,
}

impl DirMaker {
    fn new() -> Self {
        Self {
            dirs: Default::default(),
        }
    }

    fn create(&mut self, path: &Path) -> Result<(), std::io::Error> {
        if self.dirs.contains(path) {
            return Ok(());
        }
        create_dir_all(path)?;
        self.dirs.insert(path.to_path_buf());
        Ok(())
    }
}
