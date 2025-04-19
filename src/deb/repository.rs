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
use std::str::FromStr;

use pgp::types::PublicKeyTrait;
use tempfile::TempDir;
use walkdir::WalkDir;

use crate::deb::Arch;
use crate::deb::DependencyChoice;
use crate::deb::Error;
use crate::deb::MultilineValue;
use crate::deb::Package;
use crate::deb::PackageSigner;
use crate::deb::PackageVerifier;
use crate::deb::Release;
use crate::deb::SimpleValue;
use crate::deb::VerifyingKey;
use crate::deb::Version;
use crate::hash::AnyHash;
use crate::hash::Md5Hash;
use crate::hash::MultiHashReader;
use crate::hash::Sha1Hash;
use crate::hash::Sha256Hash;
use crate::hash::UpperHex;
use crate::sign::PgpCleartextSigner;

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
            let mut reader = MultiHashReader::new(File::open(path)?);
            let (package, _data) = Package::read(reader.by_ref(), verifier)?;
            let (hash, size) = reader.digest()?;
            let mut filename = PathBuf::new();
            filename.push("data");
            filename.push(hash.sha2.to_string());
            create_dir_all(output_dir.as_ref().join(&filename))?;
            filename.push(path.file_name().ok_or(ErrorKind::InvalidData)?);
            let new_path = output_dir.as_ref().join(&filename);
            fs_err::rename(path, new_path)?;
            let package = ExtendedPackage {
                inner: package,
                size,
                md5: Some(hash.md5.into()),
                sha1: Some(hash.sha1),
                sha256: Some(hash.sha2),
                filename,
            };
            packages
                .entry(package.inner.architecture)
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
        for (format, extension) in [
            (deko::Format::Verbatim, ""),
            (deko::Format::Gz, ".gz"),
            (deko::Format::Xz, ".xz"),
        ] {
            let filename = format!("Packages{}", extension);
            let mut writer = deko::AnyEncoder::new(
                File::create(output_dir.join(filename))?,
                format,
                deko::write::Compression::Best,
            )?;
            writer.write_all(packages_string.as_bytes())?;
            writer.finish()?;
        }
        let release = Release::new(suite, self, packages_string.as_str())?;
        let release_string = release.to_string();
        fs_err::write(output_dir.join("Release"), release_string.as_bytes())?;
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

    pub fn release_package(
        suite: &SimpleValue,
        version: Version,
        description: MultilineValue,
        verifying_key: &VerifyingKey,
        url: String,
        signer: &PackageSigner,
        output_dir: &Path,
    ) -> Result<PathBuf, Error> {
        let workdir = TempDir::new()?;
        let rootfs_dir = workdir.path();
        let fingerprint = verifying_key.fingerprint();
        // Write verifying key.
        let verifying_key_file = rootfs_dir.join(format!(
            "etc/apt/trusted.gpg.d/{}-{}.asc",
            suite,
            UpperHex(fingerprint.as_bytes())
        ));
        create_dir_all(verifying_key_file.parent().expect("Parent dir exists"))?;
        verifying_key
            .to_armored_writer(&mut File::create(&verifying_key_file)?, Default::default())
            .map_err(std::io::Error::other)?;
        // Write repository configuration.
        let sources_list_file = rootfs_dir.join(format!("etc/apt/sources.list.d/{}.list", suite));
        create_dir_all(sources_list_file.parent().expect("Parent dir exists"))?;
        fs_err::write(
            &sources_list_file,
            format!(
                "deb [signed-by={}] {url} {suite}/\n",
                UpperHex(fingerprint.as_bytes()),
            ),
        )?;
        // Generate the package.
        let package = Package {
            name: format!("{}-repo", suite).parse()?,
            version,
            architecture: Arch::All,
            description,
            // TODO
            license: Default::default(),
            maintainer: Default::default(),
            installed_size: Default::default(),
            provides: Default::default(),
            pre_depends: Default::default(),
            depends: if url.starts_with("https://") {
                // TODO add other types
                "apt-transport-https".parse()?
            } else {
                Default::default()
            },
            homepage: Default::default(),
            other: Default::default(),
        };
        let repo_package_file = output_dir.join(package.file_name());
        package.write(File::create(&repo_package_file)?, rootfs_dir, signer)?;
        Ok(repo_package_file)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Arch, &PerArchPackages)> {
        self.packages.iter()
    }

    pub fn architectures(&self) -> HashSet<Arch> {
        self.packages.keys().copied().collect()
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

    pub fn into_inner(self) -> Vec<ExtendedPackage> {
        self.packages
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

// TODO read from file
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
    use fs_err::remove_dir_all;
    use fs_err::remove_file;
    use std::process::Command;

    use arbtest::arbtest;
    use command_error::CommandExt;

    use super::*;
    use crate::deb::SimpleValue;
    use crate::deb::*;
    use crate::test::DirectoryOfFiles;

    #[ignore = "Needs `apt`"]
    #[test]
    fn apt_adds_random_repositories() {
        let (signing_key, verifying_key) = SigningKey::generate("wolfpack-pgp-id".into()).unwrap();
        let signer = PackageSigner::new(signing_key.clone());
        let verifier = PackageVerifier::new(verifying_key.clone());
        let release_signer = PgpCleartextSigner::new(signing_key.clone().into());
        let workdir = TempDir::new().unwrap();
        let root = workdir.path().join("root");
        remove_file("/etc/apt/sources.list.d/debian.sources").unwrap();
        arbtest(|u| {
            let mut package: Package = u.arbitrary()?;
            package.architecture = "all".parse().unwrap();
            package.depends.clear();
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let deb_path = workdir.path().join("test.deb");
            let _ = remove_dir_all(&root);
            create_dir_all(&root).unwrap();
            let package_name = package.name();
            package
                .write(
                    File::create(deb_path.as_path()).unwrap(),
                    directory.path(),
                    &signer,
                )
                .unwrap();
            let suite: SimpleValue = "meta".parse().unwrap();
            let repo_package_file = Repository::release_package(
                &suite,
                "1.0".parse().unwrap(),
                "My repo".into(),
                &verifying_key,
                format!("file://{}", root.display()),
                &signer,
                &root,
            )
            .unwrap();
            let repo = Repository::new(root.as_path(), [deb_path.as_path()], &verifier).unwrap();
            repo.write(root.as_path(), suite.clone(), &release_signer)
                .unwrap();
            assert!(Command::new("dpkg")
                .arg("--install")
                .arg(&repo_package_file)
                .status_checked()
                .unwrap()
                .success());
            assert!(Command::new("find")
                .arg(root.as_path())
                .status_checked()
                .unwrap()
                .success());
            assert!(apt_get().arg("update").status_checked().unwrap().success());
            assert!(
                apt_get()
                    .arg("install")
                    .arg(package_name.to_string())
                    .status_checked()
                    .unwrap()
                    .success(),
                "package = {:?}",
                package
            );
            assert!(
                apt_get()
                    .arg("remove")
                    .arg(package_name.to_string())
                    .status_checked()
                    .unwrap()
                    .success(),
                "package = {:?}",
                package
            );
            Ok(())
        });
    }

    fn apt_get() -> Command {
        let mut c = Command::new("apt-get");
        c.args(["-o", "APT::Get::Assume-Yes=true"]);
        c.args(["-o", "Debug::pkgDPkgPM=true"]);
        c
    }
}
