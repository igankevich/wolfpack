use std::collections::HashSet;
use std::fmt::Display;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

use crate::deb::Error;
use crate::deb::FoldedValue;
use crate::deb::MultilineValue;
use crate::deb::SimpleValue;
use crate::macros::define_try_from_string_from_string;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
#[serde(try_from = "String", into = "String")]
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

impl FromStr for Value {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value: SimpleValue = value.parse()?;
        Ok(Value::Simple(value))
    }
}

define_try_from_string_from_string!(Value);
