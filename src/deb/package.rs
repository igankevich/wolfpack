use std::fmt::Display;
use std::fmt::Formatter;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

use deko::bufread::AnyDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use normalize_path::NormalizePath;
use serde::Deserialize;
use serde::Serialize;

use crate::archive::ArchiveRead;
use crate::archive::ArchiveWrite;
use crate::deb::Arch;
use crate::deb::Dependencies;
use crate::deb::Error;
use crate::deb::Fields;
use crate::deb::MultilineValue;
use crate::deb::PackageName;
use crate::deb::PackageSigner;
use crate::deb::PackageVerifier;
use crate::deb::Provides;
use crate::deb::SimpleValue;
use crate::deb::Version;
use crate::deb::DEBIAN_BINARY_CONTENTS;
use crate::deb::DEBIAN_BINARY_FILE_NAME;
use crate::sign::Signer;
use crate::sign::Verifier;
use crate::wolf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq, arbitrary::Arbitrary))]
pub struct Package {
    pub name: PackageName,
    pub version: Version,
    pub license: SimpleValue,
    pub architecture: Arch,
    pub maintainer: SimpleValue,
    pub description: MultilineValue,
    #[serde(default)]
    pub installed_size: Option<u64>,
    #[serde(default)]
    pub provides: Provides,
    #[serde(default)]
    pub depends: Dependencies,
    #[serde(default)]
    pub homepage: Option<SimpleValue>,
    #[serde(flatten)]
    pub other: Fields,
}

impl Package {
    pub fn name(&self) -> &PackageName {
        &self.name
    }

    pub fn write<W: Write, P: AsRef<Path>>(
        &self,
        writer: W,
        directory: P,
        signer: &PackageSigner,
    ) -> Result<(), std::io::Error> {
        let data = TarGz::from_directory(directory, gz_writer())?.finish()?;
        let control = TarGz::from_files([("control", self.to_string())], gz_writer())?.finish()?;
        let mut message_bytes: Vec<u8> = Vec::new();
        message_bytes.extend(DEBIAN_BINARY_CONTENTS.as_bytes());
        message_bytes.extend(&control);
        message_bytes.extend(&data);
        let signature = signer
            .sign(&message_bytes[..])
            .map_err(|_| std::io::Error::other("failed to sign the archive"))?;
        ar::Builder::<W>::from_files(
            [
                (DEBIAN_BINARY_FILE_NAME, DEBIAN_BINARY_CONTENTS.as_bytes()),
                ("control.tar.gz", &control),
                ("data.tar.gz", &data),
                ("_gpgorigin", &signature),
            ],
            writer,
        )?;
        Ok(())
    }

    pub fn read<R: Read>(
        reader: R,
        verifier: &PackageVerifier,
    ) -> Result<(Package, Vec<u8>), Error> {
        let mut reader = ar::Archive::new(reader);
        let mut control: Option<Vec<u8>> = None;
        let mut data: Option<Vec<u8>> = None;
        let mut message_parts: [Vec<u8>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        let mut signatures: Vec<Vec<u8>> = Vec::new();
        reader.find(|entry| {
            let path = entry.normalized_path()?;
            match path.to_str() {
                Some(DEBIAN_BINARY_FILE_NAME) => {
                    message_parts[0].clear();
                    entry.read_to_end(&mut message_parts[0])?;
                }
                Some(path) if path.starts_with("control.tar") => {
                    if control.is_some() {
                        return Err(std::io::Error::other("multiple `control.tar*` files"));
                    }
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf)?;
                    message_parts[1] = buf.clone();
                    control = Some(buf);
                }
                Some(path) if path.starts_with("data.tar") => {
                    if data.is_some() {
                        return Err(std::io::Error::other("multiple `data.tar*` files"));
                    }
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf)?;
                    message_parts[2] = buf.clone();
                    data = Some(buf);
                }
                Some(path) if path.starts_with("_gpg") => {
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf)?;
                    signatures.push(buf);
                }
                _ => {}
            }
            Ok(None::<()>)
        })?;
        let control = control.ok_or_else(|| Error::MissingFile("control.tar*".into()))?;
        let message = message_parts
            .into_iter()
            .reduce(|mut m, part| {
                m.extend(part);
                m
            })
            .expect("array is not empty");
        if verifier
            .verify_any(&message[..], signatures.iter())
            .is_err()
        {
            return Err(Error::other("signature verification failed"));
        }
        let data = data.ok_or_else(|| Error::MissingFile("data.tar*".into()))?;
        let mut tar_archive = tar::Archive::new(AnyDecoder::new(&control[..]));
        for entry in tar_archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.normalize();
            if path == Path::new("control") {
                let mut buf = String::with_capacity(4096);
                entry.read_to_string(&mut buf)?;
                return Ok((buf.parse::<Package>()?, data));
            }
        }
        Err(Error::MissingFile("control.tar*".into()))
    }

    pub fn find(&self, keyword: &str) -> bool {
        self.name.as_str().to_lowercase().contains(keyword)
            || self.description.as_str().to_lowercase().contains(keyword)
    }

    pub fn file_name(&self) -> String {
        format!("{}_{}_{}.deb", self.name, self.version, self.architecture)
    }
}

impl Display for Package {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        writeln!(f, "Package: {}", self.name)?;
        writeln!(f, "Version: {}", self.version)?;
        writeln!(f, "License: {}", self.license)?;
        writeln!(f, "Architecture: {}", self.architecture)?;
        writeln!(f, "Maintainer: {}", self.maintainer)?;
        if let Some(installed_size) = self.installed_size.as_ref() {
            writeln!(f, "Installed-Size: {}", installed_size)?;
        }
        if !self.provides.is_empty() {
            writeln!(f, "Provides: {}", self.provides)?;
        }
        if !self.depends.is_empty() {
            writeln!(f, "Depends: {}", self.depends)?;
        }
        if let Some(homepage) = self.homepage.as_ref() {
            writeln!(f, "Homepage: {}", homepage)?;
        }
        for (name, value) in self.other.iter() {
            writeln!(f, "{}: {}", name, value)?;
        }
        writeln!(f, "Description: {}", self.description)?;
        Ok(())
    }
}

impl FromStr for Package {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut fields: Fields = value.parse()?;
        let control = Package {
            name: fields.remove_any("package")?.try_into()?,
            version: fields.remove_any("version")?.try_into()?,
            license: fields.remove_some("license")?.unwrap_or_default(),
            architecture: fields.remove_any("architecture")?.as_str().parse()?,
            description: fields.remove_any("description")?.try_into()?,
            maintainer: fields.remove_any("maintainer")?.try_into()?,
            installed_size: fields.remove_some("installed-size")?,
            provides: fields.remove_some("provides")?.unwrap_or_default(),
            depends: fields.remove_some("depends")?.unwrap_or_default(),
            homepage: fields.remove_some("homepage")?,
            other: fields,
        };
        Ok(control)
    }
}

impl TryFrom<wolf::Metadata> for Package {
    type Error = Error;
    fn try_from(other: wolf::Metadata) -> Result<Self, Self::Error> {
        Ok(Self {
            name: other.name.parse()?,
            version: other.version.parse()?,
            architecture: other.arch.into(),
            description: other.description.into(),
            homepage: Some(other.homepage.parse()?),
            license: other.license.parse()?,
            depends: Default::default(),
            provides: Default::default(),
            maintainer: Default::default(),
            other: Default::default(),
            installed_size: Default::default(),
        })
    }
}

type TarGz = tar::Builder<GzEncoder<Vec<u8>>>;

fn gz_writer() -> GzEncoder<Vec<u8>> {
    GzEncoder::new(Vec::new(), Compression::best())
}

#[cfg(test)]
mod tests {
    use std::fs::create_dir_all;
    use std::fs::remove_dir_all;
    use std::fs::File;
    use std::process::Command;
    use std::process::Stdio;

    use arbtest::arbtest;
    use pgp::types::PublicKeyTrait;
    use tempfile::TempDir;

    use super::*;
    use crate::deb::PackageSigner;
    use crate::deb::PackageVerifier;
    use crate::deb::SigningKey;
    use crate::deb::Value;
    use crate::hash::UpperHex;
    use crate::test::DirectoryOfFiles;

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
            let expected: Package = u.arbitrary()?;
            let string = expected.to_string();
            let actual: Package = string
                .parse()
                .unwrap_or_else(|_| panic!("string = {:?}", string));
            similar_asserts::assert_eq!(expected, actual, "string = {:?}", string);
            Ok(())
        });
    }

    #[test]
    fn write_read() {
        let (signing_key, verifying_key) = SigningKey::generate("wolfpack-pgp-id".into()).unwrap();
        let signer = PackageSigner::new(signing_key);
        let verifier = PackageVerifier::new(verifying_key);
        arbtest(|u| {
            let control: Package = u.arbitrary()?;
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let mut buf: Vec<u8> = Vec::new();
            control.write(&mut buf, directory.path(), &signer).unwrap();
            let (actual, ..) = Package::read(&buf[..], &verifier).unwrap();
            similar_asserts::assert_eq!(control, actual);
            Ok(())
        });
    }

    #[ignore = "Needs `dpkg`"]
    #[test]
    fn dpkg_installs_random_packages() {
        let (signing_key, verifying_key) = SigningKey::generate("wolfpack-pgp-id".into()).unwrap();
        let signer = PackageSigner::new(signing_key);
        let workdir = TempDir::new().unwrap();
        let root = workdir.path().join("root");
        let debsig_keyrings = root.join("usr/share/debsig/keyrings");
        let debsig_policies = root.join("etc/debsig/policies");
        let verifying_key_file = workdir.path().join("verifying-key");
        let fingerprint = verifying_key.fingerprint();
        let verifying_key_hex = UpperHex(fingerprint.as_bytes());
        let keyring_file = debsig_keyrings.join(format!("{}/debsig.gpg", verifying_key_hex));
        let policy_file = debsig_policies.join(format!("{}/debsig.pol", verifying_key_hex));
        arbtest(|u| {
            let mut control: Package = u.arbitrary()?;
            control.architecture = "all".parse().unwrap();
            control.depends.clear();
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let path = workdir.path().join("test.deb");
            let _ = remove_dir_all(root.as_path());
            create_dir_all(debsig_keyrings.as_path()).unwrap();
            create_dir_all(debsig_policies.as_path()).unwrap();
            create_dir_all(keyring_file.parent().unwrap()).unwrap();
            create_dir_all(policy_file.parent().unwrap()).unwrap();
            std::fs::write(
                policy_file.as_path(),
                format!(
                    r#"<?xml version="1.0"?>
<!DOCTYPE Policy SYSTEM "http://www.debian.org/debsig/1.0/policy.dtd">
<Policy xmlns="https://www.debian.org/debsig/1.0/">
  <Origin Name="test" id="{0}" Description="Test package"/>
  <Selection>
    <Required Type="origin" File="debsig.gpg" id="{0}"/>
  </Selection>
  <Verification MinOptional="0">
    <Required Type="origin" File="debsig.gpg" id="{0}"/>
  </Verification>
</Policy>
"#,
                    verifying_key_hex
                ),
            )
            .unwrap();
            verifying_key
                .to_armored_writer(
                    &mut File::create(verifying_key_file.as_path()).unwrap(),
                    Default::default(),
                )
                .unwrap();
            control
                .write(
                    File::create(path.as_path()).unwrap(),
                    directory.path(),
                    &signer,
                )
                .unwrap();
            assert!(
                Command::new("gpg")
                    .arg("--dearmor")
                    .arg("--output")
                    .arg(keyring_file.as_path())
                    .arg(verifying_key_file.as_path())
                    .status()
                    .unwrap()
                    .success(),
                "control:\n========{}========",
                control
            );
            assert!(
                Command::new("debsig-verify")
                    .arg("--debug")
                    .arg("--root")
                    .arg(root.as_path())
                    .arg(path.as_path())
                    .status()
                    .unwrap()
                    .success(),
                "control:\n========{}========",
                control
            );
            assert!(
                Command::new("dpkg")
                    .arg("--root")
                    .arg(root.as_path())
                    .arg("--install")
                    .arg(path.as_path())
                    .status()
                    .unwrap()
                    .success(),
                "control:\n========{}========",
                control
            );
            assert!(
                Command::new("dpkg-query")
                    .arg("--root")
                    .arg(root.as_path())
                    .arg("-L")
                    .arg(control.name().as_str())
                    .stdout(Stdio::null())
                    .status()
                    .unwrap()
                    .success(),
                "control:\n========{}========",
                control
            );
            Ok(())
        });
    }
}
