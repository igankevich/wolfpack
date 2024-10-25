use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::create_dir_all;
use std::fs::File;
use std::io::Error;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use normalize_path::NormalizePath;
use quick_xml::de::from_str;
use quick_xml::errors::serialize::DeError;
//use quick_xml::se::to_writer;
use quick_xml::se::to_string;
use serde::ser::SerializeStruct;
use serde::Deserialize;
use serde::Serialize;
use serde::Serializer;
use walkdir::WalkDir;

use crate::hash::Sha256Hash;
use crate::rpm::Package;

pub struct Repository {
    packages: HashMap<PathBuf, (Package, Sha256Hash, Vec<PathBuf>)>,
}

impl Repository {
    pub fn new<I, P>(paths: I) -> Result<Self, std::io::Error>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut packages = HashMap::new();
        let mut push_package = |directory: &Path, path: &Path| -> Result<(), std::io::Error> {
            eprintln!("reading {}", path.display());
            let relative_path = Path::new(".").join(
                path.strip_prefix(directory)
                    .map_err(std::io::Error::other)?
                    .normalize(),
            );
            let reader = File::open(path)?;
            let package = Package::read(reader)?;
            packages.insert(relative_path, package);
            Ok(())
        };
        for path in paths.into_iter() {
            let path = path.as_ref();
            if path.is_dir() {
                for entry in WalkDir::new(path).into_iter() {
                    let entry = entry?;
                    if entry.file_type().is_dir()
                        || entry.path().extension() != Some(OsStr::new("rpm"))
                    {
                        continue;
                    }
                    push_package(path, entry.path())?
                }
            } else {
                // TODO
                push_package(Path::new("."), path)?
            }
        }
        Ok(Self { packages })
    }

    pub fn write<P: AsRef<Path>>(self, output_dir: P) -> Result<(), Error> {
        let output_dir = output_dir.as_ref();
        create_dir_all(&output_dir)?;
        create_dir_all(output_dir.join("repodata"))?;
        let mut packages = Vec::new();
        for (path, (package, sha256, files)) in self.packages.into_iter() {
            packages.push(package.into_xml(path, sha256, files));
        }
        let metadata = Metadata { packages };
        // TODO hashing writer
        let mut primary_xml = Vec::<u8>::new();
        metadata.write(&mut primary_xml)?;
        let primary_xml_sha256 = Sha256Hash::compute(&primary_xml);
        std::fs::write("repodata/primary.xml", primary_xml)?;
        let repo_md = RepoMd {
            revision: 0,
            data: vec![xml::Data {
                kind: "primary".into(),
                checksum: xml::Checksum {
                    kind: "sha256".into(),
                    value: primary_xml_sha256.to_string(),
                },
                // TODO different for archives
                open_checksum: xml::Checksum {
                    kind: "sha256".into(),
                    value: primary_xml_sha256.to_string(),
                },
                location: xml::Location {
                    href: "repodata/primary.xml",
                },
                timestamp: 0,
                size: 0,
                open_size: 0,
            }],
        };
        repo_md.write(File::create("repomd.xml")?)?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RepoMd {
    revision: u64,
    #[serde(rename = "data", default)]
    data: Vec<xml::Data>,
}

impl FromStr for RepoMd {
    type Err = DeError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        from_str(value)
    }
}

impl RepoMd {
    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        // TODO to_writer
        let s = to_string(self).map_err(Error::other)?;
        writer.write_all(s.as_bytes())
    }
}

#[derive(Deserialize, Debug)]
pub struct Metadata {
    #[serde(rename = "package", default)]
    packages: Vec<xml::Package>,
}

impl Metadata {
    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        let s = to_string(self).map_err(Error::other)?;
        writer.write_all(s.as_bytes())
    }
}

impl FromStr for Metadata {
    type Err = DeError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        from_str(value)
    }
}

impl Serialize for Metadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("metadata", 2)?;
        state.serialize_field("package", &self.packages)?;
        state.serialize_field("@packages", &self.packages.len())?;
        state.end()
    }
}

#[derive(Deserialize, Debug)]
pub struct FileLists {
    #[serde(rename = "package", default)]
    packages: Vec<xml::PackageFiles>,
}

impl FromStr for FileLists {
    type Err = DeError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        from_str(value)
    }
}

impl Serialize for FileLists {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("filelists", 2)?;
        state.serialize_field("package", &self.packages)?;
        state.serialize_field("@packages", &self.packages.len())?;
        state.end()
    }
}

#[derive(Deserialize, Debug)]
pub struct OtherData {
    #[serde(rename = "package", default)]
    packages: Vec<xml::PackageChangeLog>,
}

impl FromStr for OtherData {
    type Err = DeError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        from_str(value)
    }
}

impl Serialize for OtherData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("otherdata", 2)?;
        state.serialize_field("package", &self.packages)?;
        state.serialize_field("@packages", &self.packages.len())?;
        state.end()
    }
}

pub mod xml {
    use super::*;

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Data {
        #[serde(rename = "@type")]
        pub kind: String,
        pub checksum: Checksum,
        #[serde(rename = "open-checksum")]
        pub open_checksum: Checksum,
        pub location: Location,
        pub timestamp: u64,
        pub size: u64,
        #[serde(rename = "open-size")]
        pub open_size: u64,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Checksum {
        #[serde(rename = "@type")]
        pub kind: String,
        #[serde(rename = "$value")]
        pub value: String,
        #[serde(rename = "@pkgid")]
        pub pkgid: Option<String>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Location {
        #[serde(rename = "@href")]
        pub href: PathBuf,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Package {
        #[serde(rename = "@type")]
        pub kind: String,
        pub name: String,
        pub arch: String,
        pub version: Version,
        pub checksum: Checksum,
        pub summary: String,
        pub description: String,
        pub packager: String,
        pub url: String,
        pub time: Time,
        pub size: Size,
        pub location: Location,
        pub format: Format,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Version {
        #[serde(rename = "@epoch")]
        pub epoch: u64,
        #[serde(rename = "@ver")]
        pub version: String,
        #[serde(rename = "@rel")]
        pub release: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Size {
        #[serde(rename = "@package")]
        pub package: u64,
        #[serde(rename = "@installed")]
        pub installed: u64,
        #[serde(rename = "@archive")]
        pub archive: u64,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Time {
        #[serde(rename = "@file")]
        pub file: u64,
        #[serde(rename = "@build")]
        pub build: u64,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Format {
        pub license: String,
        pub vendor: String,
        pub group: String,
        pub buildhost: String,
        pub sourcerpm: String,
        #[serde(rename = "header-range")]
        pub header_range: HeaderRange,
        #[serde(default, skip_serializing_if = "Provides::is_empty")]
        pub provides: Provides,
        #[serde(default, skip_serializing_if = "Requires::is_empty")]
        pub requires: Requires,
        #[serde(rename = "file", default, skip_serializing_if = "Vec::is_empty")]
        pub files: Vec<PathBuf>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct HeaderRange {
        #[serde(rename = "@start")]
        pub start: u64,
        #[serde(rename = "@end")]
        pub end: u64,
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
    pub struct Provides {
        #[serde(default)]
        pub entries: Vec<ProvidesEntry>,
    }

    impl Provides {
        pub fn is_empty(&self) -> bool {
            self.entries.is_empty()
        }
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct ProvidesEntry {
        #[serde(rename = "@name")]
        pub name: String,
        #[serde(rename = "@flags")]
        pub flags: String,
        #[serde(flatten)]
        pub version: Version,
    }

    #[derive(Serialize, Deserialize, Debug, Default)]
    pub struct Requires {
        #[serde(default)]
        pub entries: Vec<RequiresEntry>,
    }

    impl Requires {
        pub fn is_empty(&self) -> bool {
            self.entries.is_empty()
        }
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct RequiresEntry {
        #[serde(rename = "@name")]
        pub name: String,
        #[serde(rename = "@pre")]
        pub pre: Option<u64>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct PackageFiles {
        #[serde(rename = "@pkgid")]
        pkgid: String,
        #[serde(rename = "@name")]
        name: String,
        #[serde(rename = "@arch")]
        arch: String,
        version: Version,
        #[serde(rename = "file", default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<FileEntry>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct FileEntry {
        #[serde(rename = "@type")]
        kind: Option<String>,
        #[serde(rename = "$value")]
        path: PathBuf,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct PackageChangeLog {
        #[serde(rename = "@pkgid")]
        pkgid: String,
        #[serde(rename = "@name")]
        name: String,
        #[serde(rename = "@arch")]
        arch: String,
        version: Version,
        #[serde(rename = "changelog", default, skip_serializing_if = "Vec::is_empty")]
        change_logs: Vec<ChangeLog>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct ChangeLog {
        #[serde(rename = "@author")]
        author: String,
        #[serde(rename = "@date")]
        date: u64,
        #[serde(rename = "$value")]
        description: String,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_md_read() {
        let input = std::fs::read_to_string("epel/repomd.xml").unwrap();
        let _repo_md = RepoMd::from_str(&input).unwrap();
    }

    #[test]
    fn primary_xml_read() {
        let input = std::fs::read_to_string(
            "epel/e6c64120dd039602f051ca362c69636284674da9ac05c21cff04a4bd990dfd0f-primary.xml",
        )
        .unwrap();
        let _metadata = Metadata::from_str(&input).unwrap();
    }

    #[test]
    fn file_lists_xml_read() {
        let input = std::fs::read_to_string(
            "epel/7497ca1a100e9ad1d4275db2af4a19790e5a2b8c9d2c7f85150806000a1d1202-filelists.xml",
        )
        .unwrap();
        let _filelists = FileLists::from_str(&input).unwrap();
    }

    #[test]
    fn other_xml_read() {
        let input = std::fs::read_to_string(
            "epel/89a9bd48b92b5a42ab40acc287ba0a09c1011a15bff8c6428931173c57dba321-other.xml",
        )
        .unwrap();
        let _otherdata = OtherData::from_str(&input).unwrap();
    }
}