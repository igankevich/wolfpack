use std::fs::File;
use std::io::Error;
use std::io::Write;
use std::path::Path;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use stuckliste::receipt::ReceiptBuilder;
use tempfile::TempDir;
pub use zar::rsa::RsaPrivateKey as SigningKey;
pub use zar::rsa::RsaPublicKey as VerifyingKey;
pub use zar::ChecksumAlgo;
pub use zar::RsaSigner as PackageSigner;

use crate::macos::xml;

#[cfg_attr(test, derive(PartialEq, Eq, Clone, Debug))]
pub struct Package {
    pub identifier: String,
    pub version: String,
}

impl Package {
    #[allow(unused)]
    pub fn write<W: Write, P: AsRef<Path>>(
        &self,
        writer: W,
        directory: P,
        signer: &PackageSigner,
    ) -> Result<(), Error> {
        let info = xml::PackageInfo {
            format_version: 2,
            install_location: Some("/".into()),
            identifier: self.identifier.clone(),
            version: self.version.clone(),
            generator_version: Some(GENERATOR_VERSION.into()),
            auth: xml::Auth::Root,
            payload: xml::Payload {
                number_of_files: 0,
                install_kb: 0,
            },
            relocatable: Default::default(),
            bundles: Default::default(),
            bundle_version: Default::default(),
            upgrade_bundle: Default::default(),
            update_bundle: Default::default(),
            atomic_update_bundle: Default::default(),
            strict_identifier: Default::default(),
            relocate: Default::default(),
            scripts: Default::default(),
        };
        let workdir = TempDir::new()?;
        let package_info_file = workdir.path().join("PackageInfo");
        info.write(File::create(&package_info_file)?)?;
        let directory = directory.as_ref();
        let bom = ReceiptBuilder::new().create(directory)?;
        let bom_file = workdir.path().join("Bom");
        bom.write(File::create(&bom_file)?)?;
        let payload_file = workdir.path().join("Payload");
        {
            let writer = ZlibEncoder::new(File::create(&payload_file)?, Compression::best());
            let mut archive = cpio::Builder::new(writer);
            archive.set_format(cpio::Format::Odc);
            archive.append_dir_all(directory)?;
            archive.finish()?.finish()?;
        }
        let mut xar = zar::Builder::new(writer, Some(signer));
        xar.append_dir_all(
            workdir.path(),
            zar::Compression::Gzip,
            zar::no_extra_contents,
        )?;
        xar.finish()?;
        Ok(())
    }

    pub fn file_name(&self) -> String {
        format!("{}-{}.pkg", self.identifier, self.version)
    }
}

const GENERATOR_VERSION: &str = concat!("Wolfpack/", env!("CARGO_PKG_VERSION"));

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::process::Command;

    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use arbtest::arbtest;
    use rand::rngs::OsRng;
    use rand::Rng;
    use rand_mt::Mt64;
    use tempfile::TempDir;
    use zar::rsa::RsaPrivateKey;
    use zar::ChecksumAlgo;

    use super::*;
    use crate::test::prevent_concurrency;
    use crate::test::Chars;
    use crate::test::DirectoryOfFiles;
    use crate::test::CONTROL;
    use crate::test::UNICODE;

    #[ignore = "Needs `darling`"]
    #[test]
    fn darling_installer_installs_random_package() {
        assert!(Command::new("mount")
            .arg("-t")
            .arg("tmpfs")
            .arg("tmpfs")
            .arg("/root")
            .status()
            .unwrap()
            .success());
        let _guard = prevent_concurrency("macos");
        let signing_key = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
        let signer = PackageSigner::new(ChecksumAlgo::Sha1, signing_key, Vec::new()).unwrap();
        let workdir = TempDir::new().unwrap();
        let package_file = workdir.path().join("test.pkg");
        arbtest(|u| {
            let package: Package = u.arbitrary()?;
            let directory: DirectoryOfFiles = u.arbitrary()?;
            package
                .clone()
                .write(
                    &mut File::create(package_file.as_path()).unwrap(),
                    directory.path(),
                    &signer,
                )
                .unwrap();
            assert!(
                Command::new("darling")
                    .arg("shell")
                    .arg("xar")
                    .arg("-tf")
                    .arg(format!("/Volumes/SystemRoot{}", package_file.display()))
                    .status()
                    .unwrap()
                    .success(),
                "manifest:\n========{:?}========",
                package
            );
            assert!(
                Command::new("darling")
                    .arg("shell")
                    .arg("installer")
                    .arg("-verbose")
                    .arg("-target")
                    .arg("/")
                    .arg("-pkg")
                    .arg(format!("/Volumes/SystemRoot{}", package_file.display()))
                    .status()
                    .unwrap()
                    .success(),
                "manifest:\n========{:?}========",
                package
            );
            Ok(())
        });
    }

    impl<'a> Arbitrary<'a> for Package {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let seed: u64 = u.arbitrary()?;
            let mut rng = Mt64::new(seed);
            let valid_chars = Chars::from(UNICODE).difference(CONTROL);
            let len = rng.gen_range(1..=10);
            let identifier = valid_chars.random_string(&mut rng, len);
            let len = rng.gen_range(1..=10);
            let version = valid_chars.random_string(&mut rng, len);
            Ok(Self {
                identifier,
                version,
            })
        }
    }
}
