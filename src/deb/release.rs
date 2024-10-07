use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Write;
use std::path::PathBuf;
use std::time::SystemTime;

use chrono::DateTime;
use chrono::Utc;

use crate::deb::Error;
use crate::deb::Packages;
use crate::deb::SimpleValue;
use crate::hash::MultiHash;
use crate::hash::MultiHashReader;

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
    pub fn new(suite: SimpleValue, packages: &Packages, packages_str: &str) -> Result<Self, Error> {
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
        Ok(Self {
            date: SystemTime::now(),
            valid_until: None,
            architectures,
            components: ["main".parse::<SimpleValue>()?].into(),
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
                sums.hash.md5,
                sums.size,
                path.display()
            )?;
            write!(
                &mut sha1,
                "\n {} {} {}",
                sums.hash.sha1,
                sums.size,
                path.display()
            )?;
            write!(
                &mut sha256,
                "\n {} {} {}",
                sums.hash.sha2,
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
    hash: MultiHash,
    size: usize,
}
