use std::collections::HashSet;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

use crate::deb::Error;
use crate::deb::FoldedValue;
use crate::deb::MultilineValue;
use crate::deb::PackageName;
use crate::deb::Value;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize, Default)]
#[serde(try_from = "String", into = "String")]
pub struct SimpleValue(String);

impl SimpleValue {
    pub fn new(value: String) -> Result<Self, Error> {
        validate_simple_value(&value)?;
        Ok(Self(value))
    }

    pub(crate) unsafe fn new_unchecked(value: String) -> Self {
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Display for SimpleValue {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for SimpleValue {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value.to_string())
    }
}

impl TryFrom<String> for SimpleValue {
    type Error = Error;
    fn try_from(other: String) -> Result<Self, Self::Error> {
        other.parse()
    }
}

impl From<SimpleValue> for String {
    fn from(other: SimpleValue) -> String {
        other.0
    }
}

impl From<SimpleValue> for PathBuf {
    fn from(other: SimpleValue) -> PathBuf {
        other.0.into()
    }
}

impl From<SimpleValue> for HashSet<SimpleValue> {
    fn from(other: SimpleValue) -> HashSet<SimpleValue> {
        other
            .0
            .split_whitespace()
            .map(|x| SimpleValue(x.to_string()))
            .collect()
    }
}

impl From<FoldedValue> for SimpleValue {
    fn from(other: FoldedValue) -> Self {
        let mut buf = String::with_capacity(other.as_str().len());
        let mut words = other.words();
        if let Some(word) = words.next() {
            buf.push_str(word);
        }
        for word in words {
            buf.push(' ');
            buf.push_str(word);
        }
        SimpleValue(buf)
    }
}

impl From<PackageName> for SimpleValue {
    fn from(other: PackageName) -> Self {
        Self(other.into())
    }
}

impl PartialEq<MultilineValue> for SimpleValue {
    fn eq(&self, other: &MultilineValue) -> bool {
        self.0.eq(other.as_str())
    }
}

impl PartialEq<FoldedValue> for SimpleValue {
    fn eq(&self, other: &FoldedValue) -> bool {
        other.eq(self)
    }
}

impl TryFrom<Value> for SimpleValue {
    type Error = Error;

    fn try_from(other: Value) -> Result<Self, Self::Error> {
        match other {
            Value::Simple(value) => Ok(value),
            Value::Folded(value) => Ok(value.into()),
            Value::Multiline(..) => Err(Error::Package(
                "expected simple value, received multiline".into(),
            )),
        }
    }
}

impl TryFrom<&str> for SimpleValue {
    type Error = Error;

    fn try_from(other: &str) -> Result<Self, Self::Error> {
        other.parse()
    }
}

fn validate_simple_value(value: &str) -> Result<(), Error> {
    if !value.as_bytes().iter().all(is_valid_char) {
        return Err(ErrorKind::InvalidData.into());
    }
    if value.is_empty() || value.chars().all(char::is_whitespace) {
        return Err(ErrorKind::InvalidData.into());
    }
    if value.chars().next().iter().all(|ch| ch.is_whitespace()) {
        return Err(ErrorKind::InvalidData.into());
    }
    Ok(())
}

fn is_valid_char(ch: &u8) -> bool {
    ![b'\r', b'\n'].contains(ch)
}

pub trait ParseField {
    fn parse_field<T>(&self, name: &'static str) -> Result<T, Error>
    where
        T: FromStr;
}

impl ParseField for str {
    fn parse_field<T>(&self, name: &'static str) -> Result<T, Error>
    where
        T: FromStr,
    {
        self.parse::<T>()
            .map_err(|_| Error::InvalidField(name, self.into()))
    }
}

#[cfg(test)]
mod tests {
    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use arbtest::arbtest;
    use rand::Rng;
    use rand_mt::Mt64;

    use super::*;
    use crate::test::Chars;
    use crate::test::CONTROL;
    use crate::test::UNICODE;

    #[test]
    fn invalid_simple_value() {
        assert!("hello\nworld".parse::<SimpleValue>().is_err());
        assert!("hello\rworld".parse::<SimpleValue>().is_err());
        assert!("\n".parse::<SimpleValue>().is_err());
        assert!("\r".parse::<SimpleValue>().is_err());
        assert!(" ".parse::<SimpleValue>().is_err());
        assert!("".parse::<SimpleValue>().is_err());
    }

    #[test]
    fn valid_simple_value() {
        arbtest(|u| {
            let _value: SimpleValue = u.arbitrary()?;
            Ok(())
        });
    }

    impl<'a> Arbitrary<'a> for SimpleValue {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let seed: u64 = u.arbitrary()?;
            let mut rng = Mt64::new(seed);
            let valid_chars = Chars::from(UNICODE).difference(CONTROL);
            let s = loop {
                let len: usize = rng.gen_range(1..=100);
                let s = valid_chars.random_string(&mut rng, len);
                if !s.chars().next().iter().all(|ch| ch.is_whitespace()) {
                    break s;
                }
            };
            Ok(Self::new(s).unwrap())
        }
    }
}
