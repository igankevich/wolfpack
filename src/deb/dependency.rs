use crate::deb::SimpleValue;
use crate::deb::Version;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io::Error;
use std::io::ErrorKind;
use std::str::FromStr;

#[derive(Clone, Debug, Default)]
#[cfg_attr(test, derive(PartialEq, Eq, arbitrary::Arbitrary))]
pub struct Dependencies(Vec<DependencyChoice>);

impl Display for Dependencies {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let mut iter = self.0.iter();
        if let Some(dep) = iter.next() {
            write!(f, "{}", dep)?;
        }
        for dep in iter {
            write!(f, ", {}", dep)?;
        }
        Ok(())
    }
}

impl FromStr for Dependencies {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut deps = Vec::new();
        let value = value.trim();
        if !value.is_empty() {
            for dep in value.split(',') {
                let dep: DependencyChoice = dep.trim().parse()?;
                deps.push(dep);
            }
        }
        Ok(Self(deps))
    }
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct DependencyChoice(Vec<Dependency>);

impl DependencyChoice {
    pub fn new(deps: Vec<Dependency>) -> Result<Self, Error> {
        if deps.is_empty() {
            return Err(ErrorKind::InvalidData.into());
        }
        Ok(Self(deps))
    }
}

impl Display for DependencyChoice {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let mut iter = self.0.iter();
        if let Some(dep) = iter.next() {
            write!(f, "{}", dep)?;
        }
        for dep in iter {
            write!(f, " | {}", dep)?;
        }
        Ok(())
    }
}

impl FromStr for DependencyChoice {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut deps = Vec::new();
        let value = value.trim();
        if !value.is_empty() {
            for dep in value.split('|') {
                let dep: Dependency = dep.trim().parse()?;
                deps.push(dep);
            }
        }
        Ok(Self(deps))
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, arbitrary::Arbitrary))]
pub struct Dependency {
    pub name: SimpleValue,
    pub version: Option<DependencyVersion>,
}

impl Display for Dependency {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(version) = self.version.as_ref() {
            write!(f, " ({})", version)?;
        }
        Ok(())
    }
}

impl FromStr for Dependency {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let i = value.as_bytes().iter().position(|ch| *ch == b'(');
        let (name, version) = if i.is_some() && value.ends_with(')') {
            let i = i.unwrap();
            let name: SimpleValue = value[..i].trim().parse().map_err(Error::other)?;
            let version: DependencyVersion = value[(i + 1)..(value.len() - 1)].parse()?;
            (name, Some(version))
        } else {
            let name: SimpleValue = value.parse().map_err(Error::other)?;
            (name, None)
        };
        Ok(Self { name, version })
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, arbitrary::Arbitrary))]
pub struct DependencyVersion {
    pub operator: DependencyVersionOp,
    pub version: Version,
}

impl Display for DependencyVersion {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{} {}", self.operator, self.version)
    }
}

impl FromStr for DependencyVersion {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut iter = value.split_ascii_whitespace();
        let operator = iter.next().ok_or(ErrorKind::InvalidData)?;
        let operator: DependencyVersionOp = operator.parse()?;
        let version = iter.next().ok_or(ErrorKind::InvalidData)?;
        let version: Version = version.parse().map_err(Error::other)?;
        if iter.next().is_some() {
            return Err(ErrorKind::InvalidData.into());
        }
        Ok(Self { operator, version })
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, arbitrary::Arbitrary))]
pub enum DependencyVersionOp {
    /// `<<`
    Lesser,
    /// `<=`
    LesserEqual,
    /// `=`
    Equal,
    /// `>>`
    Greater,
    /// `>=`
    GreaterEqual,
}

impl DependencyVersionOp {
    pub fn as_str(&self) -> &str {
        use DependencyVersionOp::*;
        match self {
            Lesser => "<<",
            LesserEqual => "<=",
            Equal => "=",
            Greater => ">>",
            GreaterEqual => ">=",
        }
    }
}

impl Display for DependencyVersionOp {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for DependencyVersionOp {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        use DependencyVersionOp::*;
        match value {
            "<<" => Ok(Lesser),
            "<=" => Ok(LesserEqual),
            "=" => Ok(Equal),
            ">>" => Ok(Greater),
            ">=" => Ok(GreaterEqual),
            _ => Err(ErrorKind::InvalidData.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::to_string_parse_symmetry;
    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;

    #[test]
    fn dependency_version_op_symmery() {
        to_string_parse_symmetry::<DependencyVersionOp>();
    }

    #[test]
    fn dependency_version_symmetry() {
        to_string_parse_symmetry::<DependencyVersion>();
    }

    #[test]
    fn dependency_symmetry() {
        to_string_parse_symmetry::<Dependency>();
    }

    #[test]
    fn dependency_choice_symmetry() {
        to_string_parse_symmetry::<DependencyChoice>();
    }

    #[test]
    fn dependencies_symmetry() {
        to_string_parse_symmetry::<Dependency>();
    }

    impl<'a> Arbitrary<'a> for DependencyChoice {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let mut deps: Vec<Dependency> = u.arbitrary()?;
            if deps.is_empty() {
                deps.push(u.arbitrary()?);
            }
            Ok(Self(deps))
        }
    }
}
