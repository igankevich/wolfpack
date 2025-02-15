use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fs::create_dir_all;
use std::fs::File;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use flate2::write::GzEncoder;
use flate2::Compression;
use ksign::IO;
use walkdir::WalkDir;

use crate::hash::Sha256Hash;
use crate::hash::Sha256Reader;
use crate::ipk::Error;
use crate::ipk::Package;
use crate::ipk::PackageSigner;
use crate::ipk::PackageVerifier;
use crate::ipk::SimpleValue;

pub struct Repository {
    packages: HashMap<SimpleValue, PerArchPackages>,
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
        let mut packages: HashMap<SimpleValue, PerArchPackages> = HashMap::new();
        let mut push_package = |path: &Path| -> Result<(), Error> {
            eprintln!("reading {}", path.display());
            let mut reader = Sha256Reader::new(File::open(path)?);
            let control = Package::read_control(reader.by_ref(), path, verifier)?;
            let (hash, size) = reader.digest()?;
            let mut filename = PathBuf::new();
            filename.push(hash.to_string());
            create_dir_all(output_dir.as_ref().join(&filename))?;
            filename.push(path.file_name().ok_or(ErrorKind::InvalidData)?);
            let new_path = output_dir.as_ref().join(&filename);
            std::fs::rename(path, new_path)?;
            let control = ExtendedPackage {
                control,
                size,
                hash,
                filename,
            };
            packages
                .entry(control.control.architecture.clone())
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
                        || entry.path().extension() != Some(OsStr::new("deb"))
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
        std::fs::write(output_dir.join("Packages"), packages_string.as_bytes())?;
        {
            let mut writer = GzEncoder::new(
                File::create(output_dir.join("Packages.gz"))?,
                Compression::best(),
            );
            writer.write_all(packages_string.as_bytes())?;
            writer.finish()?;
        }
        let signature = signer.sign(packages_string.as_bytes());
        signature
            .write_to_file(output_dir.join("Packages.sig"))
            .map_err(|e| Error::other(e.to_string()))?;
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SimpleValue, &PerArchPackages)> {
        self.packages.iter()
    }

    pub fn architectures(&self) -> HashSet<SimpleValue> {
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

    use std::fs::remove_dir_all;
    use std::process::Command;
    use std::time::Duration;

    use arbtest::arbtest;
    use tempfile::TempDir;

    use super::*;
    use crate::ipk::SigningKey;
    use crate::test::DirectoryOfFiles;

    #[ignore = "Needs `opkg`"]
    #[test]
    fn opkg_installs_from_repo() {
        let workdir = TempDir::new().unwrap();
        let repo_dir = workdir.path().join("repo");
        let signing_key = SigningKey::generate(Some("wolfpack".into()));
        let verifying_key = signing_key.to_verifying_key();
        // speed up opkg update
        std::fs::remove_file("/etc/opkg/distfeeds.conf").unwrap();
        arbtest(|u| {
            let mut package: Package = u.arbitrary()?;
            package.architecture = "all".parse().unwrap();
            package.depends.clear();
            package.installed_size = Some(100);
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let package_path = workdir.path().join("test.ipk");
            package
                .write(directory.path(), package_path.as_path(), &signing_key)
                .unwrap();
            let _ = remove_dir_all(&repo_dir);
            Repository::new(&repo_dir, [&package_path], &verifying_key)
                .unwrap()
                .write(&repo_dir, &signing_key)
                .unwrap();
            Command::new("find").arg(workdir.path()).status().unwrap();
            std::fs::write(
                "/etc/opkg/customfeeds.conf",
                format!("src/gz test file://{}\n", repo_dir.display()),
            )
            .unwrap();
            verifying_key
                .write_to_file(format!("/etc/opkg/keys/{}", verifying_key.fingerprint()))
                .unwrap();
            Command::new("cat")
                .arg("/etc/opkg/customfeeds.conf")
                .status()
                .unwrap();
            Command::new("sh")
                .arg("-c")
                .arg("cat /etc/opkg/keys/*")
                .status()
                .unwrap();
            assert!(
                Command::new("opkg")
                    .arg("update")
                    .arg(package_path.as_path())
                    .status()
                    .unwrap()
                    .success(),
                "package:\n========{}========",
                package
            );
            assert!(
                Command::new("opkg")
                    .arg("install")
                    .arg(package.name().to_string())
                    .status()
                    .unwrap()
                    .success(),
                "package:\n========{}========",
                package
            );
            assert!(
                Command::new("opkg")
                    .arg("remove")
                    .arg(package.name().to_string())
                    .status()
                    .unwrap()
                    .success(),
                "package:\n========{}========",
                package
            );
            Ok(())
        })
        .budget(Duration::from_secs(5));
    }
}
