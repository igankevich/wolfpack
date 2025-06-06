use std::collections::HashMap;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

use crate::deb::Error;
use crate::deb::PackageName;
use crate::deb::Version;
use crate::wolf;

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct CompactManifest {
    pub name: PackageName,
    pub origin: String,
    pub version: Version,
    pub comment: String,
    pub maintainer: String,
    pub www: String,
    pub abi: String,
    pub arch: String,
    pub prefix: PathBuf,
    pub flatsize: u32,
    pub licenselogic: LicenseLogic,
    pub licenses: Vec<String>,
    pub desc: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub deps: HashMap<PackageName, Dependency>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shlibs_required: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shlibs_provided: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
}

impl Display for CompactManifest {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let s = serde_json::to_string(self).map_err(|_| std::fmt::Error)?;
        f.write_str(&s)
    }
}

impl Debug for CompactManifest {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(
            serde_json::to_string(self)
                .map_err(|_| std::fmt::Error)?
                .as_str(),
        )
    }
}

impl FromStr for CompactManifest {
    type Err = serde_json::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(value)
    }
}

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    #[serde(flatten)]
    pub(crate) compact: CompactManifest,
    // TODO hashes
    pub(crate) files: HashMap<PathBuf, String>,
    pub(crate) config: Vec<PathBuf>,
    pub(crate) directories: HashMap<PathBuf, String>,
}

impl Display for Manifest {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let s = serde_json::to_string(self).map_err(|_| std::fmt::Error)?;
        f.write_str(&s)
    }
}

impl FromStr for Manifest {
    type Err = serde_json::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(value)
    }
}

impl TryFrom<wolf::Metadata> for CompactManifest {
    type Error = Error;
    fn try_from(other: wolf::Metadata) -> Result<Self, Self::Error> {
        Ok(Self {
            name: other.name.parse()?,
            version: other.version.parse()?,
            arch: "all".into(),
            desc: other.description,
            www: other.homepage,
            licenses: vec![other.license],
            categories: Default::default(),
            annotations: Default::default(),
            deps: Default::default(),
            comment: Default::default(),
            maintainer: Default::default(),
            flatsize: Default::default(),
            origin: Default::default(),
            prefix: Default::default(),
            shlibs_provided: Default::default(),
            shlibs_required: Default::default(),
            abi: Default::default(),
            licenselogic: LicenseLogic::Single,
        })
    }
}

/// This metadata is stored in `data.pkg` and `packagesite.pkg` files.
#[derive(Serialize, Deserialize)]
pub struct PackageMeta {
    #[serde(flatten)]
    pub(crate) compact: CompactManifest,
    // sha256
    pub(crate) sum: String,
    pub(crate) path: PathBuf,
    pub(crate) repopath: PathBuf,
    pub(crate) pkgsize: u32,
}

impl PackageMeta {
    pub fn to_vec(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

impl Display for PackageMeta {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let s = serde_json::to_string(self).map_err(|_| std::fmt::Error)?;
        f.write_str(&s)
    }
}

impl FromStr for PackageMeta {
    type Err = serde_json::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(value)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Dependency {
    pub origin: String,
    pub version: Version,
}

impl Debug for Dependency {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(
            serde_json::to_string(self)
                .map_err(|_| std::fmt::Error)?
                .as_str(),
        )
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub enum LicenseLogic {
    Single,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct SafeString(String);

impl From<SafeString> for String {
    fn from(other: SafeString) -> Self {
        other.0
    }
}

impl From<SafeString> for PathBuf {
    fn from(other: SafeString) -> Self {
        other.0.into()
    }
}

#[cfg(test)]
mod tests {
    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use rand::Rng;
    use rand_mt::Mt64;

    use super::*;
    use crate::test::Chars;
    use crate::test::CONTROL;
    use crate::test::UNICODE;

    impl<'a> Arbitrary<'a> for SafeString {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let seed: u64 = u.arbitrary()?;
            let mut rng = Mt64::new(seed);
            let valid_chars = Chars::from(UNICODE).difference(CONTROL);
            let len: usize = rng.gen_range(1..=100);
            let s = valid_chars.random_string(&mut rng, len);
            Ok(Self(s))
        }
    }

    impl<'a> Arbitrary<'a> for CompactManifest {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            Ok(Self {
                name: u.arbitrary()?,
                origin: u.arbitrary::<SafeString>()?.into(),
                version: u.arbitrary()?,
                comment: u.arbitrary::<SafeString>()?.into(),
                maintainer: u.arbitrary::<SafeString>()?.into(),
                www: u.arbitrary::<SafeString>()?.into(),
                abi: u.arbitrary::<SafeString>()?.into(),
                arch: u.arbitrary::<SafeString>()?.into(),
                prefix: u.arbitrary()?,
                flatsize: u.arbitrary()?,
                licenselogic: u.arbitrary()?,
                licenses: u
                    .arbitrary::<Vec<SafeString>>()?
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                desc: u.arbitrary::<SafeString>()?.into(),
                deps: u.arbitrary()?,
                categories: u
                    .arbitrary::<Vec<SafeString>>()?
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                shlibs_required: u
                    .arbitrary::<Vec<SafeString>>()?
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                shlibs_provided: u
                    .arbitrary::<Vec<SafeString>>()?
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                annotations: u
                    .arbitrary::<HashMap<SafeString, SafeString>>()?
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
            })
        }
    }

    impl<'a> Arbitrary<'a> for Dependency {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            Ok(Self {
                origin: u.arbitrary::<SafeString>()?.into(),
                version: u.arbitrary()?,
            })
        }
    }
}
