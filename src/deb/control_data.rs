use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;

use crate::deb::Error;
use crate::deb::FieldName;
use crate::deb::FoldedValue;
use crate::deb::MultilineValue;
use crate::deb::PackageName;
use crate::deb::PackageVersion;
use crate::deb::SimpleValue;

#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct ControlData {
    package: PackageName,
    version: PackageVersion,
    pub architecture: SimpleValue,
    maintainer: SimpleValue,
    description: MultilineValue,
    installed_size: Option<u64>,
    other: Fields,
}

impl Display for ControlData {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        writeln!(f, "Package: {}", self.package)?;
        writeln!(f, "Version: {}", self.version)?;
        writeln!(f, "Architecture: {}", self.architecture)?;
        writeln!(f, "Maintainer: {}", self.maintainer)?;
        if let Some(installed_size) = self.installed_size.as_ref() {
            writeln!(f, "Installed-Size: {}", installed_size)?;
        }
        for (name, value) in self.other.fields.iter() {
            writeln!(f, "{}: {}", name, value)?;
        }
        write!(f, "Description: {}", self.description)?;
        Ok(())
    }
}

impl FromStr for ControlData {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut state = ParserStatus::Initial;
        let mut fields = Fields::new();
        for line in value.lines() {
            if line.starts_with('#') {
                continue;
            }
            if line.chars().all(char::is_whitespace) {
                return Err(Error::ControlData("empty line".into()));
            }
            state = state.advance(Some(line), &mut fields)?;
        }
        state.advance(None, &mut fields)?;
        let control = ControlData {
            package: fields.remove("package")?.try_into()?,
            version: fields.remove("version")?.try_into()?,
            architecture: fields.remove("architecture")?.try_into()?,
            description: fields.remove("description")?.try_into()?,
            maintainer: fields.remove("maintainer")?.try_into()?,
            installed_size: {
                let option = fields.remove("installed-size").ok().map(|x| {
                    let value: String = x.to_string();
                    value.parse::<u64>().map_err(|_| Error::FieldValue(value))
                });
                match option {
                    Some(result) => Some(result?),
                    None => None,
                }
            },
            other: fields,
        };
        Ok(control)
    }
}

fn is_multiline(name: &FieldName) -> bool {
    name == "description"
}

enum ParserStatus {
    Initial,
    Reading(FieldName, String, usize, bool),
}

impl ParserStatus {
    fn advance(self, line: Option<&str>, fields: &mut Fields) -> Result<Self, Error> {
        let state = match (self, line) {
            (ParserStatus::Initial, Some(line)) => {
                let mut iter = line.splitn(2, ':');
                let name = iter.next().ok_or_else(|| Error::ControlData(line.into()))?;
                let value = iter.next().ok_or_else(|| Error::ControlData(line.into()))?;
                let value = value.trim_start();
                let name: FieldName = name.parse()?;
                if !value.is_empty() {
                    ParserStatus::Reading(name, value.into(), 1, false)
                } else {
                    ParserStatus::Initial
                }
            }
            (ParserStatus::Reading(name, mut value, num_lines, has_empty_lines), Some(line))
                if line.starts_with([' ', '\t']) =>
            {
                //let line = &line[1..];
                let has_empty_lines = has_empty_lines || line == " ." || line == "\t.";
                eprintln!("line {:?} empty {:?}", line, has_empty_lines);
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
                use std::collections::hash_map::Entry;
                match fields.fields.entry(name) {
                    Entry::Occupied(o) => return Err(Error::DuplicateField(o.key().to_string())),
                    Entry::Vacant(v) => {
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

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
enum Value {
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

impl TryFrom<Value> for SimpleValue {
    type Error = Error;

    fn try_from(other: Value) -> Result<Self, Self::Error> {
        match other {
            Value::Simple(value) => Ok(value),
            Value::Folded(value) => Ok(value.into()),
            Value::Multiline(..) => Err(Error::ControlData(
                "expected simple value, received multiline".into(),
            )),
        }
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

impl TryFrom<Value> for PackageName {
    type Error = Error;

    fn try_from(other: Value) -> Result<Self, Self::Error> {
        match other {
            Value::Simple(value) => value.try_into(),
            _ => Err(Error::ControlData(
                "expected simple value, received multiline/folded".into(),
            )),
        }
    }
}

impl TryFrom<Value> for PackageVersion {
    type Error = Error;

    fn try_from(other: Value) -> Result<Self, Self::Error> {
        match other {
            Value::Simple(value) => value.try_into(),
            _ => Err(Error::ControlData(
                "expected simple value, received multiline/folded".into(),
            )),
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            Value::Simple(value) => write!(f, "{}", value),
            Value::Folded(value) => write!(f, "{}", value),
            Value::Multiline(value) => write!(f, "{}", value),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
struct Fields {
    fields: HashMap<FieldName, Value>,
}

impl Fields {
    fn new() -> Self {
        Self {
            fields: Default::default(),
        }
    }

    fn remove(&mut self, name: &'static str) -> Result<Value, Error> {
        self.fields
            .remove(&FieldName::new_unchecked(name))
            .ok_or_else(|| Error::MissingField(name))
    }
}

#[cfg(test)]
mod tests {
    use arbtest::arbtest;

    use super::*;

    #[test]
    fn value_eq() {
        arbtest(|u| {
            let simple: SimpleValue = u.arbitrary()?;
            let value1 = Value::Simple(simple.clone());
            let value2 = Value::Folded(simple.into());
            assert_eq!(value1, value2);
            Ok(())
        });
    }

    #[test]
    fn display_parse() {
        arbtest(|u| {
            let expected: ControlData = u.arbitrary()?;
            let string = expected.to_string();
            let actual: ControlData = string
                .parse()
                .unwrap_or_else(|_| panic!("string = {:?}", string));
            assert_eq!(expected, actual, "string = {:?}", string);
            Ok(())
        }); //.seed(0x6c1a446e0000044f);
    }

    // TODO display object difference, i.e. assert_eq_diff, DebugDiff trait
}
