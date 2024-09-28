use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Write;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;

use chrono::DateTime;
use chrono::Utc;
use walkdir::WalkDir;

use crate::deb::Error;
use crate::deb::HashingReader;
use crate::deb::Md5Digest;
use crate::deb::Sha1Digest;
use crate::deb::Sha2Digest;
use crate::deb::SimpleValue;

// https://wiki.debian.org/DebianRepository/Format#A.22Release.22_files
pub struct Release {
    date: SystemTime,
    valid_until: Option<SystemTime>,
    architectures: HashSet<SimpleValue>,
    components: HashSet<SimpleValue>,
    suite: SimpleValue,
    checksums: HashMap<PathBuf, Checksums>,
}

impl Release {
    pub fn new<P: AsRef<Path>>(
        directory: P,
        architectures: HashSet<SimpleValue>,
        suite: SimpleValue,
    ) -> Result<Self, Error> {
        let mut checksums = HashMap::new();
        for entry in WalkDir::new(directory).into_iter() {
            let entry = entry?;
            if entry.file_type().is_dir() {
                continue;
            }
            let path = entry.path();
            let file_stem = match path.file_stem() {
                Some(file_stem) => file_stem,
                None => continue,
            };
            if ![OsStr::new("Packages"), OsStr::new("Release")].contains(&file_stem)
                || path.extension() == Some(OsStr::new("gpg"))
            {
                continue;
            }
            let reader = HashingReader::new(File::open(path)?);
            let (md5, sha1, sha256, size) = reader.digest()?;
            checksums.insert(
                path.into(),
                Checksums {
                    size,
                    md5,
                    sha1,
                    sha256,
                },
            );
        }
        Ok(Self {
            date: SystemTime::now(),
            valid_until: None,
            architectures,
            // TODO do we need them?
            components: Default::default(),
            suite,
            checksums,
        })
    }
}

impl Display for Release {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let date: DateTime<Utc> = self.date.into();
        writeln!(f, "Date: {}", date.to_rfc2822())?;
        if let Some(valid_until) = self.valid_until {
            let valid_until: DateTime<Utc> = valid_until.into();
            writeln!(f, "Valid-Until: {}", valid_until)?;
        }
        write!(f, "Architectures:")?;
        for arch in self.architectures.iter() {
            write!(f, " {}", arch)?;
        }
        writeln!(f)?;
        write!(f, "Components:")?;
        for comp in self.components.iter() {
            write!(f, " {}", comp)?;
        }
        writeln!(f)?;
        writeln!(f, "Suite: {}", self.suite)?;
        let mut md5 = String::new();
        let mut sha1 = String::new();
        let mut sha256 = String::new();
        for (path, sums) in self.checksums.iter() {
            write!(
                &mut md5,
                "\n {:x} {} {}",
                sums.md5,
                sums.size,
                path.display()
            )?;
            write!(
                &mut sha1,
                "\n {} {} {}",
                sums.sha1,
                sums.size,
                path.display()
            )?;
            write!(
                &mut sha256,
                "\n {} {} {}",
                sums.sha256,
                sums.size,
                path.display()
            )?;
        }
        writeln!(f, "MD5Sum: {}", md5)?;
        writeln!(f, "SHA1: {}", sha1)?;
        writeln!(f, "SHA256: {}", sha256)?;
        Ok(())
    }
}

struct Checksums {
    md5: Md5Digest,
    sha1: Sha1Digest,
    sha256: Sha2Digest,
    size: usize,
}
