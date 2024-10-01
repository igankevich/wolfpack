use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;

use crate::deb::Error;
use crate::deb::FoldedValue;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SimpleValue(String);

impl SimpleValue {
    pub fn try_from(value: String) -> Result<Self, Error> {
        validate_simple_value(&value)?;
        Ok(Self(value))
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
        Self::try_from(value.to_string())
    }
}

impl From<SimpleValue> for String {
    fn from(other: SimpleValue) -> String {
        other.0
    }
}

impl From<FoldedValue> for SimpleValue {
    fn from(other: FoldedValue) -> Self {
        let mut buf = String::with_capacity(other.0.len());
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

impl From<SimpleValue> for FoldedValue {
    fn from(other: SimpleValue) -> Self {
        Self(other.0)
    }
}

fn validate_simple_value(value: &str) -> Result<(), Error> {
    if !value.as_bytes().iter().all(is_valid_char) {
        return Err(Error::FieldValue(value.to_string()));
    }
    Ok(())
}

fn is_valid_char(ch: &u8) -> bool {
    ![b'\r', b'\n'].contains(ch)
}

#[cfg(test)]
mod tests {
    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use arbtest::arbtest;

    use super::*;

    #[test]
    fn invalid_simple_value() {
        assert!("hello\nworld".parse::<SimpleValue>().is_err());
        assert!("hello\rworld".parse::<SimpleValue>().is_err());
        assert!("\n".parse::<SimpleValue>().is_err());
        assert!("\r".parse::<SimpleValue>().is_err());
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
            let s: String = u.arbitrary()?;
            let s = s.replace(['\n', '\r'], " ");
            Ok(Self::try_from(s).unwrap())
        }
    }
}
