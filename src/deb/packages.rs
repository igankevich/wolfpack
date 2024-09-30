use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use walkdir::WalkDir;

use crate::deb::ControlData;
use crate::deb::Error;
use crate::deb::Package;
use crate::hash::MultiHash;
use crate::hash::MultiHashReader;

pub struct Packages {
    packages: Vec<ExtendedControlData>,
}

impl Packages {
    pub fn new<I, P>(paths: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut packages = Vec::new();
        let mut push_package = |path: &Path| -> Result<(), Error> {
            eprintln!("reading {}", path.display());
            let mut reader = MultiHashReader::new(File::open(path)?);
            let control = Package::read_control(&mut reader)?;
            let (hash, size) = reader.digest()?;
            let control = ExtendedControlData {
                control,
                size,
                hash,
                filename: path.into(),
            };
            packages.push(control);
            Ok(())
        };
        for path in paths.into_iter() {
            let path = path.as_ref();
            if path.is_dir() {
                for entry in WalkDir::new(path).into_iter() {
                    let entry = entry?;
                    if entry.file_type().is_dir()
                        || entry.path().extension() != Some(OsStr::new("deb"))
                    {
                        continue;
                    }
                    push_package(entry.path())?
                }
            } else {
                push_package(path)?
            }
        }
        Ok(Self { packages })
    }

    pub fn iter(&self) -> impl Iterator<Item = &ExtendedControlData> {
        self.packages.iter()
    }
}

impl IntoIterator for Packages {
    type Item = <Vec<ExtendedControlData> as IntoIterator>::Item;
    type IntoIter = <Vec<ExtendedControlData> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.packages.into_iter()
    }
}

impl Display for Packages {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for control in self.packages.iter() {
            writeln!(f, "{}", control)?;
        }
        Ok(())
    }
}

pub struct ExtendedControlData {
    pub control: ControlData,
    hash: MultiHash,
    filename: PathBuf,
    size: usize,
}

impl Display for ExtendedControlData {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.control)?;
        writeln!(f, "Filename: {}", self.filename.display())?;
        writeln!(f, "Size: {}", self.size)?;
        writeln!(f, "MD5sum: {:x}", self.hash.md5)?;
        writeln!(f, "SHA1: {}", self.hash.sha1)?;
        writeln!(f, "SHA256: {}", self.hash.sha2)?;
        Ok(())
    }
}
