use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::str::FromStr;

use crate::deb::Error;
use crate::hash::Md5Hash;

#[derive(Debug)]
pub struct Md5Sums {
    sums: HashMap<PathBuf, Md5Hash>,
}

impl Md5Sums {
    pub fn new() -> Self {
        Self {
            sums: Default::default(),
        }
    }

    pub fn insert(&mut self, path: PathBuf, hash: Md5Hash) {
        self.sums.insert(path, hash);
    }

    pub fn get(&self, path: &PathBuf) -> Option<&Md5Hash> {
        self.sums.get(path)
    }
}

impl Default for Md5Sums {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for Md5Sums {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for (path, hash) in self.sums.iter() {
            writeln!(f, "{}  {}", hash, path.display())?;
        }
        Ok(())
    }
}

impl FromStr for Md5Sums {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut sums: HashMap<PathBuf, Md5Hash> = HashMap::new();
        for line in value.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut iter = value.splitn(2, char::is_whitespace);
            let hash = iter.next().ok_or_else(|| Error::Md5Sums)?;
            let path = iter.next().ok_or_else(|| Error::Md5Sums)?;
            let hash: Md5Hash = hash.parse().map_err(|_| Error::Md5Sums)?;
            sums.insert(path.into(), hash);
        }
        Ok(Self { sums })
    }
}
