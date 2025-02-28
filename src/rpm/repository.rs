use fs_err::create_dir_all;
use fs_err::File;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use quick_xml::de::from_str;
use quick_xml::errors::serialize::DeError;
//use quick_xml::se::to_writer;
use quick_xml::se::to_string;
use serde::ser::SerializeStruct;
use serde::Deserialize;
use serde::Serialize;
use serde::Serializer;
use walkdir::WalkDir;

use crate::hash::Hasher;
use crate::hash::Sha256Hash;
use crate::hash::Sha256Reader;
use crate::rpm::Arch;
use crate::rpm::Package;
use crate::rpm::PackageSigner;

pub struct Repository {
    packages: HashMap<PathBuf, (Package, Sha256Hash, Vec<PathBuf>)>,
}

impl Repository {
    pub fn new<I, P1, P2>(output_dir: P2, paths: I) -> Result<Self, std::io::Error>
    where
        I: IntoIterator<Item = P1>,
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let mut packages = HashMap::new();
        let mut push_package = |path: &Path| -> Result<(), std::io::Error> {
            let mut reader = Sha256Reader::new(File::open(path)?);
            let package = Package::read(reader.by_ref())?;
            let (hash, _size) = reader.digest()?;
            let mut filename = PathBuf::new();
            filename.push("data");
            filename.push(hash.to_string());
            create_dir_all(output_dir.as_ref().join(&filename))?;
            filename.push(path.file_name().ok_or(ErrorKind::InvalidData)?);
            let new_path = output_dir.as_ref().join(&filename);
            fs_err::rename(path, new_path)?;
            packages.insert(filename, package);
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
                    push_package(entry.path())?
                }
            } else {
                push_package(path)?
            }
        }
        Ok(Self { packages })
    }

    pub fn write<P: AsRef<Path>>(self, output_dir: P, signer: &PackageSigner) -> Result<(), Error> {
        let output_dir = output_dir.as_ref();
        let repodata = output_dir.join("repodata");
        create_dir_all(&repodata)?;
        let mut packages = Vec::new();
        for (path, (package, sha256, files)) in self.packages.into_iter() {
            packages.push(package.into_xml(path, sha256, files));
        }
        let metadata = Metadata { packages };
        // TODO hashing writer
        let mut primary_xml = Vec::<u8>::new();
        metadata.write(&mut primary_xml)?;
        let primary_xml_sha256 = sha2::Sha256::compute(&primary_xml);
        fs_err::write(repodata.join("primary.xml"), primary_xml)?;
        let repo_md = RepoMd {
            revision: 0,
            data: vec![xml::Data {
                kind: "primary".into(),
                checksum: xml::Checksum {
                    kind: "sha256".into(),
                    value: primary_xml_sha256.to_string(),
                    pkgid: None,
                },
                // TODO different for archives
                open_checksum: xml::Checksum {
                    kind: "sha256".into(),
                    value: primary_xml_sha256.to_string(),
                    pkgid: None,
                },
                location: xml::Location {
                    href: "repodata/primary.xml".into(),
                },
                timestamp: 0,
                size: 0,
                open_size: 0,
            }],
        };
        let mut repo_md_vec = Vec::new();
        repo_md.write(&mut repo_md_vec)?;
        fs_err::write(repodata.join("repomd.xml"), &repo_md_vec[..])?;
        let signature = signer
            .sign(&repo_md_vec)
            .map_err(|_| Error::other("failed to sign"))?;
        signature.write_armored(File::create(repodata.join("repomd.xml.asc"))?)?;
        Ok(())
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename = "repomd")]
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

impl Serialize for RepoMd {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("repomd", 3)?;
        state.serialize_field("revision", &self.revision)?;
        state.serialize_field("data", &self.data)?;
        state.serialize_field("@xmlns", "http://linux.duke.edu/metadata/repo")?;
        state.end()
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename = "metadata")]
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
        state.serialize_field("@xmlns", "http://linux.duke.edu/metadata/common")?;
        state.serialize_field("@xmlns:rpm", "http://linux.duke.edu/metadata/rpm")?;
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
        #[serde(rename = "@pkgid", skip_serializing_if = "Option::is_none", default)]
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
        pub arch: Arch,
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
        #[serde(rename = "license")]
        pub license: String,
        #[serde(rename = "vendor")]
        pub vendor: String,
        #[serde(rename = "group")]
        pub group: String,
        #[serde(rename = "buildhost")]
        pub buildhost: String,
        #[serde(rename = "sourcerpm")]
        pub sourcerpm: String,
        #[serde(rename = "header-range")]
        pub header_range: HeaderRange,
        #[serde(
            rename = "provides",
            default,
            skip_serializing_if = "Provides::is_empty"
        )]
        pub provides: Provides,
        #[serde(
            rename = "requires",
            default,
            skip_serializing_if = "Requires::is_empty"
        )]
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
    use fs_err::File;
    use std::process::Command;

    use arbtest::arbtest;
    use command_error::CommandExt;
    use tempfile::TempDir;

    use super::*;
    use crate::rpm::SigningKey;
    use crate::test::prevent_concurrency;
    use crate::test::DirectoryOfFiles;

    #[ignore]
    #[test]
    fn repo_md_read() {
        let input = fs_err::read_to_string("epel/repomd.xml").unwrap();
        let _repo_md = RepoMd::from_str(&input).unwrap();
    }

    #[ignore]
    #[test]
    fn primary_xml_read() {
        let input = fs_err::read_to_string(
            "epel/e6c64120dd039602f051ca362c69636284674da9ac05c21cff04a4bd990dfd0f-primary.xml",
        )
        .unwrap();
        let _metadata = Metadata::from_str(&input).unwrap();
    }

    #[ignore]
    #[test]
    fn file_lists_xml_read() {
        let input = fs_err::read_to_string(
            "epel/7497ca1a100e9ad1d4275db2af4a19790e5a2b8c9d2c7f85150806000a1d1202-filelists.xml",
        )
        .unwrap();
        let _filelists = FileLists::from_str(&input).unwrap();
    }

    #[ignore]
    #[test]
    fn other_xml_read() {
        let input = fs_err::read_to_string(
            "epel/89a9bd48b92b5a42ab40acc287ba0a09c1011a15bff8c6428931173c57dba321-other.xml",
        )
        .unwrap();
        let _otherdata = OtherData::from_str(&input).unwrap();
    }

    #[ignore = "Needs `dnf`"]
    #[test]
    fn dnf_install() {
        let _guard = prevent_concurrency("rpm");
        arbtest(|u| {
            let workdir = TempDir::new().unwrap();
            let package_file = workdir.path().join("test.rpm");
            let verifying_key_file = workdir.path().join("verifying-key");
            let _signing_key_file = workdir.path().join("signing-key");
            let (signing_key, verifying_key) = SigningKey::generate("wolfpack".into()).unwrap();
            let signer = PackageSigner::new(signing_key);
            verifying_key
                .write_armored(File::create(verifying_key_file.as_path()).unwrap())
                .unwrap();
            let mut package: Package = u.arbitrary()?;
            package.arch = Arch::X86_64;
            package.name = "test".into();
            package.version = "1.0.0".into();
            let directory: DirectoryOfFiles = u.arbitrary()?;
            package
                .clone()
                .write(
                    &mut File::create(package_file.as_path()).unwrap(),
                    directory.path(),
                    &signer,
                )
                .unwrap();
            let output_dir = workdir.path().join("repo");
            let repository = Repository::new(&output_dir, [workdir.path()]).unwrap();
            repository.write(&output_dir, &signer).unwrap();
            fs_err::write(
                "/etc/yum.repos.d/test.repo",
                format!(
                    r#"[test]
name=test
baseurl=file://{}
enabled=1
repo_gpgcheck=1
gpgcheck=1
gpgkey=file://{}
"#,
                    output_dir.display(),
                    verifying_key_file.display(),
                ),
            )
            .unwrap();
            assert!(
                Command::new("cat")
                    .arg(output_dir.join("repodata").join("repomd.xml"))
                    .status_checked()
                    .unwrap()
                    .success(),
                "package:\n========{:?}========",
                package
            );
            assert!(
                dnf()
                    .arg("--repo=test")
                    .arg("install")
                    .arg(&package.name)
                    .status_checked()
                    .unwrap()
                    .success(),
                "package:\n========{:?}========",
                package
            );
            assert!(
                dnf()
                    .arg("remove")
                    .arg(&package.name)
                    .status_checked()
                    .unwrap()
                    .success(),
                "package:\n========{:?}========",
                package
            );
            Ok(())
        });
    }

    fn dnf() -> Command {
        let mut c = Command::new("dnf");
        c.arg("--setopt=debuglevel=10");
        c.arg("--setopt=errorlevel=10");
        c.arg("--setopt=rpmverbosity=debug");
        c.arg("--setopt=assumeyes=1");
        c
    }
}
