use std::fmt::Display;
use std::fmt::Formatter;

use crate::deb::Error;
use crate::deb::SimpleValue;

#[derive(Clone, PartialEq, Eq, Hash)]
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

impl Display for PackageName {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<SimpleValue> for PackageName {
    type Error = Error;
    fn try_from(other: SimpleValue) -> Result<Self, Self::Error> {
        Self::try_from(other.into())
    }
}

fn is_valid_char(ch: char) -> bool {
    ch.is_ascii_lowercase() || ch.is_ascii_digit() || ['+', '-', '.'].contains(&ch)
}
