use fs_err::create_dir_all;
use fs_err::File;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use ksign::IO;
use walkdir::WalkDir;

use crate::hash::Sha256Hash;
use crate::hash::Sha256Reader;
use crate::ipk::Arch;
use crate::ipk::Error;
use crate::ipk::Package;
use crate::ipk::PackageSigner;
use crate::ipk::PackageVerifier;

pub struct Repository {
    packages: HashMap<Arch, PerArchPackages>,
}

impl Repository {
    pub fn new<I, P, P2>(
        output_dir: P2,
        paths: I,
        verifier: &PackageVerifier,
    ) -> Result<Self, Error>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let mut packages: HashMap<Arch, PerArchPackages> = HashMap::new();
        let mut push_package = |path: &Path| -> Result<(), Error> {
            let mut reader = Sha256Reader::new(File::open(path)?);
            let control = Package::read_control(reader.by_ref(), path, verifier)?;
            let (hash, size) = reader.digest()?;
            let mut filename = PathBuf::new();
            filename.push("data");
            filename.push(hash.to_string());
            create_dir_all(output_dir.as_ref().join(&filename))?;
            filename.push(path.file_name().ok_or(ErrorKind::InvalidData)?);
            let new_path = output_dir.as_ref().join(&filename);
            fs_err::rename(path, new_path)?;
            let control = ExtendedPackage {
                control,
                size,
                hash,
                filename,
            };
            packages
                .entry(control.control.architecture)
                .or_insert_with(|| PerArchPackages {
                    packages: Vec::new(),
                })
                .packages
                .push(control);
            Ok(())
        };
        for path in paths.into_iter() {
            let path = path.as_ref();
            if path.is_dir() {
                for entry in WalkDir::new(path).into_iter() {
                    let entry = entry?;
                    if entry.file_type().is_dir()
                        || entry.path().extension() != Some(OsStr::new("ipk"))
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

    pub fn write<P: AsRef<Path>>(
        &self,
        output_dir: P,
        signer: &PackageSigner,
    ) -> Result<(), Error> {
        let output_dir = output_dir.as_ref();
        create_dir_all(output_dir)?;
        let packages_string = self.to_string();
        for (format, extension) in [(deko::Format::Verbatim, ""), (deko::Format::Gz, ".gz")] {
            let filename = format!("Packages{}", extension);
            let mut writer = deko::AnyEncoder::new(
                File::create(output_dir.join(filename))?,
                format,
                deko::write::Compression::Best,
            )?;
            writer.write_all(packages_string.as_bytes())?;
            writer.finish()?;
        }
        let signature = signer.sign(packages_string.as_bytes());
        signature
            .write_to_file(output_dir.join("Packages.sig"))
            .map_err(|e| Error::other(e.to_string()))?;
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Arch, &PerArchPackages)> {
        self.packages.iter()
    }

    pub fn architectures(&self) -> HashSet<Arch> {
        self.packages.keys().cloned().collect()
    }
}

impl Display for Repository {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for (_, per_arch_packages) in self.packages.iter() {
            Display::fmt(per_arch_packages, f)?;
        }
        Ok(())
    }
}

pub struct PerArchPackages {
    packages: Vec<ExtendedPackage>,
}

impl Display for PerArchPackages {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for control in self.packages.iter() {
            writeln!(f, "{}", control)?;
        }
        Ok(())
    }
}

pub struct ExtendedPackage {
    pub control: Package,
    hash: Sha256Hash,
    filename: PathBuf,
    size: u64,
}

impl Display for ExtendedPackage {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.control)?;
        writeln!(f, "Filename: {}", self.filename.display())?;
        writeln!(f, "Size: {}", self.size)?;
        writeln!(f, "SHA256sum: {}", self.hash)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use fs_err::remove_dir_all;
    use std::process::Command;

    use arbtest::arbtest;
    use command_error::CommandExt;
    use tempfile::TempDir;

    use super::*;
    use crate::ipk::SigningKey;
    use crate::test::prevent_concurrency;
    use crate::test::DirectoryOfFiles;

    #[ignore = "Needs `opkg`"]
    #[test]
    fn opkg_installs_from_repo() {
        let _guard = prevent_concurrency("opkg");
        let workdir = TempDir::new().unwrap();
        let repo_dir = workdir.path().join("repo");
        let signing_key = SigningKey::generate(Some("wolfpack".into()));
        let verifying_key = signing_key.to_verifying_key();
        // speed up opkg update
        fs_err::remove_file("/etc/opkg/distfeeds.conf").unwrap();
        arbtest(|u| {
            let mut package: Package = u.arbitrary()?;
            package.architecture = "all".parse().unwrap();
            package.depends.clear();
            package.installed_size = Some(100);
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let package_path = workdir.path().join("test.ipk");
            package
                .write(package_path.as_path(), directory.path(), &signing_key)
                .unwrap();
            let _ = remove_dir_all(&repo_dir);
            Repository::new(&repo_dir, [&package_path], &verifying_key)
                .unwrap()
                .write(&repo_dir, &signing_key)
                .unwrap();
            Command::new("find")
                .arg(workdir.path())
                .status_checked()
                .unwrap();
            fs_err::write(
                "/etc/opkg/test.conf",
                format!("src/gz test file://{}\n", repo_dir.display()),
            )
            .unwrap();
            verifying_key
                .write_to_file(format!("/etc/opkg/keys/{}", verifying_key.fingerprint()))
                .unwrap();
            Command::new("cat")
                .arg("/etc/opkg/test.conf")
                .status_checked()
                .unwrap();
            Command::new("sh")
                .arg("-c")
                .arg("cat /etc/opkg/keys/*")
                .status_checked()
                .unwrap();
            assert!(
                Command::new("opkg")
                    .arg("update")
                    .arg(package_path.as_path())
                    .status_checked()
                    .unwrap()
                    .success(),
                "package:\n========{}========",
                package
            );
            assert!(
                Command::new("opkg")
                    .arg("install")
                    .arg(package.name.to_string())
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
