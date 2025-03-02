use fs_err::File;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io::BufReader;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use deko::bufread::AnyDecoder;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use normalize_path::NormalizePath;

use crate::archive::ArchiveRead;
use crate::archive::ArchiveWrite;
use crate::deb::Dependencies;
use crate::deb::Fields;
use crate::deb::MultilineValue;
use crate::deb::PackageName;
use crate::deb::ParseField;
use crate::deb::Provides;
use crate::deb::SimpleValue;
use crate::deb::Version;
use crate::deb::DEBIAN_BINARY_CONTENTS;
use crate::deb::DEBIAN_BINARY_FILE_NAME;
use crate::ipk::Arch;
use crate::ipk::Error;
use crate::ipk::PackageSigner;
use crate::ipk::PackageVerifier;
use crate::sign::SignatureWriter;
use crate::sign::VerifyingReader;
use crate::wolf;

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, arbitrary::Arbitrary))]
pub struct Package {
    pub name: PackageName,
    pub version: Version,
    pub license: SimpleValue,
    pub arch: Arch,
    pub maintainer: SimpleValue,
    pub description: MultilineValue,
    pub installed_size: Option<u64>,
    pub provides: Provides,
    pub depends: Dependencies,
    pub other: Fields,
}

impl Package {
    pub fn write<P1: AsRef<Path>, P2: Into<PathBuf>>(
        &self,
        output_file: P2,
        directory: P1,
        signer: &PackageSigner,
    ) -> Result<(), std::io::Error> {
        let output_file: PathBuf = output_file.into();
        let writer = File::create(output_file.as_path())?;
        let signature_output_file = to_signature_path(output_file);
        let writer = SignatureWriter::new(writer, signer, signature_output_file);
        let writer = GzEncoder::new(writer, Compression::best());
        let data = tar::Builder::from_directory(directory, gz_writer())?.finish()?;
        let control =
            tar::Builder::from_files([("control", self.to_string())], gz_writer())?.finish()?;
        tar::Builder::from_files(
            [
                (DEBIAN_BINARY_FILE_NAME, DEBIAN_BINARY_CONTENTS.as_bytes()),
                ("control.tar.gz", &control),
                ("data.tar.gz", &data),
            ],
            writer,
        )?
        .finish()?
        .write_signature()?;
        Ok(())
    }

    pub fn read_control<R: Read, P: AsRef<Path>>(
        reader: R,
        path: P,
        verifier: &PackageVerifier,
    ) -> Result<Package, Error> {
        let signature_path = to_signature_path(path.as_ref().to_path_buf());
        let reader = VerifyingReader::new(reader, verifier, signature_path);
        let reader = GzDecoder::new(reader);
        let mut reader = tar::Archive::new(reader);
        reader
            .find(|entry| {
                let path = entry.normalized_path()?;
                if matches!(path.to_str(), Some(path) if path.starts_with("control.tar")) {
                    // TODO remove `BufReader` when deko supports it
                    let mut tar_archive = tar::Archive::new(AnyDecoder::new(BufReader::new(entry)));
                    for entry in tar_archive.entries()? {
                        let mut entry = entry?;
                        let path = entry.path()?.normalize();
                        if path == Path::new("control") {
                            let mut buf = String::with_capacity(4096);
                            entry.read_to_string(&mut buf)?;
                            return buf
                                .parse::<Package>()
                                .map(Some)
                                .map_err(std::io::Error::other);
                        }
                    }
                }
                Ok(None)
            })?
            .ok_or_else(|| Error::MissingFile("missing control.tar*".into()))
    }

    pub fn file_name(&self) -> String {
        format!("{}_{}_{}.ipk", self.name, self.version, self.arch)
    }
}

impl Display for Package {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        writeln!(f, "Package: {}", self.name)?;
        writeln!(f, "Version: {}", self.version)?;
        writeln!(f, "License: {}", self.license)?;
        writeln!(f, "Architecture: {}", self.arch)?;
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
            arch: fields.remove_any("architecture")?.as_str().parse()?,
            description: fields.remove_any("description")?.try_into()?,
            maintainer: fields.remove_any("maintainer")?.try_into()?,
            installed_size: fields.remove_some("installed-size")?,
            provides: fields.remove_some("provides")?.unwrap_or_default(),
            depends: fields.remove_some("depends")?.unwrap_or_default(),
            other: fields,
        };
        Ok(control)
    }
}

impl TryFrom<wolf::Metadata> for Package {
    type Error = Error;
    fn try_from(other: wolf::Metadata) -> Result<Self, Self::Error> {
        Ok(Self {
            name: other.name.parse_field("name")?,
            version: other.version.parse_field("version")?,
            arch: Arch::All,
            description: other.description.into(),
            license: other.license.parse_field("license")?,
            depends: Default::default(),
            provides: Default::default(),
            maintainer: "Wolfpack <wolfpack@wolfpack.com>".parse()?,
            other: Default::default(),
            installed_size: Default::default(),
        })
    }
}

fn gz_writer() -> GzEncoder<Vec<u8>> {
    GzEncoder::new(Vec::new(), Compression::best())
}

fn to_signature_path(mut path: PathBuf) -> PathBuf {
    match path.file_name() {
        Some(file_name) => {
            let mut file_name = file_name.to_os_string();
            file_name.push(".sig");
            path.set_file_name(file_name);
        }
        None => path.set_file_name("sig"),
    };
    path
}

#[cfg(test)]
mod tests {

    use std::process::Command;

    use arbtest::arbtest;
    use command_error::CommandExt;
    use tempfile::TempDir;

    use super::*;
    use crate::ipk::SigningKey;
    use crate::test::prevent_concurrency;
    use crate::test::DirectoryOfFiles;

    #[test]
    fn write_read() {
        let workdir = TempDir::new().unwrap();
        let signing_key = SigningKey::generate(Some("wolfpack".into()));
        let verifying_key = signing_key.to_verifying_key();
        arbtest(|u| {
            let control: Package = u.arbitrary()?;
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let file_path = workdir.path().join("test.ipk");
            Package::write(
                &control,
                file_path.as_path(),
                directory.path(),
                &signing_key,
            )
            .unwrap();
            let actual = Package::read_control(
                File::open(file_path.as_path()).unwrap(),
                file_path.as_path(),
                &verifying_key,
            )
            .unwrap();
            assert_eq!(control, actual);
            Ok(())
        });
    }

    #[ignore = "Needs `opkg`"]
    #[test]
    fn opkg_installs_random_packages() {
        let _guard = prevent_concurrency("opkg");
        let workdir = TempDir::new().unwrap();
        let signing_key = SigningKey::generate(Some("wolfpack".into()));
        let _verifying_key = signing_key.to_verifying_key();
        arbtest(|u| {
            let mut package: Package = u.arbitrary()?;
            package.arch = "all".parse().unwrap();
            package.installed_size = Some(100);
            package.depends.clear();
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let package_path = workdir.path().join("test.ipk");
            package
                .write(package_path.as_path(), directory.path(), &signing_key)
                .unwrap();
            assert!(
                Command::new("opkg")
                    .arg("install")
                    .arg(package_path.as_path())
                    .status_checked()
                    .unwrap()
                    .success(),
                "package:\n========{}========",
                package
            );
            assert!(
                Command::new("opkg")
                    .arg("remove")
                    .arg(package.name.to_string())
                    .status_checked()
                    .unwrap()
                    .success(),
                "package:\n========{}========",
                package
            );
            Ok(())
        });
    }
}
