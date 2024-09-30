use std::borrow::Cow;
use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;

use crate::deb::Error;

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
    ![b'\r', b'\n'].contains(&ch)
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct FoldedValue(String);

impl FoldedValue {
    pub fn new<'a, V: Into<Cow<'a, str>>>(value: V) -> Self {
        let value: Cow<'_, str> = value.into();
        if value.chars().any(char::is_whitespace) {
            let mut folded = String::with_capacity(value.len());
            let mut words = value.split_whitespace();
            if let Some(word) = words.next() {
                folded.push_str(word);
            }
            for word in words {
                folded.push(' ');
                folded.push_str(word);
            }
            Self(folded)
        } else {
            Self(value.into_owned())
        }
    }
}

impl Display for FoldedValue {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for FoldedValue {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct MultilineValue(pub String);

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

impl FromStr for MultilineValue {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
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
        Ok(Self(multiline))
    }
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

    #[test]
    fn folded_value_whitespace_is_insignificant() {
        arbtest(|u| {
            let s1: String = u.arbitrary()?;
            let s2 = s1.replace(char::is_whitespace, "  ");
            let value1 = FoldedValue::new(s1);
            let value2 = FoldedValue::new(s2);
            assert_eq!(value1, value2);
            Ok(())
        });
    }

    #[test]
    fn folded_to_simple() {
        arbtest(|u| {
            let s: String = u.arbitrary()?;
            let value = FoldedValue::new(s);
            let _simple = SimpleValue::try_from(value.0).unwrap();
            Ok(())
        });
    }

    #[test]
    fn multiline_display_parse() {
        arbtest(|u| {
            let expected: MultilineValue = u.arbitrary()?;
            let string = expected.to_string();
            let actual: MultilineValue = string.parse().unwrap();
            assert_eq!(expected, actual, "string = {:?}", string);
            Ok(())
        });
    }

    impl<'a> Arbitrary<'a> for SimpleValue {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let s: String = u.arbitrary()?;
            let s = s.replace(&['\n', '\r'], " ");
            Ok(Self::try_from(s).unwrap())
        }
    }

    impl<'a> Arbitrary<'a> for MultilineValue {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            Ok(Self(u.arbitrary()?))
        }
    }
}
