use std::cmp::Ordering;
use std::fmt::Display;
use std::fmt::Formatter;

use crate::deb::Error;
use crate::deb::SimpleValue;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PackageVersion {
    epoch: u64,
    upstream_version: Version,
    debian_revision: Version,
}

impl PackageVersion {
    pub fn try_from(version: &str) -> Result<Self, Error> {
        Self::do_try_from(version).map_err(|version| Error::PackageVersion(version.to_string()))
    }

    fn do_try_from(version: &str) -> Result<Self, &str> {
        let (epoch, version) = match version.find(|ch| ch == ':') {
            Some(i) => (
                version[..i].parse().map_err(|_| version)?,
                &version[(i + 1)..],
            ),
            None => (0, version),
        };
        let (debian_revision, version, has_debian_revision) = match version.rfind(|ch| ch == '-') {
            Some(i) => (version[(i + 1)..].to_string(), &version[..i], true),
            None => ("0".into(), version, false),
        };
        if !debian_revision.chars().all(is_valid_char) {
            return Err(version);
        }
        let is_valid_char_v2 = if has_debian_revision {
            is_valid_char_with_hyphen
        } else {
            is_valid_char
        };
        if !(version.chars().all(is_valid_char_v2)
            && version.chars().next().iter().all(char::is_ascii_digit))
        {
            return Err(version);
        }
        Ok(Self {
            epoch,
            upstream_version: Version(version.to_string()),
            debian_revision: Version(debian_revision),
        })
    }
}

impl PartialOrd for PackageVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PackageVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let ret = self.epoch.cmp(&other.epoch);
        if ret != Ordering::Equal {
            return ret;
        }
        let ret = self.upstream_version.cmp(&other.upstream_version);
        if ret != Ordering::Equal {
            return ret;
        }
        let ret = self.debian_revision.cmp(&other.debian_revision);
        if ret != Ordering::Equal {
            return ret;
        }
        Ordering::Equal
    }
}

impl Display for PackageVersion {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        if self.epoch != 0 {
            write!(f, "{}:", self.epoch)?;
        }
        f.write_str(&self.upstream_version.0)?;
        if self.debian_revision.0 != "0" {
            write!(f, "-{}", self.debian_revision.0)?;
        }
        Ok(())
    }
}

impl TryFrom<SimpleValue> for PackageVersion {
    type Error = Error;
    fn try_from(other: SimpleValue) -> Result<Self, Self::Error> {
        Self::try_from(other.as_str())
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Version(String);

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut s1 = self.0.as_str();
        let mut s2 = other.0.as_str();
        while !s1.is_empty() || !s2.is_empty() {
            let n1 = s1
                .chars()
                .position(|ch| ch.is_ascii_digit())
                .unwrap_or(s1.len());
            let n2 = s2
                .chars()
                .position(|ch| ch.is_ascii_digit())
                .unwrap_or(s2.len());
            let ret = lexical_cmp(s1.chars().take(n1), s2.chars().take(n2));
            if ret != Ordering::Equal {
                return ret;
            }
            s1 = &s1[n1..];
            s2 = &s2[n2..];
            let n1 = s1
                .chars()
                .position(|ch| !ch.is_ascii_digit())
                .unwrap_or(s1.len());
            let n2 = s2
                .chars()
                .position(|ch| !ch.is_ascii_digit())
                .unwrap_or(s2.len());
            let ret = numerical_cmp(s1.chars().take(n1), s2.chars().take(n2));
            if ret != Ordering::Equal {
                return ret;
            }
            s1 = &s1[n1..];
            s2 = &s2[n2..];
        }
        Ordering::Equal
    }
}

fn is_valid_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ['+', '.', '~'].contains(&ch)
}

fn is_valid_char_with_hyphen(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ['+', '.', '~', '-'].contains(&ch)
}

fn lexical_cmp<I1, I2>(mut iter1: I1, mut iter2: I2) -> Ordering
where
    I1: Iterator<Item = char>,
    I2: Iterator<Item = char>,
{
    loop {
        match (iter1.next(), iter2.next()) {
            (Some(ch1), Some(ch2)) => {
                if ch1.is_alphabetic() && !ch2.is_alphabetic() {
                    return Ordering::Less;
                }
                if ch1 == '~' && ch2 != '~' {
                    return Ordering::Less;
                }
                let ret = ch1.cmp(&ch2);
                if ret != Ordering::Equal {
                    return ret;
                }
            }
            (None, Some(ch2)) => {
                return if ch2 == '~' {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
            (Some(ch1), None) => {
                return if ch1 == '~' {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            (None, None) => return Ordering::Equal,
        }
    }
}

fn numerical_cmp<I1, I2>(mut iter1: I1, mut iter2: I2) -> Ordering
where
    I1: Iterator<Item = char>,
    I2: Iterator<Item = char>,
{
    loop {
        match (iter1.next(), iter2.next()) {
            (Some(ch1), Some(ch2)) => {
                let ret = ch1.cmp(&ch2);
                if ret != Ordering::Equal {
                    return ret;
                }
            }
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => return Ordering::Equal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_cmp() {
        let v1 = Version("~~".into());
        let v2 = Version("~~a".into());
        let v3 = Version("~".into());
        let v4 = Version("".into());
        let v5 = Version("a".into());
        assert!(v1 < v2);
        assert!(v1 < v3);
        assert!(v1 < v4);
        assert!(v1 < v5);
        assert!(v2 < v3);
        assert!(v2 < v4);
        assert!(v2 < v5);
        assert!(v3 < v4);
        assert!(v3 < v5);
        assert!(v4 < v5);
    }
}
