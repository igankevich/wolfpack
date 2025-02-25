use fs_err::File;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io::BufReader;
use std::io::Read;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::Path;
use std::path::PathBuf;

use deko::bufread::AnyDecoder;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use normalize_path::NormalizePath;

use crate::archive::ArchiveRead;
use crate::archive::ArchiveWrite;
use crate::deb;
use crate::deb::DEBIAN_BINARY_CONTENTS;
use crate::deb::DEBIAN_BINARY_FILE_NAME;
use crate::ipk::Error;
use crate::ipk::PackageSigner;
use crate::ipk::PackageVerifier;
use crate::sign::SignatureWriter;
use crate::sign::VerifyingReader;
use crate::wolf;

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, arbitrary::Arbitrary))]
pub struct Package(deb::Package);

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
            tar::Builder::from_files([("control", self.0.to_string())], gz_writer())?.finish()?;
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
                                .parse::<deb::Package>()
                                .map(Into::into)
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
        format!("{}_{}_{}.ipk", self.name, self.version, self.architecture)
    }
}

// TODO remove derefs
impl Deref for Package {
    type Target = deb::Package;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Package {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for Package {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl From<deb::Package> for Package {
    fn from(other: deb::Package) -> Self {
        Self(other)
    }
}

impl TryFrom<wolf::Metadata> for Package {
    type Error = Error;
    fn try_from(other: wolf::Metadata) -> Result<Self, Self::Error> {
        Ok(Self(other.try_into()?))
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
            package.architecture = "all".parse().unwrap();
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
                    .arg(package.name().to_string())
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
