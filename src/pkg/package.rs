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
use zstd::stream::write::Encoder as ZstdEncoder;

use crate::archive::ArchiveWrite;
use crate::pkg::CompactManifest;
use crate::pkg::Manifest;
use crate::pkg::Sha256Reader;

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

    pub fn build<W: Write>(&self, writer: W) -> Result<(), std::io::Error> {
        let mut package = tar::Builder::new(ZstdEncoder::new(writer, COMPRESSION_LEVEL)?);
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
            let relative_path = Path::new("./").join(path);
            if absolute_path == Path::new("/") {
                continue;
            }
            eprintln!("path {:?}", absolute_path.display());
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
                file_contents.insert(relative_path, (metadata, contents));
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
            eprintln!("file path {:?}", path.display());
            package.add_regular_file_with_metadata(path, &metadata, contents)?;
        }
        package.into_inner()?.finish()?;
        Ok(())
    }
}

const COMPRESSION_LEVEL: i32 = 22;
