use std::fs::create_dir_all;
use std::fs::File;
use std::path::Path;

use crate::deb::Error;
use crate::deb::PackageVerifier;
use crate::deb::Packages;
use crate::deb::Release;
use crate::deb::SimpleValue;
use crate::sign::PgpCleartextSigner;

pub struct Repository;

impl Repository {
    pub fn write<I, P, P2>(
        output_dir: P2,
        suite: SimpleValue,
        paths: I,
        verifier: &PackageVerifier,
        signer: &PgpCleartextSigner,
    ) -> Result<(), Error>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let repo_dir = output_dir.as_ref().to_path_buf();
        let dists_dir = output_dir.as_ref();
        let output_dir = dists_dir.join(suite.to_string());
        create_dir_all(output_dir.as_path())?;
        let packages = Packages::new(repo_dir.as_path(), paths, verifier)?;
        let packages_string = packages.to_string();
        std::fs::write(output_dir.join("Packages"), packages_string.as_bytes())?;
        let release = Release::new(suite, &packages, packages_string.as_str())?;
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
}

#[cfg(test)]
mod tests {
    use std::fs::remove_dir_all;
    use std::process::Command;

    use arbtest::arbtest;
    use pgp::types::PublicKeyTrait;
    use pgp::KeyType;
    use tempfile::TempDir;

    use super::*;
    use crate::deb::SimpleValue;
    use crate::deb::*;
    use crate::sign::PgpVerifier;
    use crate::test::pgp_keys;
    use crate::test::DirectoryOfFiles;
    use crate::test::UpperHex;

    #[ignore]
    #[test]
    fn apt_adds_random_repositories() {
        let (signing_key, verifying_key) = pgp_keys(KeyType::EdDSALegacy);
        //let (signing_key, verifying_key) = pgp_keys(KeyType::Rsa(2048));
        //let (signing_key, verifying_key) = pgp_keys(KeyType::Ed25519);
        let signer = PackageSigner::new(signing_key.clone());
        let verifier = PgpVerifier::new(verifying_key.clone());
        let release_signer = PgpCleartextSigner::new(signing_key.clone());
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
            let mut control: ControlData = u.arbitrary()?;
            control.architecture = "amd64".parse().unwrap();
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let deb_path = workdir.path().join("test.deb");
            let _ = remove_dir_all(root.as_path());
            let package_name = control.name();
            Package::write(
                &control,
                directory.path(),
                File::create(deb_path.as_path()).unwrap(),
                &signer,
            )
            .unwrap();
            let suite: SimpleValue = "meta".parse().unwrap();
            Repository::write(
                root.as_path(),
                suite.clone(),
                [deb_path.as_path()],
                &verifier,
                &release_signer,
            )
            .unwrap();
            let fingerprint = verifying_key.fingerprint();
            std::fs::write(
                "/etc/apt/sources.list.d/test.list",
                format!(
                    "deb [signed-by={1}] file://{0} meta/\n",
                    root.display(),
                    UpperHex(&fingerprint.as_bytes()),
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
