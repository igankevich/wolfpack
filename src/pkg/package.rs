use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::read_dir;
use std::fs::File;
use std::fs::Metadata;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use normalize_path::NormalizePath;
use walkdir::WalkDir;
use zstd::stream::read::Decoder as ZstdDecoder;
use zstd::stream::write::Encoder as ZstdEncoder;

use crate::archive::ArchiveWrite;
use crate::archive::TarBuilder;
use crate::hash::Sha256Reader;
use crate::pkg::CompactManifest;
use crate::pkg::Manifest;

pub struct Package {
    manifest: CompactManifest,
    directory: PathBuf,
}

impl Package {
    pub fn new(manifest: CompactManifest, directory: PathBuf) -> Self {
        Self {
            manifest,
            directory,
        }
    }

    pub fn write<W: Write>(&self, writer: W) -> Result<(), std::io::Error> {
        let mut package = TarBuilder::new(ZstdEncoder::new(writer, COMPRESSION_LEVEL)?);
        let mut files: HashMap<PathBuf, String> = HashMap::new();
        let mut config: HashSet<PathBuf> = HashSet::new();
        let mut directories: HashMap<PathBuf, String> = HashMap::new();
        let mut file_contents: HashMap<PathBuf, (Metadata, Vec<u8>)> = HashMap::new();
        for entry in WalkDir::new(self.directory.as_path()).into_iter() {
            let entry = entry?;
            let path = entry
                .path()
                .strip_prefix(self.directory.as_path())
                .map_err(std::io::Error::other)?
                .normalize();
            let absolute_path = Path::new("/").join(path.as_path());
            if absolute_path == Path::new("/") {
                continue;
            }
            if entry.file_type().is_dir() {
                if read_dir(entry.path())?.count() == 0 {
                    directories.insert(absolute_path.clone(), "y".to_string());
                }
                if absolute_path.starts_with(Path::new("/etc")) {
                    config.insert(absolute_path);
                }
            } else {
                let mut reader = Sha256Reader::new(File::open(entry.path())?);
                let mut contents = Vec::new();
                reader.read_to_end(&mut contents)?;
                let metadata = std::fs::metadata(entry.path())?;
                file_contents.insert(absolute_path.clone(), (metadata, contents));
                let (sha256, _) = reader.digest()?;
                files.insert(absolute_path, format!("1${}", sha256));
            }
        }
        package.add_regular_file("+COMPACT_MANIFEST", self.manifest.to_string())?;
        let manifest = Manifest {
            compact: self.manifest.clone(),
            files,
            config: config.into_iter().collect(),
            directories,
        };
        package.add_regular_file("+MANIFEST", manifest.to_string())?;
        for (path, (metadata, contents)) in file_contents.into_iter() {
            package.add_regular_file_with_metadata(path, &metadata, contents)?;
        }
        package.into_inner()?.finish()?;
        Ok(())
    }

    pub(crate) fn read_compact_manifest<R: Read>(
        reader: R,
    ) -> Result<CompactManifest, std::io::Error> {
        let mut reader = tar::Archive::new(ZstdDecoder::new(reader)?);
        for entry in reader.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.normalize();
            if path == Path::new("+COMPACT_MANIFEST") {
                let mut buf = String::with_capacity(4096);
                entry.read_to_string(&mut buf)?;
                return Ok(buf.parse()?);
            }
        }
        Err(std::io::Error::other("missing file: +COMPACT_MANIFEST"))
    }
}

const COMPRESSION_LEVEL: i32 = 22;

#[cfg(test)]
mod tests {
    use std::process::Command;

    use arbtest::arbtest;
    use tempfile::TempDir;

    use super::*;
    use crate::pkg::CompactManifest;
    use crate::test::prevent_concurrency;
    use crate::test::DirectoryOfFiles;

    #[test]
    fn write_read() {
        arbtest(|u| {
            let package: CompactManifest = u.arbitrary()?;
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let mut buf: Vec<u8> = Vec::new();
            Package::new(package.clone(), directory.path().into())
                .write(&mut buf)
                .unwrap();
            let actual = Package::read_compact_manifest(&buf[..]).unwrap();
            assert_eq!(package, actual);
            Ok(())
        });
    }

    #[ignore = "Needs FreeBSD's `pkg`"]
    #[test]
    fn freebsd_pkg_installs_random_packages() {
        let _guard = prevent_concurrency("freebsd-pkg");
        let workdir = TempDir::new().unwrap();
        let package_file = workdir.path().join("test.pkg");
        arbtest(|u| {
            let mut package: CompactManifest = u.arbitrary()?;
            package.flatsize = 100;
            package.deps.clear(); // missing dependencies
            let directory: DirectoryOfFiles = u.arbitrary()?;
            Package::new(package.clone(), directory.path().into())
                .write(File::create(package_file.as_path()).unwrap())
                .unwrap();
            assert!(
                Command::new("pkg")
                    .arg("install")
                    .arg("-y")
                    .arg(package_file.as_path())
                    .status()
                    .unwrap()
                    .success(),
                "manifest:\n========{:?}========",
                package
            );
            assert!(
                Command::new("pkg")
                    .arg("remove")
                    .arg("-y")
                    .arg(package.name.to_string())
                    .status()
                    .unwrap()
                    .success(),
                "manifest:\n========{:?}========",
                package
            );
            Ok(())
        });
    }
}
