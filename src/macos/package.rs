use std::fs::File;
use std::io::Error;
use std::io::Write;
use std::path::Path;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use tempfile::TempDir;

use crate::cpio::CpioBuilder;
use crate::macos::xml;
use crate::macos::Bom;
use crate::macos::PackageSigner;
use crate::xar::SignedXarBuilder;
use crate::xar::XarCompression;

#[cfg_attr(test, derive(arbitrary::Arbitrary, PartialEq, Eq, Clone, Debug))]
pub struct Package {
    pub identifier: String,
    pub version: String,
}

impl Package {
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
            generator_version: Some("wolfpack".into()),
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
        let bom = Bom::from_directory(directory)?;
        let bom_file = workdir.path().join("Bom");
        bom.write(File::create(&bom_file)?)?;
        let payload_file = workdir.path().join("Payload");
        CpioBuilder::from_directory(
            ZlibEncoder::new(File::create(&payload_file)?, Compression::best()),
            directory,
        )?
        .finish()?;
        let mut xar = SignedXarBuilder::new(writer, signer);
        xar.add_file_by_path(
            "PackageInfo".into(),
            &package_info_file,
            XarCompression::Gzip,
        )?;
        xar.add_file_by_path("Bom".into(), &bom_file, XarCompression::Gzip)?;
        xar.add_file_by_path("Payload".into(), &payload_file, XarCompression::None)?;
        xar.sign(signer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::process::Command;
    use std::time::Duration;

    use arbtest::arbtest;
    use tempfile::TempDir;

    use super::*;
    use crate::macos::PackageSigner;
    use crate::macos::SigningKey;
    use crate::test::prevent_concurrency;
    use crate::test::DirectoryOfFiles;

    #[ignore]
    #[test]
    fn macos_installer_installs_random_package() {
        assert!(Command::new("mount")
            .arg("-t")
            .arg("tmpfs")
            .arg("tmpfs")
            .arg("/root")
            .status()
            .unwrap()
            .success());
        let _guard = prevent_concurrency("macos");
        let (signing_key, _verifying_key) = SigningKey::generate("wolfpack".into()).unwrap();
        let signer = PackageSigner::new(signing_key);
        let workdir = TempDir::new().unwrap();
        let package_file = workdir.path().join("test.pkg");
        //let verifying_key_file = workdir.path().join("verifying-key");
        //verifying_key
        //    .write_armored(File::create(verifying_key_file.as_path()).unwrap())
        //    .unwrap();
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
        })
        .budget(Duration::from_secs(5));
    }
}
