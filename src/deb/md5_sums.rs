use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::str::FromStr;

use crate::deb::Error;
use crate::hash::Md5Hash;

#[cfg_attr(test, derive(PartialEq, Eq, Debug))]
pub struct Md5Sums {
    sums: HashMap<PathBuf, Md5Hash>,
}

impl Md5Sums {
    pub fn new() -> Self {
        Self {
            sums: Default::default(),
        }
    }

    pub fn insert(&mut self, path: PathBuf, hash: Md5Hash) -> Result<(), std::io::Error> {
        if path.as_os_str().as_encoded_bytes().contains(&b'\n') {
            return Err(std::io::Error::other(format!(
                "path contains a newline: {:?}",
                path
            )));
        }
        self.sums.insert(path, hash);
        Ok(())
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
        for line in value.split('\n') {
            if line.is_empty() || line.chars().all(char::is_whitespace) {
                continue;
            }
            let i = line.find(SEPARATOR).ok_or(Error::Md5Sums)?;
            let hash = &line[..i];
            let path = &line[(i + SEPARATOR.len())..];
            let hash: Md5Hash = hash.parse().map_err(|_| Error::Md5Sums)?;
            sums.insert(path.into(), hash);
        }
        Ok(Self { sums })
    }
}

const SEPARATOR: &str = "  ";

#[cfg(test)]
mod tests {
    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;

    use super::*;
    use crate::hash::display_parse;

    #[test]
    fn test_display_parse() {
        display_parse::<Md5Sums>();
    }

    impl<'a> Arbitrary<'a> for Md5Sums {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let sums: HashMap<Md5SumsPath, Md5Hash> = u.arbitrary()?;
            Ok(Self {
                sums: sums.into_iter().map(|(k, v)| (k.0, v)).collect(),
            })
        }
    }

    // non-empty, no newlines
    #[derive(Debug, Hash, PartialEq, Eq)]
    struct Md5SumsPath(PathBuf);

    impl<'a> Arbitrary<'a> for Md5SumsPath {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let len = u.int_in_range(1..=10)?;
            let mut path = String::with_capacity(len);
            for _ in 0..len {
                let ch = loop {
                    let ch = u.arbitrary()?;
                    if ch != '\n' {
                        break ch;
                    }
                };
                path.push(ch);
            }
            Ok(Self(path.into()))
        }
    }
}
