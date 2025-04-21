use std::fmt::Display;
use std::fmt::Formatter;
use std::io::ErrorKind;

use serde::Deserialize;
use serde::Serialize;

use crate::deb::Error;
use crate::deb::SimpleValue;
use crate::deb::Value;

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MultilineValue(String);

impl MultilineValue {
    pub fn try_from(value: String) -> Result<Self, Error> {
        if value.is_empty() || value.starts_with(char::is_whitespace) {
            return Err(ErrorKind::InvalidData.into());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Display for MultilineValue {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let mut lines = self.0.split('\n');
        if let Some(line) = lines.next() {
            write!(f, "{}", line)?;
        }
        for line in lines {
            if line.is_empty() || line.chars().all(|ch| [' ', '\t'].contains(&ch)) {
                write!(f, "\n .")?;
            } else {
                write!(f, "\n {}", line)?;
            }
        }
        Ok(())
    }
}

impl From<SimpleValue> for MultilineValue {
    fn from(value: SimpleValue) -> Self {
        Self(value.into())
    }
}

impl From<String> for MultilineValue {
    fn from(value: String) -> Self {
        value.as_str().into()
    }
}

impl From<&str> for MultilineValue {
    fn from(value: &str) -> Self {
        let mut multiline = String::with_capacity(value.len());
        let mut lines = value.split('\n');
        // parse the first line verbatim
        if let Some(line) = lines.next() {
            multiline.push_str(line);
            multiline.push('\n');
        }
        for line in lines {
            if line == " ." {
                multiline.push('\n');
            } else if line.starts_with([' ', '\t']) {
                multiline.push_str(&line[1..]);
                multiline.push('\n');
            } else {
                multiline.push_str(line);
                multiline.push('\n');
            }
        }
        if !multiline.is_empty() {
            multiline.pop();
        }
        Self(multiline)
    }
}

impl From<MultilineValue> for String {
    fn from(v: MultilineValue) -> Self {
        v.0
    }
}

impl PartialEq<SimpleValue> for MultilineValue {
    fn eq(&self, other: &SimpleValue) -> bool {
        self.0.eq(other.as_str())
    }
}

impl TryFrom<Value> for MultilineValue {
    type Error = Error;

    fn try_from(other: Value) -> Result<Self, Self::Error> {
        match other {
            Value::Simple(value) => Ok(value.into()),
            Value::Multiline(value) => Ok(value),
            _ => Err(Error::Package(
                "expected multiline value, received folded".into(),
            )),
        }
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
    fn multiline_display_parse() {
        arbtest(|u| {
            let expected: MultilineValue = u.arbitrary()?;
            let string = expected.to_string();
            let actual: MultilineValue = string.clone().into();
            assert_eq!(expected, actual, "string = {:?}", string);
            Ok(())
        });
    }

    impl<'a> Arbitrary<'a> for MultilineValue {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let seed: u64 = u.arbitrary()?;
            let mut rng = Mt64::new(seed);
            let valid_chars = Chars::from(UNICODE).difference(CONTROL);
            let s = loop {
                let len: usize = rng.gen_range(1..=100);
                let s = valid_chars.random_string(&mut rng, len);
                if !s.starts_with(dpkg_is_whitespace) {
                    break s;
                }
            };
            Ok(Self::try_from(s).unwrap())
        }
    }

    fn dpkg_is_whitespace(ch: char) -> bool {
        ch.is_whitespace() || ch.is_control()
    }
}
