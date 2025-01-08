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

use crate::archive::ArchiveRead;
use crate::archive::ArchiveWrite;
use crate::deb::Error;
use crate::deb::Fields;
use crate::deb::MultilineValue;
use crate::deb::PackageName;
use crate::deb::PackageSigner;
use crate::deb::PackageVerifier;
use crate::deb::PackageVersion;
use crate::deb::SimpleValue;
use crate::deb::DEBIAN_BINARY_CONTENTS;
use crate::deb::DEBIAN_BINARY_FILE_NAME;
use crate::sign::Signer;
use crate::sign::Verifier;

#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct Package {
    pub name: PackageName,
    pub version: PackageVersion,
    pub license: SimpleValue,
    pub architecture: SimpleValue,
    pub maintainer: SimpleValue,
    pub description: MultilineValue,
    pub installed_size: Option<u64>,
    pub other: Fields,
}

impl Package {
    pub fn name(&self) -> &PackageName {
        &self.name
    }

    pub fn write<W: Write, P: AsRef<Path>>(
        &self,
        directory: P,
        writer: W,
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

    pub fn read_control<R: Read>(reader: R, verifier: &PackageVerifier) -> Result<Package, Error> {
        let mut reader = ar::Archive::new(reader);
        let mut control: Option<Vec<u8>> = None;
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
                    message_parts[2].clear();
                    entry.read_to_end(&mut message_parts[2])?;
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
        let mut tar_archive = tar::Archive::new(AnyDecoder::new(&control[..]));
        for entry in tar_archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.normalize();
            if path == Path::new("control") {
                let mut buf = String::with_capacity(4096);
                entry.read_to_string(&mut buf)?;
                return buf.parse::<Package>();
            }
        }
        Err(Error::MissingFile("control.tar*".into()))
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
            license: fields.remove_any("license")?.try_into()?,
            architecture: fields.remove_any("architecture")?.try_into()?,
            description: fields.remove_any("description")?.try_into()?,
            maintainer: fields.remove_any("maintainer")?.try_into()?,
            installed_size: fields.remove_some("installed-size")?,
            other: fields,
        };
        Ok(control)
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
    use std::time::Duration;

    use arbtest::arbtest;
    use pgp::types::PublicKeyTrait;
    use tempfile::TempDir;

    use super::*;
    use crate::deb::PackageSigner;
    use crate::deb::PackageVerifier;
    use crate::deb::SigningKey;
    use crate::test::DirectoryOfFiles;
    use crate::test::UpperHex;

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
            assert_eq!(expected, actual, "string = {:?}", string);
            Ok(())
        });
    }

    // TODO display object difference, i.e. assert_eq_diff, DebugDiff trait

    #[test]
    fn write_read() {
        let (signing_key, verifying_key) = SigningKey::generate("wolfpack-pgp-id".into()).unwrap();
        let signer = PackageSigner::new(signing_key);
        let verifier = PackageVerifier::new(verifying_key);
        arbtest(|u| {
            let control: Package = u.arbitrary()?;
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let mut buf: Vec<u8> = Vec::new();
            control.write(directory.path(), &mut buf, &signer).unwrap();
            let actual = Package::read_control(&buf[..], &verifier).unwrap();
            assert_eq!(control, actual);
            Ok(())
        });
    }

    #[ignore]
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
                    directory.path(),
                    File::create(path.as_path()).unwrap(),
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
        })
        .budget(Duration::from_secs(10));
    }
}
