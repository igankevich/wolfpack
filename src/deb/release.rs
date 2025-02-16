use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Write;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::SystemTime;

use chrono::DateTime;
use chrono::Utc;

use crate::deb::Arch;
use crate::deb::Error;
use crate::deb::Fields;
use crate::deb::Repository;
use crate::deb::SimpleValue;
use crate::hash::AnyHash;
use crate::hash::Md5Hash;
use crate::hash::MultiHash;
use crate::hash::MultiHashReader;
use crate::hash::Sha1Hash;
use crate::hash::Sha256Hash;

// https://wiki.debian.org/DebianRepository/Format#A.22Release.22_files
pub struct Release {
    date: Option<SystemTime>,
    valid_until: Option<SystemTime>,
    architectures: HashSet<Arch>,
    components: HashSet<SimpleValue>,
    suite: SimpleValue,
    md5: HashMap<PathBuf, (Md5Hash, u64)>,
    sha1: HashMap<PathBuf, (Sha1Hash, u64)>,
    sha256: HashMap<PathBuf, (Sha256Hash, u64)>,
}

impl Release {
    pub fn new(
        suite: SimpleValue,
        packages: &Repository,
        packages_str: &str,
    ) -> Result<Self, Error> {
        let architectures = packages.architectures();
        let mut checksums = HashMap::new();
        let reader = MultiHashReader::new(packages_str.as_bytes());
        let (hash, size) = reader.digest()?;
        checksums.insert("Packages".into(), Checksums { size, hash });
        for (arch, per_arch_packages) in packages.iter() {
            let mut path = PathBuf::new();
            path.push("main");
            path.push(format!("binary-{}", arch));
            path.push("Packages");
            let per_arch_packages_string = per_arch_packages.to_string();
            let reader = MultiHashReader::new(per_arch_packages_string.as_bytes());
            let (hash, size) = reader.digest()?;
            checksums.insert(path, Checksums { size, hash });
        }
        let mut md5 = HashMap::new();
        let mut sha1 = HashMap::new();
        let mut sha256 = HashMap::new();
        for (path, checksum) in checksums.into_iter() {
            md5.insert(path.clone(), (checksum.hash.md5.into(), checksum.size));
            sha1.insert(path.clone(), (checksum.hash.sha1, checksum.size));
            sha256.insert(path, (checksum.hash.sha2, checksum.size));
        }
        Ok(Self {
            date: Some(SystemTime::now()),
            valid_until: None,
            architectures,
            components: ["main".parse::<SimpleValue>()?].into(),
            suite,
            md5,
            sha1,
            sha256,
        })
    }

    pub fn get_files<P: AsRef<Path>>(
        &self,
        prefix: P,
        file_stem: &str,
    ) -> Vec<(PathBuf, AnyHash, u64)> {
        let prefix = prefix.as_ref();
        let file_stem = OsStr::new(file_stem);
        let mut files = Vec::new();
        get_files(&self.md5, prefix, file_stem, &mut files);
        get_files(&self.sha1, prefix, file_stem, &mut files);
        get_files(&self.sha256, prefix, file_stem, &mut files);
        files.sort_by_key(|(_path, hash, size)| {
            // smallest size, largest hash
            (*size, usize::MAX - hash.len())
        });
        files
    }

    pub fn components(&self) -> &HashSet<SimpleValue> {
        &self.components
    }
}

impl Display for Release {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        if let Some(date) = self.date {
            let date: DateTime<Utc> = date.into();
            writeln!(f, "Date: {}", date.to_rfc2822())?;
        }
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
        for (path, (hash, size)) in self.md5.iter() {
            write!(&mut md5, "\n {} {} {}", hash, size, path.display())?;
        }
        for (path, (hash, size)) in self.sha1.iter() {
            write!(&mut sha1, "\n {} {} {}", hash, size, path.display())?;
        }
        for (path, (hash, size)) in self.sha256.iter() {
            write!(&mut sha256, "\n {} {} {}", hash, size, path.display())?;
        }
        writeln!(f, "MD5Sum: {}", md5)?;
        writeln!(f, "SHA1: {}", sha1)?;
        writeln!(f, "SHA256: {}", sha256)?;
        Ok(())
    }
}

impl FromStr for Release {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut fields: Fields = value.parse()?;
        let control = Release {
            suite: fields.remove_any("suite")?.try_into()?,
            architectures: fields.remove_any("architectures")?.try_into()?,
            components: fields.remove_any("components")?.try_into()?,
            date: fields.remove_system_time("date")?,
            valid_until: fields.remove_system_time("valid-until")?,
            md5: fields.remove_hashes("md5sum")?,
            sha1: fields.remove_hashes("sha1")?,
            sha256: fields.remove_hashes("sha256")?,
        };
        Ok(control)
    }
}

struct Checksums {
    hash: MultiHash,
    size: u64,
}

fn get_files<H: Into<AnyHash> + Clone>(
    hashes: &HashMap<PathBuf, (H, u64)>,
    prefix: &Path,
    file_stem: &OsStr,
    files: &mut Vec<(PathBuf, AnyHash, u64)>,
) {
    files.extend(hashes.iter().filter_map(|(path, (hash, size))| {
        if path.starts_with(prefix) && path.file_stem() == Some(file_stem) {
            Some((path.clone(), hash.clone().into(), *size))
        } else {
            None
        }
    }))
}
