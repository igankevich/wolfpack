use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fs::create_dir_all;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use walkdir::WalkDir;

use crate::deb::DependencyChoice;
use crate::deb::Error;
use crate::deb::Package;
use crate::deb::PackageVerifier;
use crate::deb::Release;
use crate::deb::SimpleValue;
use crate::hash::AnyHash;
use crate::hash::Md5Hash;
use crate::hash::MultiHashReader;
use crate::hash::Sha1Hash;
use crate::hash::Sha256Hash;
use crate::sign::PgpCleartextSigner;

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
            let mut reader = MultiHashReader::new(File::open(path)?);
            let (package, _data) = Package::read(reader.by_ref(), verifier)?;
            let (hash, size) = reader.digest()?;
            let mut filename = PathBuf::new();
            filename.push("data");
            filename.push(hash.sha2.to_string());
            create_dir_all(output_dir.as_ref().join(&filename))?;
            filename.push(path.file_name().unwrap());
            let new_path = output_dir.as_ref().join(&filename);
            std::fs::rename(path, new_path)?;
            let package = ExtendedPackage {
                inner: package,
                size,
                md5: Some(hash.md5.into()),
                sha1: Some(hash.sha1),
                sha256: Some(hash.sha2),
                filename,
            };
            packages
                .entry(package.inner.architecture.clone())
                .or_insert_with(|| PerArchPackages {
                    packages: Vec::new(),
                })
                .packages
                .push(package);
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

    pub fn write<P>(
        &self,
        output_dir: P,
        suite: SimpleValue,
        signer: &PgpCleartextSigner,
    ) -> Result<(), Error>
    where
        P: AsRef<Path>,
    {
        let dists_dir = output_dir.as_ref();
        let output_dir = dists_dir.join(suite.to_string());
        create_dir_all(output_dir.as_path())?;
        let packages_string = self.to_string();
        std::fs::write(output_dir.join("Packages"), packages_string.as_bytes())?;
        let release = Release::new(suite, self, packages_string.as_str())?;
        let release_string = release.to_string();
        std::fs::write(output_dir.join("Release"), release_string.as_bytes())?;
        let signed_release = signer
            .sign(release_string.as_str())
            .map_err(|_| Error::other("failed to sign the release"))?;
        // TODO cleartext signature does not work
        //signed_release
        //    .to_armored_writer(
        //        &mut File::create(output_dir.join("InRelease"))?,
        //        Default::default(),
        //    )
        //    .map_err(|e| Error::other(e.to_string()))?;
        signed_release.signatures()[0]
            .to_armored_writer(
                &mut File::create(output_dir.join("Release.gpg"))?,
                Default::default(),
            )
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

impl PerArchPackages {
    pub fn find(&self, keyword: &str) -> Vec<ExtendedPackage> {
        let mut matches = Vec::new();
        for package in self.packages.iter() {
            if package.inner.find(keyword) {
                matches.push(package.clone());
            }
        }
        matches
    }

    pub fn find_by_name(&self, name: &str) -> Vec<ExtendedPackage> {
        let mut matches = Vec::new();
        for package in self.packages.iter() {
            if package.inner.name.as_str() == name {
                matches.push(package.clone());
            }
        }
        matches
    }

    pub fn find_dependency(&self, dependency: &DependencyChoice) -> Vec<ExtendedPackage> {
        let mut matches = Vec::new();
        for package in self.packages.iter() {
            if dependency.matches(&package.inner) {
                matches.push(package.clone());
            }
        }
        matches
    }
}

impl Display for PerArchPackages {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for package in self.packages.iter() {
            writeln!(f, "{}", package)?;
        }
        Ok(())
    }
}

impl FromStr for PerArchPackages {
    type Err = Error;
    fn from_str(string: &str) -> Result<Self, Self::Err> {
        let mut packages = Vec::new();
        for chunk in string.split("\n\n") {
            // Normalize the chunk.
            let chunk = chunk.trim();
            if chunk.is_empty() {
                continue;
            }
            packages.push(chunk.parse()?);
        }
        Ok(Self { packages })
    }
}

#[derive(Clone)]
pub struct ExtendedPackage {
    pub inner: Package,
    pub md5: Option<Md5Hash>,
    pub sha1: Option<Sha1Hash>,
    pub sha256: Option<Sha256Hash>,
    pub filename: PathBuf,
    pub size: u64,
}

impl ExtendedPackage {
    pub fn hash(&self) -> Option<AnyHash> {
        if let Some(hash) = self.sha256.as_ref() {
            return Some(hash.clone().into());
        }
        if let Some(hash) = self.sha1.as_ref() {
            return Some(hash.clone().into());
        }
        if let Some(hash) = self.md5.as_ref() {
            return Some(hash.clone().into());
        }
        None
    }
}

impl Display for ExtendedPackage {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.inner)?;
        writeln!(f, "Filename: {}", self.filename.display())?;
        writeln!(f, "Size: {}", self.size)?;
        if let Some(md5) = self.md5.as_ref() {
            writeln!(f, "MD5sum: {}", md5)?;
        }
        if let Some(sha1) = self.sha1.as_ref() {
            writeln!(f, "SHA1: {}", sha1)?;
        }
        if let Some(sha256) = self.sha256.as_ref() {
            writeln!(f, "SHA256: {}", sha256)?;
        }
        Ok(())
    }
}

impl FromStr for ExtendedPackage {
    type Err = Error;
    fn from_str(string: &str) -> Result<Self, Self::Err> {
        let mut inner: Package = string.parse()?;
        let extended = Self {
            md5: inner.other.remove_some("md5sum")?,
            sha1: inner.other.remove_some("sha1")?,
            sha256: inner.other.remove_some("sha256")?,
            filename: inner.other.remove_any("filename")?.try_into()?,
            size: inner.other.remove("size")?,
            inner,
        };
        Ok(extended)
    }
}

#[cfg(test)]
mod tests {
    use std::fs::remove_dir_all;
    use std::process::Command;

    use arbtest::arbtest;
    use pgp::types::PublicKeyTrait;
    use tempfile::TempDir;

    use super::*;
    use crate::deb::SimpleValue;
    use crate::deb::*;
    use crate::test::DirectoryOfFiles;
    use crate::test::UpperHex;

    #[ignore]
    #[test]
    fn apt_adds_random_repositories() {
        let (signing_key, verifying_key) = SigningKey::generate("wolfpack-pgp-id".into()).unwrap();
        let signer = PackageSigner::new(signing_key.clone());
        let verifier = PackageVerifier::new(verifying_key.clone());
        let release_signer = PgpCleartextSigner::new(signing_key.clone().into());
        let workdir = TempDir::new().unwrap();
        let root = workdir.path().join("root");
        let verifying_key_file = workdir.path().join("etc/apt/trusted.gpg.d/test.asc");
        verifying_key
            .to_armored_writer(
                &mut File::create(verifying_key_file.as_path()).unwrap(),
                Default::default(),
            )
            .unwrap();
        arbtest(|u| {
            let mut package: Package = u.arbitrary()?;
            package.architecture = "amd64".parse().unwrap();
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let deb_path = workdir.path().join("test.deb");
            let _ = remove_dir_all(root.as_path());
            let package_name = package.name();
            package
                .write(
                    directory.path(),
                    File::create(deb_path.as_path()).unwrap(),
                    &signer,
                )
                .unwrap();
            let suite: SimpleValue = "meta".parse().unwrap();
            Repository::new(root.as_path(), [deb_path.as_path()], &verifier)
                .unwrap()
                .write(root.as_path(), suite.clone(), &release_signer)
                .unwrap();
            let fingerprint = verifying_key.fingerprint();
            std::fs::write(
                "/etc/apt/sources.list.d/test.list",
                format!(
                    "deb [signed-by={1}] file://{0} meta/\n",
                    root.display(),
                    UpperHex(fingerprint.as_bytes()),
                ),
            )
            .unwrap();
            assert!(Command::new("find")
                .arg(root.as_path())
                .status()
                .unwrap()
                .success());
            assert!(Command::new("cat")
                .arg(root.join("meta/Release"))
                .status()
                .unwrap()
                .success());
            assert!(Command::new("cat")
                .arg(root.join("meta/Packages"))
                .status()
                .unwrap()
                .success());
            assert!(Command::new("apt-get")
                //Command::new("strace")
                //.arg("-f")
                //.arg("-e")
                //.arg("execve")
                //.arg("apt-get")
                .arg("update")
                .status()
                .unwrap()
                .success());
            assert!(
                Command::new("apt-get")
                    //Command::new("strace")
                    //.arg("-f")
                    //.arg("apt-get")
                    .arg("install")
                    .arg(package_name.to_string())
                    .status()
                    .unwrap()
                    .success(),
                "package = {:?}",
                package
            );
            Ok(())
        });
    }
}
