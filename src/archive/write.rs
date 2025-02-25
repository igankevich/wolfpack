use std::fs::Metadata;
use std::io::Error;
use std::io::Write;
use std::path::Path;

use normalize_path::NormalizePath;
use walkdir::WalkDir;

// TODO generic Header class
pub trait ArchiveWrite<W: Write> {
    fn new(writer: W) -> Self;

    fn add_regular_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        contents: C,
    ) -> Result<(), Error>;

    fn add_regular_file_with_metadata<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        metadata: &Metadata,
        contents: C,
    ) -> Result<(), Error>;

    fn into_inner(self) -> Result<W, Error>;

    fn from_files<I, P, D>(files: I, writer: W) -> Result<W, Error>
    where
        I: IntoIterator<Item = (P, D)>,
        P: AsRef<Path>,
        D: AsRef<[u8]>,
        Self: Sized,
    {
        let mut archive = Self::new(writer);
        for (path, data) in files.into_iter() {
            archive.add_regular_file(path, data)?;
        }
        archive.into_inner()
    }

    fn from_directory<P>(directory: P, writer: W) -> Result<W, Error>
    where
        P: AsRef<Path>,
        Self: Sized,
    {
        // TODO symlinks
        // TODO hardlinks
        let directory = directory.as_ref();
        let mut archive = Self::new(writer);
        for entry in WalkDir::new(directory).into_iter() {
            let entry = entry?;
            let entry_path = entry
                .path()
                .strip_prefix(directory)
                .map_err(std::io::Error::other)?
                .normalize();
            if entry_path == Path::new("") {
                continue;
            }
            let relative_path = Path::new(".").join(entry_path);
            let metadata = fs_err::metadata(entry.path())?;
            let data = if entry.file_type().is_dir() {
                Vec::new()
            } else {
                fs_err::read(entry.path())?
            };
            archive.add_regular_file_with_metadata(relative_path, &metadata, data)?;
        }
        archive.into_inner()
    }
}
