use std::collections::HashSet;
use std::fs::copy;
use std::fs::create_dir_all;
use std::fs::set_permissions;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use lddtree::DependencyAnalyzer;
use wolfpack::build;
use wolfpack::cargo;
use wolfpack::wolf;

use crate::Error;

pub struct ProjectBuilder {}

impl ProjectBuilder {
    pub fn new() -> Self {
        Self {}
    }

    pub fn build(&self, project_dir: &Path, output_dir: &Path) -> Result<(), Error> {
        let config = cargo::BuildConfig::default();
        let package = cargo::get_package_metadata(config.main_package.as_deref(), project_dir)?;
        let outputs = cargo::build_package(&config, project_dir)?;
        let mut dirs = DirMaker::new();
        let rootfs_dir = output_dir.join("rootfs");
        let app_dir = {
            let mut dir = rootfs_dir.to_path_buf();
            // TODO configure
            dir.push("opt");
            dir.push(&package.name);
            dir
        };
        let lib_dir = app_dir.join("lib");
        for (target, path) in outputs.into_iter() {
            let subdir = match target {
                build::BuildTarget::Executable => "bin",
                build::BuildTarget::Library => "lib",
            };
            let dest_dir = app_dir.join(subdir);
            dirs.create(&dest_dir)?;
            let dest_file = dest_dir.join(path.file_name().expect("File name is present"));
            copy(&path, &dest_file)?;
            set_permissions(&dest_file, Permissions::from_mode(0o755))?;
            let analyzer = DependencyAnalyzer::new("/".into());
            let dependencies = analyzer.analyze(&path)?;
            let interpreter = if let Some(interpreter) = dependencies.interpreter.as_ref() {
                let library = dependencies
                    .libraries
                    .get(interpreter)
                    .ok_or_else(|| Error::LibraryNotFound(interpreter.into()))?;
                let realpath = library.realpath.as_ref().expect("Checked above");
                let file_name = realpath.file_name().expect("File name exists");
                let dest_file = Path::new("/").join(
                    lib_dir
                        .strip_prefix(&rootfs_dir)
                        .expect("Prefix exists")
                        .join(file_name),
                );
                eprintln!("Set interp {:?}", dest_file);
                Some(dest_file)
            } else {
                None
            };
            for (file_name, library) in dependencies.libraries.iter() {
                let realpath = library
                    .realpath
                    .as_ref()
                    .ok_or_else(|| Error::LibraryNotFound(library.path.clone()))?;
                let file_name = Path::new(&file_name).file_name().expect("File name exists");
                let dest_file = lib_dir.join(file_name);
                dirs.create(&lib_dir)?;
                copy(realpath, &dest_file)?;
                set_permissions(&dest_file, Permissions::from_mode(0o755))?;
                let mut patchelf = Command::new("patchelf");
                patchelf.arg("--set-rpath");
                patchelf.arg("$ORIGIN");
                if let Some(interpreter) = interpreter.as_ref() {
                    patchelf.arg("--set-interpreter");
                    patchelf.arg(interpreter);
                }
                patchelf.arg("--force-rpath");
                patchelf.arg(&dest_file);
                patchelf.status()?;
            }
            let mut patchelf = Command::new("patchelf");
            patchelf.arg("--set-rpath");
            patchelf.arg("$ORIGIN/../lib");
            if let Some(interpreter) = interpreter.as_ref() {
                patchelf.arg("--set-interpreter");
                patchelf.arg(interpreter);
            }
            patchelf.arg("--force-rpath");
            patchelf.arg(&dest_file);
            patchelf.status()?;
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
            name: package.name,
            version: package.version,
            description: package.description.unwrap_or_default(),
            homepage: [package.homepage, package.documentation, package.repository]
                .into_iter()
                .flatten()
                .next()
                .unwrap_or_default(),
            license: package.license.unwrap_or_default(),
            arch: config.get_target()?.try_into()?,
        };
        let metadata_string = toml::to_string_pretty(&metadata)?;
        let metadata_file = output_dir.join("wolfpack.toml");
        std::fs::write(&metadata_file, metadata_string.as_bytes())?;
        // TODO spdx licenses
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
