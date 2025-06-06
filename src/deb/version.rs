use std::cmp::Ordering;
use std::fmt::Display;
use std::fmt::Formatter;
use std::hash::Hash;
use std::hash::Hasher;
use std::io::ErrorKind;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

use crate::deb::Error;
use crate::deb::SimpleValue;
use crate::deb::Value;

pub type Epoch = u64;

/// https://www.debian.org/doc/debian-policy/ch-controlfields.html#version
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Version {
    epoch: Epoch,
    upstream_version: UpstreamVersion,
    revision: Revision,
}

impl Version {
    pub fn new(version: &str) -> Result<Self, Error> {
        Self::do_new(version).map_err(|version| Error::Version(version.to_string()))
    }

    fn do_new(version: &str) -> Result<Self, &str> {
        let (epoch, version) = match version.find(':') {
            Some(i) => (
                version[..i].parse().map_err(|_| version)?,
                &version[(i + 1)..],
            ),
            None => (0, version),
        };
        let (revision, version, has_revision) = match version.rfind('-') {
            Some(i) => (version[(i + 1)..].to_string(), &version[..i], true),
            None => (String::new(), version, false),
        };
        Ok(Self {
            epoch,
            upstream_version: UpstreamVersion::new(version.to_string(), has_revision)
                .map_err(|_| version)?,
            revision: Revision::new(revision).map_err(|_| version)?,
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let ret = self.epoch.cmp(&other.epoch);
        if ret != Ordering::Equal {
            return ret;
        }
        let ret = self.upstream_version.cmp(&other.upstream_version);
        if ret != Ordering::Equal {
            return ret;
        }
        let ret = self.revision.cmp(&other.revision);
        if ret != Ordering::Equal {
            return ret;
        }
        Ordering::Equal
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        if self.epoch != 0 {
            write!(f, "{}:", self.epoch)?;
        }
        f.write_str(&self.upstream_version.0)?;
        if !self.revision.0.is_empty() {
            write!(f, "-{}", self.revision.0)?;
        }
        Ok(())
    }
}

impl TryFrom<SimpleValue> for Version {
    type Error = Error;
    fn try_from(other: SimpleValue) -> Result<Self, Self::Error> {
        Self::new(other.as_str())
    }
}

impl TryFrom<Value> for Version {
    type Error = Error;

    fn try_from(other: Value) -> Result<Self, Self::Error> {
        match other {
            Value::Simple(value) => value.try_into(),
            _ => Err(Error::Package(
                "expected simple value, received multiline/folded".into(),
            )),
        }
    }
}

impl From<Version> for String {
    fn from(other: Version) -> Self {
        other.to_string()
    }
}

impl TryFrom<String> for Version {
    type Error = Error;
    fn try_from(other: String) -> Result<Self, Self::Error> {
        Self::new(other.as_str())
    }
}

impl FromStr for Version {
    type Err = Error;
    fn from_str(other: &str) -> Result<Self, Self::Err> {
        Self::new(other)
    }
}

#[derive(Clone, Debug)]
struct Revision(String);

impl Revision {
    fn new(s: String) -> Result<Self, Error> {
        if !s.chars().all(is_valid_char) {
            return Err(ErrorKind::InvalidData.into());
        }
        Ok(Self(s))
    }

    fn to_str(&self) -> &str {
        if self.0.is_empty() {
            "0"
        } else {
            self.0.as_str()
        }
    }
}

impl PartialEq for Revision {
    fn eq(&self, other: &Self) -> bool {
        self.to_str().eq(other.to_str())
    }
}

impl Eq for Revision {}

impl Hash for Revision {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.to_str().hash(state);
    }
}

impl PartialOrd for Revision {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Revision {
    fn cmp(&self, other: &Self) -> Ordering {
        version_cmp(self.to_str(), other.to_str())
    }
}

impl FromStr for Revision {
    type Err = Error;
    fn from_str(other: &str) -> Result<Self, Self::Err> {
        Self::new(other.into())
    }
}

impl Display for Revision {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct UpstreamVersion(String);

impl UpstreamVersion {
    fn new(s: String, has_revision: bool) -> Result<Self, String> {
        let is_valid_char_v2 = if has_revision {
            is_valid_char_with_hyphen
        } else {
            is_valid_char
        };
        if !(s.chars().all(is_valid_char_v2) && s.chars().next().iter().all(char::is_ascii_digit)) {
            return Err(s);
        }
        Ok(Self(s))
    }
}

impl PartialOrd for UpstreamVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for UpstreamVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        version_cmp(self.0.as_str(), other.0.as_str())
    }
}

impl Display for UpstreamVersion {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn version_cmp(mut s1: &str, mut s2: &str) -> Ordering {
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
        let (i1, i2) = if n1 < n2 {
            (
                std::iter::repeat_n('0', n2 - n1),
                std::iter::repeat_n('0', 0),
            )
        } else {
            (
                std::iter::repeat_n('0', 0),
                std::iter::repeat_n('0', n1 - n2),
            )
        };
        let ret = numerical_cmp(i1.chain(s1.chars().take(n1)), i2.chain(s2.chars().take(n2)));
        if ret != Ordering::Equal {
            return ret;
        }
        s1 = &s1[n1..];
        s2 = &s2[n2..];
    }
    Ordering::Equal
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
    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use arbtest::arbtest;

    use super::*;
    use crate::test::to_string_parse_symmetry;

    #[test]
    fn test_version_cmp() {
        let v1 = UpstreamVersion("~~".into());
        let v2 = UpstreamVersion("~~a".into());
        let v3 = UpstreamVersion("~".into());
        let v4 = UpstreamVersion("".into());
        let v5 = UpstreamVersion("a".into());
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
        assert!(UpstreamVersion("3.0.7".into()) <= UpstreamVersion("3.0.15".into()))
    }

    #[test]
    fn package_version_symmetry() {
        to_string_parse_symmetry::<Version>();
    }

    #[test]
    fn upstream_version_symmetry() {
        arbtest(|u| {
            let ArbitraryUpstreamVersion(expected, has_revision) = u.arbitrary()?;
            let string = expected.to_string();
            let actual = UpstreamVersion::new(string.clone(), has_revision)
                .inspect_err(|e| {
                    panic!(
                        "Failed to parse `{}` as `{}`: {:?}",
                        string,
                        std::any::type_name::<UpstreamVersion>(),
                        e
                    )
                })
                .unwrap();
            assert_eq!(
                expected, actual,
                "expected = {expected:?}, actual = {actual:?}, string = {string:?}"
            );
            Ok(())
        });
    }

    #[test]
    fn revision_symmetry() {
        to_string_parse_symmetry::<Revision>();
    }

    #[test]
    fn valid_package_version() {
        arbtest(|u| {
            let _value: Version = u.arbitrary()?;
            Ok(())
        });
    }

    #[test]
    fn revisions() {
        assert!(Revision::new("#".into()).is_err());
        assert!(Revision::new("0-".into()).is_err());
        assert!(Revision::new("".into()).is_ok());
        assert_eq!(
            Revision::new("".into()).unwrap(),
            Revision::new("0".into()).unwrap()
        );
        assert_eq!(
            Ordering::Equal,
            Revision::new("".into())
                .unwrap()
                .cmp(&Revision::new("0".into()).unwrap())
        );
    }

    #[test]
    fn valid_revisions() {
        arbtest(|u| {
            let _value: Revision = u.arbitrary()?;
            Ok(())
        });
    }

    #[test]
    fn upstream_versions() {
        assert!(UpstreamVersion::new("#".into(), true).is_err());
        assert!(UpstreamVersion::new("0-".into(), true).is_ok());
        assert!(UpstreamVersion::new("0-".into(), false).is_err());
    }

    #[test]
    fn valid_upstream_version() {
        arbtest(|u| {
            let _value: ArbitraryUpstreamVersion = u.arbitrary()?;
            Ok(())
        });
    }

    impl<'a> Arbitrary<'a> for Version {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let ArbitraryUpstreamVersion(upstream_version, has_revision) = u.arbitrary()?;
            let version = Self {
                epoch: u.int_in_range(0..=u16::MAX as Epoch)?,
                upstream_version,
                revision: if has_revision {
                    u.arbitrary()?
                } else {
                    Revision::new(String::new()).unwrap()
                },
            };
            Ok(version)
        }
    }

    impl<'a> Arbitrary<'a> for Revision {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let valid_chars = get_valid_chars();
            let len = u.arbitrary_len::<char>()?.max(1);
            let mut string = String::with_capacity(len);
            for _ in 0..len {
                string.push(*u.choose(&valid_chars)?);
            }
            Ok(Self::new(string).unwrap())
        }
    }

    #[derive(Debug)]
    struct ArbitraryUpstreamVersion(UpstreamVersion, bool);

    impl<'a> Arbitrary<'a> for ArbitraryUpstreamVersion {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let has_revision: bool = u.arbitrary()?;
            let valid_first_chars: Vec<_> = ('0'..='9').collect();
            let valid_chars = if has_revision {
                get_valid_chars_with_hyphen()
            } else {
                get_valid_chars()
            };
            let len = u.arbitrary_len::<char>()?;
            let mut string = String::with_capacity(len);
            string.push(*u.choose(&valid_first_chars)?);
            for _ in 1..len {
                string.push(*u.choose(&valid_chars)?);
            }
            Ok(Self(
                UpstreamVersion::new(string, has_revision).unwrap(),
                has_revision,
            ))
        }
    }

    fn get_valid_chars() -> Vec<char> {
        ('a'..='z')
            .chain('A'..='Z')
            .chain('0'..='9')
            .chain(['+', '.', '~'])
            .collect()
    }

    fn get_valid_chars_with_hyphen() -> Vec<char> {
        ('a'..='z')
            .chain('A'..='Z')
            .chain('0'..='9')
            .chain(['+', '.', '~', '-'])
            .collect()
    }
}
