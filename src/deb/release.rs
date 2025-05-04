use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Write;
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
use crate::hash::Sha512Hash;

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
    sha512: HashMap<PathBuf, (Sha512Hash, u64)>,
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
        let mut sha512 = HashMap::new();
        for (path, checksum) in checksums.into_iter() {
            md5.insert(path.clone(), (checksum.hash.md5.into(), checksum.size));
            sha1.insert(path.clone(), (checksum.hash.sha1, checksum.size));
            sha256.insert(path.clone(), (checksum.hash.sha256, checksum.size));
            sha512.insert(path, (checksum.hash.sha512, checksum.size));
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
            sha512,
        })
    }

    pub fn get_files(&self, file_prefix: &str) -> Vec<(PathBuf, AnyHash, u64)> {
        let mut files = Vec::new();
        get_files(&self.md5, file_prefix, &mut files);
        get_files(&self.sha1, file_prefix, &mut files);
        get_files(&self.sha256, file_prefix, &mut files);
        get_files(&self.sha512, file_prefix, &mut files);
        files.sort_by(|a, b| {
            let a_size = a.2;
            let b_size = b.2;
            // Check zero-sized files last because zero size might mean the absence of the
            // file.
            if a_size == 0 {
                return if b_size == 0 {
                    Ordering::Equal
                } else {
                    Ordering::Greater
                };
            }
            if b_size == 0 {
                return Ordering::Less;
            }
            let a_hash_len = a.1.len();
            let b_hash_len = b.1.len();
            let smallest_size = a_size.cmp(&b_size);
            let largest_hash_len = a_hash_len.cmp(&b_hash_len).reverse();
            smallest_size.then(largest_hash_len)
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
        let mut sha512 = String::new();
        for (path, (hash, size)) in self.md5.iter() {
            write!(&mut md5, "\n {} {} {}", hash, size, path.display())?;
        }
        for (path, (hash, size)) in self.sha1.iter() {
            write!(&mut sha1, "\n {} {} {}", hash, size, path.display())?;
        }
        for (path, (hash, size)) in self.sha256.iter() {
            write!(&mut sha256, "\n {} {} {}", hash, size, path.display())?;
        }
        for (path, (hash, size)) in self.sha512.iter() {
            write!(&mut sha512, "\n {} {} {}", hash, size, path.display())?;
        }
        writeln!(f, "MD5Sum: {}", md5)?;
        writeln!(f, "SHA1: {}", sha1)?;
        writeln!(f, "SHA256: {}", sha256)?;
        writeln!(f, "SHA512: {}", sha512)?;
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
            sha512: fields.remove_hashes("sha512")?,
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
    file_prefix: &str,
    files: &mut Vec<(PathBuf, AnyHash, u64)>,
) {
    files.extend(hashes.iter().filter_map(|(path, (hash, size))| {
        let Some(path) = path.to_str() else {
            return None;
        };
        if path.starts_with(file_prefix) {
            Some((path.into(), hash.clone().into(), *size))
        } else {
            None
        }
    }))
}
