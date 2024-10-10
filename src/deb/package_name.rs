use std::fmt::Display;
use std::fmt::Formatter;
use std::ops::Deref;
use std::str::FromStr;

use crate::deb::Error;
use crate::deb::SimpleValue;
use crate::deb::Value;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PackageName(String);

impl PackageName {
    pub fn try_from(name: String) -> Result<Self, Error> {
        if !(name.chars().all(is_valid_char)
            && name.chars().next().iter().all(char::is_ascii_alphanumeric)
            && (name.len() >= 2))
        {
            return Err(Error::PackageName(name));
        }
        Ok(Self(name))
    }
}

impl Deref for PackageName {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for PackageName {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PackageName {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_string())
    }
}

impl TryFrom<SimpleValue> for PackageName {
    type Error = Error;
    fn try_from(other: SimpleValue) -> Result<Self, Self::Error> {
        Self::try_from(other.into())
    }
}

impl From<PackageName> for String {
    fn from(other: PackageName) -> Self {
        other.0
    }
}

impl TryFrom<Value> for PackageName {
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

fn is_valid_char(ch: char) -> bool {
    ch.is_ascii_lowercase() || ch.is_ascii_digit() || ['+', '-', '.'].contains(&ch)
}

#[cfg(test)]
mod tests {
    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use arbtest::arbtest;

    use super::*;
    use crate::test::chars;

    #[test]
    fn invalid_names() {
        assert!("#hello".parse::<PackageName>().is_err());
        assert!("-hello".parse::<PackageName>().is_err());
        assert!("+hello".parse::<PackageName>().is_err());
        assert!(".hello".parse::<PackageName>().is_err());
        assert!("".parse::<PackageName>().is_err());
        assert!("x".parse::<PackageName>().is_err());
    }

    #[test]
    fn valid_names() {
        arbtest(|u| {
            let _value: PackageName = u.arbitrary()?;
            Ok(())
        });
    }

    #[test]
    fn package_name_to_simple() {
        arbtest(|u| {
            let expected: PackageName = u.arbitrary()?;
            let simple1 = SimpleValue::new(expected.0.clone()).unwrap();
            let simple2: SimpleValue = expected.into();
            assert_eq!(simple1, simple2);
            Ok(())
        });
    }

    impl<'a> Arbitrary<'a> for PackageName {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let valid_first_chars = chars!('a'..='z', '0'..='9');
            let valid_chars = valid_first_chars.union(&chars!(&['+', '-', '.']));
            let len = u.int_in_range(2..=100)?;
            let mut s = valid_chars.arbitrary_string(u, len - 1)?;
            s.insert(0, valid_first_chars.arbitrary_char(u)?);
            Ok(Self::try_from(s).unwrap())
        }
    }
}
