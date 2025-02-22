use std::fs::copy;
use std::fs::create_dir_all;
use std::fs::set_permissions;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use lddtree::DependencyAnalyzer;
use wolfpack::build;
use wolfpack::cargo;

use crate::Error;

pub struct ProjectBuilder {}

impl ProjectBuilder {
    pub fn new() -> Self {
        Self {}
    }

    pub fn build(&self, project_dir: &Path, output_dir: &Path) -> Result<(), Error> {
        let config = cargo::BuildConfig::default();
        let outputs = cargo::build_package(&config, project_dir)?;
        let lib_dir = output_dir.join("lib");
        for (target, path) in outputs.into_iter() {
            let subdir = match target {
                build::BuildTarget::Executable => "bin",
                build::BuildTarget::Library => "lib",
            };
            let dest_dir = output_dir.join(subdir);
            create_dir_all(&dest_dir)?;
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
                let dest_file = lib_dir.join(file_name);
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
                create_dir_all(&lib_dir)?;
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
        Ok(())
    }
}
