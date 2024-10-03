use std::fmt::Display;
use std::fmt::Formatter;

use crate::deb::Error;
use crate::deb::SimpleValue;
use crate::deb::Value;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct MultilineValue(String);

impl MultilineValue {
    pub fn try_from(value: String) -> Result<Self, Error> {
        if value.is_empty() || value.starts_with(char::is_whitespace) {
            return Err(Error::FieldValue(value));
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
            _ => Err(Error::ControlData(
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

    use super::*;

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
            let mut string: String = u.arbitrary()?;
            string = string.replace('\r', "");
            if string.starts_with(char::is_whitespace) {
                string = "x".to_string() + &string;
            }
            if string.is_empty() {
                string.push('x');
            }
            Ok(Self::try_from(string).unwrap())
        }
    }
}
