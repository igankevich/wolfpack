use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fs::create_dir_all;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use walkdir::WalkDir;

use crate::deb::Error;
use crate::deb::Package;
use crate::deb::PackageVerifier;
use crate::deb::Release;
use crate::deb::SimpleValue;
use crate::hash::MultiHash;
use crate::hash::MultiHashReader;
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
            let control = Package::read_control(&mut reader, verifier)?;
            let (hash, size) = reader.digest()?;
            let mut filename = PathBuf::new();
            filename.push("data");
            filename.push(hash.sha2.to_string());
            create_dir_all(output_dir.as_ref().join(&filename))?;
            filename.push(path.file_name().unwrap());
            let new_path = output_dir.as_ref().join(&filename);
            std::fs::rename(path, new_path)?;
            let control = ExtendedControlData {
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
    packages: Vec<ExtendedControlData>,
}

impl Display for PerArchPackages {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for control in self.packages.iter() {
            writeln!(f, "{}", control)?;
        }
        Ok(())
    }
}

pub struct ExtendedControlData {
    pub control: Package,
    hash: MultiHash,
    filename: PathBuf,
    size: usize,
}

impl Display for ExtendedControlData {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.control)?;
        writeln!(f, "Filename: {}", self.filename.display())?;
        writeln!(f, "Size: {}", self.size)?;
        writeln!(f, "MD5sum: {:x}", self.hash.md5)?;
        writeln!(f, "SHA1: {}", self.hash.sha1)?;
        writeln!(f, "SHA256: {}", self.hash.sha2)?;
        Ok(())
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
        let verifying_key_file = workdir.path().join("/etc/apt/trusted.gpg.d/test.asc");
        verifying_key
            .to_armored_writer(
                &mut File::create(verifying_key_file.as_path()).unwrap(),
                Default::default(),
            )
            .unwrap();
        arbtest(|u| {
            let mut control: Package = u.arbitrary()?;
            control.architecture = "amd64".parse().unwrap();
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let deb_path = workdir.path().join("test.deb");
            let _ = remove_dir_all(root.as_path());
            let package_name = control.name();
            control
                .write(
                    directory.path(),
                    File::create(deb_path.as_path()).unwrap(),
                    &signer,
                )
                .unwrap();
            let suite: SimpleValue = "meta".parse().unwrap();
            Repository::new(
                root.as_path(),
                [deb_path.as_path()],
                &verifier,
            ).unwrap().write(
                root.as_path(),
                suite.clone(),
                &release_signer,
            )
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
                "control = {:?}",
                control
            );
            Ok(())
        });
    }
}
