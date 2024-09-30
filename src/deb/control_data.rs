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

#[derive(Clone)]
pub struct ControlData {
    package: PackageName,
    version: PackageVersion,
    pub architecture: SimpleValue,
    maintainer: SimpleValue,
    description: MultilineValue,
    installed_size: Option<u64>,
    other: HashMap<FieldName, SimpleValue>,
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
        for (name, value) in self.other.iter() {
            writeln!(f, "{}: {}", name, value)?;
        }
        write!(f, "Description: {}", self.description)?;
        Ok(())
    }
}

impl FromStr for ControlData {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut fields: HashMap<FieldName, SimpleValue> = HashMap::new();
        let mut description: Option<MultilineValue> = None;
        let mut multiline: Option<(FieldName, String)> = None;
        let mut folded: Option<(FieldName, String)> = None;
        for line in value.lines() {
            if line.starts_with('#') {
                continue;
            }
            if line.starts_with([' ', '\t']) {
                if let Some((_, value)) = folded.as_mut() {
                    value.push(' ');
                    value.push_str(&line[1..]);
                    continue;
                }
                if let Some((_, value)) = multiline.as_mut() {
                    value.push('\n');
                    value.push_str(&line[1..]);
                    continue;
                }
                return Err(Error::ControlData(line.into()));
            }
            let mut iter = line.splitn(2, ':');
            let name = iter.next().ok_or_else(|| Error::ControlData(line.into()))?;
            let value = iter.next().ok_or_else(|| Error::ControlData(line.into()))?;
            let value = value.trim_start();
            let name: FieldName = name.parse()?;
            if name == "tag" {
                folded = Some((name, value.to_string()));
            } else if name == "description" {
                multiline = Some((name, value.to_string()));
            } else if let Some((name, value)) = folded.take() {
                fields.insert(name, FoldedValue::new(value).into());
            } else if let Some((name, value)) = multiline.take() {
                let value: MultilineValue = value.parse()?;
                if name == "description" {
                    if description.is_some() {
                        return Err(Error::DuplicateField(name.to_string()));
                    }
                    description = Some(value);
                } else {
                    return Err(Error::ControlData(format!("unknown multiline `{}`", name)));
                }
            } else {
                let value: SimpleValue = value.parse()?;
                if fields.insert(name.clone(), value).is_some() {
                    return Err(Error::DuplicateField(name.to_string()));
                }
            }
        }
        if let Some((name, value)) = folded.take() {
            fields.insert(name, FoldedValue::new(value).into());
        }
        if let Some((name, value)) = multiline.take() {
            let value: MultilineValue = value.parse()?;
            if name == "description" {
                if description.is_some() {
                    return Err(Error::DuplicateField(name.to_string()));
                }
                description = Some(value);
            } else {
                return Err(Error::ControlData(format!("unknown multiline `{}`", name)));
            }
        }
        let control = ControlData {
            package: fields
                .remove(&FieldName::new_unchecked("Package"))
                .ok_or_else(|| Error::MissingField("Package"))?
                .try_into()?,
            version: fields
                .remove(&FieldName::new_unchecked("Version"))
                .ok_or_else(|| Error::MissingField("Version"))?
                .try_into()?,
            architecture: fields
                .remove(&FieldName::new_unchecked("Architecture"))
                .ok_or_else(|| Error::MissingField("Architecture"))?,
            description: description.ok_or_else(|| Error::MissingField("Description"))?,
            maintainer: fields
                .remove(&FieldName::new_unchecked("Maintainer"))
                .ok_or_else(|| Error::MissingField("Maintainer"))?,
            installed_size: {
                let option = fields
                    .remove(&FieldName::new_unchecked("Installed-Size"))
                    .map(|x| {
                        let value: String = x.into();
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
