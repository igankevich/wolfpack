use std::collections::HashSet;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io::Error;
use std::path::PathBuf;

use crate::deb::FoldedValue;
use crate::deb::MultilineValue;
use crate::deb::SimpleValue;

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub enum Value {
    Simple(SimpleValue),
    Folded(FoldedValue),
    Multiline(MultilineValue),
}

impl Value {
    pub fn as_str(&self) -> &str {
        match self {
            Value::Simple(v) => v.as_str(),
            Value::Folded(v) => v.as_str(),
            Value::Multiline(v) => v.as_str(),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.as_str().eq(other.as_str())
    }
}

impl Eq for Value {}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            Value::Simple(value) => write!(f, "{}", value),
            Value::Folded(value) => write!(f, "{}", value),
            Value::Multiline(value) => write!(f, "{}", value),
        }
    }
}

impl TryFrom<Value> for HashSet<SimpleValue> {
    type Error = Error;
    fn try_from(other: Value) -> Result<Self, Self::Error> {
        match other {
            Value::Simple(v) => Ok(v.into()),
            _ => Err(Error::other("wrong value type")),
        }
    }
}

impl TryFrom<Value> for PathBuf {
    type Error = Error;
    fn try_from(other: Value) -> Result<Self, Self::Error> {
        match other {
            Value::Simple(v) => Ok(v.into()),
            _ => Err(Error::other("wrong value type")),
        }
    }
}
