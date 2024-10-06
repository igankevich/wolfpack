use std::fs::create_dir_all;
use std::fs::File;
use std::path::Path;

use crate::deb::Error;
use crate::deb::Packages;
use crate::deb::Release;
use crate::deb::SimpleValue;
use crate::sign::PgpCleartextSigner;
use crate::sign::Verifier;

pub struct Repository;

impl Repository {
    pub fn write<I, P, P2, V>(
        output_dir: P2,
        suite: SimpleValue,
        paths: I,
        verifier: &V,
        signer: &PgpCleartextSigner,
    ) -> Result<(), Error>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
        P2: AsRef<Path>,
        V: Verifier,
    {
        let output_dir = output_dir.as_ref().join("dists").join(suite.to_string());
        create_dir_all(output_dir.as_path())?;
        let packages = Packages::new(output_dir.as_path(), paths, verifier)?;
        let packages_string = packages.to_string();
        std::fs::write(output_dir.join("Packages"), packages_string.as_bytes())?;
        for (arch, per_arch_packages) in packages.iter() {
            let mut path = output_dir.clone();
            path.push("main");
            path.push(format!("binary-{}", arch));
            create_dir_all(path.as_path())?;
            path.push("Packages");
            std::fs::write(path, per_arch_packages.to_string())?;
        }
        let release = Release::new(suite, &packages, packages_string.as_str())?;
        let release_string = release.to_string();
        std::fs::write(output_dir.join("Release"), release_string.as_bytes())?;
        let signed_release = signer
            .sign(release_string.as_str())
            .map_err(|_| Error::other("failed to sign the release"))?;
        //std::fs::write(output_dir.join("Release"), signed_release.signed_text().as_bytes())?;
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
    use pgp::crypto::hash::HashAlgorithm;
    use pgp::packet::SignatureType;
    use pgp::types::PublicKeyTrait;
    use pgp::KeyType;
    use tempfile::TempDir;

    use super::*;
    use crate::deb::SimpleValue;
    use crate::deb::*;
    use crate::sign::PgpSigner;
    use crate::sign::PgpVerifier;
    use crate::test::pgp_keys;
    use crate::test::DirectoryOfFiles;
    use crate::test::UpperHex;

    #[ignore]
    #[test]
    fn apt_adds_random_repositories() {
        let (signing_key, verifying_key) = pgp_keys(KeyType::EdDSALegacy);
        let signer = PgpSigner::new(
            signing_key.clone(),
            SignatureType::Binary,
            HashAlgorithm::SHA2_256,
        );
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
            let suite: SimpleValue = "wolfpack-repo".parse().unwrap();
            Repository::write(
                root.as_path(),
                suite.clone(),
                [deb_path.as_path()],
                &verifier,
                &release_signer,
            )
            .unwrap();
            let fingerprint = verifying_key.key_id();
            std::fs::write(
                "/etc/apt/sources.list.d/test.sources",
                format!(
                    r#"Types: deb
URIs: file://{0}
Suites: {1}
Components: main
Signed-With: {2}
"#,
                    root.display(),
                    suite,
                    UpperHex(&fingerprint.to_vec()),
                ),
            )
            .unwrap();
            assert!(Command::new("find")
                .arg(root.as_path())
                .status()
                .unwrap()
                .success());
            assert!(Command::new("cat")
                .arg(root.join("dists/wolfpack-repo/Release"))
                .status()
                .unwrap()
                .success());
            assert!(Command::new("cat")
                .arg(root.join("dists/wolfpack-repo/Packages"))
                .status()
                .unwrap()
                .success());
            assert!(//Command::new("apt-get")
                    Command::new("strace")
                    .arg("-f")
                    .arg("-e")
                    .arg("execve")
                    .arg("apt-get")
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
