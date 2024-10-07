use std::io::Read;
use std::io::Write;
use std::path::Path;

use crate::deb::BasicPackage;
use crate::deb::ControlData;
use crate::deb::Error;
use crate::deb::PackageSigner;
use crate::deb::PackageVerifier;
use crate::deb::SignatureKind;

pub struct Package;

impl Package {
    pub fn write<W: Write, P: AsRef<Path>>(
        control_data: &ControlData,
        directory: P,
        writer: W,
        signer: &PackageSigner,
    ) -> Result<(), std::io::Error> {
        BasicPackage::write::<W, ar::Builder<W>, PackageSigner, P>(
            control_data,
            directory,
            writer,
            signer,
            SignatureKind::Bundled { file_name: "_gpgorigin".into() },
        )
    }

    pub fn read_control<R: Read>(
        reader: R,
        verifier: &PackageVerifier,
    ) -> Result<ControlData, Error> {
        BasicPackage::read_control::<R, ar::Archive<R>, PackageVerifier>(reader, verifier)
    }
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
    use pgp::KeyType;
    use tempfile::TempDir;

    use super::*;
    use crate::sign::PgpVerifier;
    use crate::test::pgp_keys;
    use crate::test::DirectoryOfFiles;
    use crate::test::UpperHex;

    #[test]
    fn write_read() {
        let (signing_key, verifying_key) = pgp_keys(KeyType::EdDSALegacy);
        let signer = PackageSigner::new(signing_key);
        let verifier = PgpVerifier::new(verifying_key);
        arbtest(|u| {
            let control: ControlData = u.arbitrary()?;
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let mut buf: Vec<u8> = Vec::new();
            Package::write(&control, directory.path(), &mut buf, &signer).unwrap();
            let actual = Package::read_control(&buf[..], &verifier).unwrap();
            assert_eq!(control, actual);
            Ok(())
        });
    }

    #[ignore]
    #[test]
    fn dpkg_installs_random_packages() {
        let (signing_key, verifying_key) = pgp_keys(KeyType::EdDSALegacy);
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
            let mut control: ControlData = u.arbitrary()?;
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
            Package::write(
                &control,
                directory.path(),
                File::create(path.as_path()).unwrap(),
                &signer,
            )
            .unwrap();
            assert!(Command::new("gpg")
                .arg("--dearmor")
                .arg("--output")
                .arg(keyring_file.as_path())
                .arg(verifying_key_file.as_path())
                .status()
                .unwrap()
                .success());
            assert!(Command::new("debsig-verify")
                .arg("--debug")
                .arg("--root")
                .arg(root.as_path())
                .arg(path.as_path())
                .status()
                .unwrap()
                .success());
            eprint!("{}", control);
            assert!(
                Command::new("dpkg")
                    .arg("--root")
                    .arg(root.as_path())
                    .arg("--install")
                    .arg(path.as_path())
                    .status()
                    .unwrap()
                    .success(),
                "control = {:?}",
                control
            );
            assert!(Command::new("dpkg-query")
                .arg("--root")
                .arg(root.as_path())
                .arg("-L")
                .arg(control.name().as_str())
                .stdout(Stdio::null())
                .status()
                .unwrap()
                .success());
            Ok(())
        })
        .budget(Duration::from_secs(10));
    }
}
