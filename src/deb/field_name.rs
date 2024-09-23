use std::cmp::Ordering;
use std::fmt::Display;
use std::fmt::Formatter;
use std::hash::Hash;
use std::hash::Hasher;

use crate::deb::Error;

#[derive(Clone)]
pub struct FieldName(String);

impl FieldName {
    pub fn try_from(name: String) -> Result<Self, Error> {
        if !(name.chars().all(is_valid_char) && name.starts_with(is_valid_first_char)) {
            return Err(Error::FieldName(name));
        }
        Ok(Self(name))
    }
}

impl PartialEq for FieldName {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for FieldName {}

impl PartialOrd for FieldName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FieldName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.to_lowercase().cmp(&other.0.to_lowercase())
    }
}

impl Hash for FieldName {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.0.to_ascii_lowercase().hash(state);
    }
}

impl Display for FieldName {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn is_valid_char(ch: char) -> bool {
    ('!'..='9').contains(&ch) || (';'..='~').contains(&ch)
}

fn is_valid_first_char(ch: char) -> bool {
    !['#', '.'].contains(&ch)
}
