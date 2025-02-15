use chrono::DateTime;
use std::collections::hash_map::Entry::*;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fmt::Display;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::SystemTime;

use crate::deb::Error;
use crate::deb::FieldName;
use crate::deb::Value;

#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct Fields {
    fields: HashMap<FieldName, Value>,
}

impl Fields {
    pub fn new() -> Self {
        Self {
            fields: Default::default(),
        }
    }

    pub fn remove_any(&mut self, name: &'static str) -> Result<Value, Error> {
        self.fields
            .remove(&FieldName::new_unchecked(name))
            .ok_or(Error::MissingField(name))
    }

    pub fn remove<T: FromStr>(&mut self, name: &'static str) -> Result<T, Error>
    where
        <T as FromStr>::Err: Display,
    {
        let value = self
            .fields
            .remove(&FieldName::new_unchecked(name))
            .ok_or(Error::MissingField(name))?;
        value
            .as_str()
            .parse::<T>()
            .map_err(|e| Error::FieldValue(name, value.to_string(), e.to_string()))
    }

    pub fn remove_some<T: FromStr>(&mut self, name: &'static str) -> Result<Option<T>, Error>
    where
        <T as FromStr>::Err: Display,
    {
        self.fields
            .remove(&FieldName::new_unchecked(name))
            .map(|value| {
                value
                    .as_str()
                    .parse::<T>()
                    .map_err(|e| Error::FieldValue(name, value.to_string(), e.to_string()))
            })
            .transpose()
    }

    pub fn remove_system_time(&mut self, name: &'static str) -> Result<Option<SystemTime>, Error> {
        let Some(value) = self.fields.remove(&FieldName::new_unchecked(name)) else {
            return Ok(None);
        };
        let value = value.as_str().replace("UTC", "+0000");
        match DateTime::parse_from_rfc2822(&value) {
            Ok(date) => Ok(Some(date.into())),
            Err(e) => {
                log::error!("Failed to parse date {:?}: {}", value, e);
                Ok(None)
            }
        }
    }

    pub fn remove_hashes<H: FromStr>(
        &mut self,
        name: &'static str,
    ) -> Result<HashMap<PathBuf, (H, u64)>, Error> {
        let mut hashes = HashMap::new();
        let Some(value) = self.fields.remove(&FieldName::new_unchecked(name)) else {
            return Ok(hashes);
        };
        for line in value.as_str().lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut values = line.split_whitespace();
            let hash: H = values
                .next()
                .ok_or_else(|| Error::other("file hash is missing"))?
                .parse()
                .map_err(|_| Error::other("failed to parse file hash"))?;
            let size: u64 = values
                .next()
                .ok_or_else(|| Error::other("file size is missing"))?
                .parse::<u64>()
                .map_err(|_| Error::other("failed to parse file size"))?;
            let path: PathBuf = values
                .next()
                .ok_or_else(|| Error::other("file path is missing"))?
                .into();
            hashes.insert(path, (hash, size));
        }
        Ok(hashes)
    }

    pub fn clear(&mut self) {
        self.fields.clear();
    }
}

impl Default for Fields {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Fields {
    type Target = HashMap<FieldName, Value>;

    fn deref(&self) -> &Self::Target {
        &self.fields
    }
}

impl FromStr for Fields {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut state = ParserStatus::Initial;
        let mut fields = Fields::new();
        for line in value.lines() {
            if line.starts_with('#') {
                continue;
            }
            if line.chars().all(char::is_whitespace) {
                return Err(Error::Package("empty line".into()));
            }
            state = state.advance(Some(line), &mut fields)?;
        }
        state.advance(None, &mut fields)?;
        Ok(fields)
    }
}

#[derive(Debug)]
enum ParserStatus {
    Initial,
    Reading(FieldName, String, usize, bool),
}

impl ParserStatus {
    fn advance(self, line: Option<&str>, fields: &mut Fields) -> Result<Self, Error> {
        let state = match (self, line) {
            (ParserStatus::Initial, Some(line)) => {
                let mut iter = line.splitn(2, ':');
                let name = iter.next().ok_or_else(|| Error::Package(line.into()))?;
                let value = iter.next().ok_or_else(|| Error::Package(line.into()))?;
                let value = value.trim_start();
                let name: FieldName = name.parse()?;
                if !value.is_empty() || is_multiline(&name) {
                    ParserStatus::Reading(name, value.into(), 1, false)
                } else {
                    ParserStatus::Initial
                }
            }
            (ParserStatus::Reading(name, mut value, num_lines, has_empty_lines), Some(line))
                if line.starts_with([' ', '\t']) =>
            {
                let has_empty_lines = has_empty_lines || line == " ." || line == "\t.";
                value.push('\n');
                value.push_str(line);
                ParserStatus::Reading(name, value, num_lines + 1, has_empty_lines)
            }
            (ParserStatus::Reading(name, value, num_lines, has_empty_lines), line) => {
                let value = if num_lines == 1 {
                    Value::Simple(value.parse()?)
                } else if has_empty_lines || is_multiline(&name) {
                    Value::Multiline(value.into())
                } else {
                    Value::Folded(value.try_into()?)
                };
                match fields.fields.entry(name) {
                    Occupied(o) => return Err(Error::DuplicateField(o.key().to_string())),
                    Vacant(v) => {
                        v.insert(value);
                    }
                }
                if line.is_some() {
                    ParserStatus::Initial.advance(line, fields)?
                } else {
                    ParserStatus::Initial
                }
            }
            (state @ ParserStatus::Initial, None) => state,
        };
        Ok(state)
    }
}

fn is_multiline(name: &FieldName) -> bool {
    name == "description" || name == "md5sum" || name == "sha256" || name == "sha1"
}
