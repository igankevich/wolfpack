use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use walkdir::WalkDir;

use crate::deb::ControlData;
use crate::deb::Error;
use crate::deb::HashingReader;
use crate::deb::Md5Digest;
use crate::deb::Package;
use crate::deb::Sha1Digest;
use crate::deb::Sha2Digest;

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
            let mut reader = HashingReader::new(File::open(path)?);
            let control = Package::read_control(&mut reader)?;
            let (md5, sha1, sha256, size) = reader.digest()?;
            let control = ExtendedControlData {
                control,
                size,
                md5,
                sha1,
                sha256,
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

    pub fn into_iter(self) -> impl IntoIterator<Item = ExtendedControlData> {
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
    // TODO Checksums
    md5: Md5Digest,
    sha1: Sha1Digest,
    sha256: Sha2Digest,
    filename: PathBuf,
    size: usize,
}

impl Display for ExtendedControlData {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.control)?;
        writeln!(f, "Filename: {}", self.filename.display())?;
        writeln!(f, "Size: {}", self.size)?;
        writeln!(f, "MD5sum: {:x}", self.md5)?;
        writeln!(f, "SHA1: {}", self.sha1)?;
        writeln!(f, "SHA256: {}", self.sha256)?;
        Ok(())
    }
}
