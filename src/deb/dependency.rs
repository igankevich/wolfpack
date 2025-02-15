use std::cmp::Ordering;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io::ErrorKind;
use std::ops::Deref;
use std::str::FromStr;

use crate::deb::Error;
use crate::deb::Package;
use crate::deb::PackageName;
use crate::deb::SimpleValue;
use crate::deb::Version;

#[derive(Clone, Debug, Default)]
#[cfg_attr(test, derive(PartialEq, Eq, arbitrary::Arbitrary))]
pub struct Dependencies(Vec<DependencyChoice>);

impl Dependencies {
    pub fn into_inner(self) -> Vec<DependencyChoice> {
        self.0
    }
}

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

impl Deref for Dependencies {
    type Target = [DependencyChoice];

    fn deref(&self) -> &Self::Target {
        &self.0[..]
    }
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(test, derive(PartialEq, Eq, arbitrary::Arbitrary))]
pub struct Provides(Vec<Dependency>);

impl Provides {
    pub fn matches(&self, other: &Dependency) -> bool {
        self.0.iter().any(|dep| dep.provides_matches(other))
    }
}

impl Display for Provides {
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

impl FromStr for Provides {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut deps = Vec::new();
        let value = value.trim();
        if !value.is_empty() {
            for dep in value.split(',') {
                let dep: Dependency = dep.trim().parse()?;
                deps.push(dep);
            }
        }
        Ok(Self(deps))
    }
}

impl Deref for Provides {
    type Target = [Dependency];

    fn deref(&self) -> &Self::Target {
        &self.0[..]
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

    pub fn matches(&self, package: &Package) -> bool {
        self.0.iter().any(|dep| dep.matches(package))
    }

    pub fn version_matches(&self, package_name: &str, package_version: &Version) -> bool {
        self.0
            .iter()
            .any(|dep| dep.version_matches(package_name, package_version))
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

impl Deref for DependencyChoice {
    type Target = [Dependency];

    fn deref(&self) -> &Self::Target {
        &self.0[..]
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, arbitrary::Arbitrary))]
pub struct Dependency {
    pub name: PackageName,
    pub arch: Option<DependencyArch>,
    pub version: Option<DependencyVersion>,
}

impl Dependency {
    pub fn matches(&self, package: &Package) -> bool {
        if self.name != package.name {
            // Name doesn't match, but maybe "Provides" match.
            let Some(provides) = package.provides.as_ref() else {
                return false;
            };
            if !provides.matches(self) {
                return false;
            }
        }
        if let Some(version) = self.version.as_ref() {
            // Check versions.
            return version.matches(&package.version);
        }
        true
    }

    pub fn version_matches(&self, package_name: &str, package_version: &Version) -> bool {
        if self.name.as_str() != package_name {
            // Name doesn't match.
            return false;
        }
        if let Some(version) = self.version.as_ref() {
            // Check versions.
            return version.matches(package_version);
        }
        true
    }

    pub fn provides_matches(&self, other: &Dependency) -> bool {
        // TODO arch?
        if self.name != other.name {
            return false;
        }
        if let Some(version) = self.version.as_ref() {
            if version.operator != DependencyVersionOp::Equal {
                // Only `=` is permitted in `Provides`.
                return false;
            }
            if let Some(other_version) = other.version.as_ref() {
                return other_version.matches(&version.version);
            }
        }
        true
    }
}

impl Display for Dependency {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(arch) = self.arch.as_ref() {
            write!(f, ":{}", arch)?;
        }
        if let Some(version) = self.version.as_ref() {
            write!(f, " ({})", version)?;
        }
        Ok(())
    }
}

impl FromStr for Dependency {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let n = value.as_bytes().len();
        let (version, version_range) = {
            let range = if value.ends_with(')') {
                value
                    .as_bytes()
                    .iter()
                    .position(|ch| *ch == b'(')
                    .map(|i| i + 1..n - 1)
                    .unwrap_or(0..0)
            } else {
                0..0
            };
            let version: Option<DependencyVersion> = if !range.is_empty() {
                Some(value[range.clone()].parse()?)
            } else {
                None
            };
            (version, range)
        };
        let (arch, arch_range) = {
            let arch_end = if version_range.is_empty() {
                n
            } else {
                version_range.start - 1
            };
            let range = value[..arch_end]
                .as_bytes()
                .iter()
                .position(|ch| *ch == b':')
                .map(|i| i + 1..arch_end)
                .unwrap_or(0..0);
            let arch: Option<DependencyArch> = if !range.is_empty() {
                Some(value[range.clone()].trim().parse()?)
            } else {
                None
            };
            (arch, range)
        };
        let name = {
            let name_end = if !arch_range.is_empty() {
                arch_range.start - 1
            } else if !version_range.is_empty() {
                version_range.start - 1
            } else {
                n
            };
            let range = 0..name_end;
            let name: PackageName = value[range].trim().parse()?;
            name
        };
        Ok(Self {
            name,
            arch,
            version,
        })
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, arbitrary::Arbitrary))]
pub struct DependencyVersion {
    pub operator: DependencyVersionOp,
    pub version: Version,
}

impl DependencyVersion {
    pub fn matches(&self, package: &Version) -> bool {
        use DependencyVersionOp::*;
        let ordering = package.cmp(&self.version);
        match self.operator {
            Lesser => ordering == Ordering::Less,
            LesserEqual => ordering == Ordering::Less || ordering == Ordering::Equal,
            Equal => ordering == Ordering::Equal,
            Greater => ordering == Ordering::Greater,
            GreaterEqual => ordering == Ordering::Greater || ordering == Ordering::Equal,
        }
    }
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
        let version: Version = version.parse()?;
        if iter.next().is_some() {
            return Err(ErrorKind::InvalidData.into());
        }
        Ok(Self { operator, version })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
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

// TODO custom Arbitrary
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, arbitrary::Arbitrary))]
pub enum DependencyArch {
    Any,
    All,
    One(SimpleValue),
}

impl Display for DependencyArch {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        use DependencyArch::*;
        match self {
            Any => write!(f, "any"),
            All => write!(f, "all"),
            One(arch) => write!(f, "{}", arch),
        }
    }
}

impl FromStr for DependencyArch {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        use DependencyArch::*;
        match value {
            "any" => Ok(Any),
            "all" => Ok(All),
            other => Ok(One(other.parse()?)),
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
    fn dependency_arch_symmetry() {
        to_string_parse_symmetry::<DependencyArch>();
    }

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

    //impl<'a> Arbitrary<'a> for DependencyArch {
    //    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
    //        let mut arch: DependencyArch = u.arbitrary()?;
    //        match arch {
    //            DependencyArch::One(ref mut arch) => {
    //                if arch.as_str() == "all" || arch.as_str() == "one" {
    //                    *arch = "x".parse::<SimpleValue>().unwrap();
    //                }
    //            }
    //            _ => {}
    //            //DependencyArch::Set(ref mut arches) => {
    //            //    if arches.is_empty() {
    //            //        arches.push(u.arbitrary()?);
    //            //    }
    //            //}
    //        }
    //        Ok(arch)
    //    }
    //}
}
