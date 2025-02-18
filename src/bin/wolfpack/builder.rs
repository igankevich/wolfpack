use std::collections::HashSet;
use std::fs::File;
use std::path::Path;

use serde::Deserialize;
use serde::Serialize;
use wolfpack::deb;
use wolfpack::ipk;
use wolfpack::macos;
use wolfpack::msix;
use wolfpack::pkg;
use wolfpack::rpm;
use wolfpack::wolf;

use crate::Error;
use crate::SigningKeyGenerator;

pub struct PackageBuilder {
    formats: HashSet<PackageFormat>,
}

impl PackageBuilder {
    pub fn new(formats: HashSet<PackageFormat>) -> Self {
        Self { formats }
    }

    pub fn build(
        &self,
        metadata_file: &Path,
        rootfs_dir: &Path,
        output_dir: &Path,
        signing_key_generator: &SigningKeyGenerator,
    ) -> Result<(), Error> {
        for format in self.formats.iter() {
            format.build(metadata_file, rootfs_dir, output_dir, signing_key_generator)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum PackageFormat {
    Deb,
    Rpm,
    Ipk,
    Pkg,
    MacOs,
    Msix,
}

impl PackageFormat {
    // TODO split into linux, freebsd, macos, windows
    pub fn all() -> &'static [Self] {
        use PackageFormat::*;
        &[Deb, Rpm, Ipk, Pkg, MacOs, Msix]
    }

    pub fn build(
        &self,
        metadata_file: &Path,
        rootfs_dir: &Path,
        output_dir: &Path,
        signing_key_generator: &SigningKeyGenerator,
    ) -> Result<(), Error> {
        let metadata = std::fs::read_to_string(metadata_file)?;
        let metadata: PackageMetadata = toml::from_str(&metadata)?;
        match self {
            Self::Deb => {
                let package: deb::Package = metadata.common.try_into()?;
                let output_file = output_dir.join(package.file_name());
                let (signing_key, _) = signing_key_generator.deb()?;
                let signer = deb::PackageSigner::new(signing_key);
                let file = File::create(&output_file)?;
                package.write(file, rootfs_dir, &signer)?;
            }
            Self::Rpm => {
                let package: rpm::Package = metadata.common.try_into()?;
                let output_file = output_dir.join(package.file_name());
                let (signing_key, _) = signing_key_generator.rpm()?;
                let signer = rpm::PackageSigner::new(signing_key);
                let file = File::create(&output_file)?;
                package.write(file, rootfs_dir, &signer)?;
            }
            Self::Ipk => {
                let package: ipk::Package = metadata.common.try_into()?;
                let output_file = output_dir.join(package.file_name());
                let (signing_key, _) = signing_key_generator.ipk()?;
                let signer = signing_key;
                package.write(output_file, rootfs_dir, &signer)?;
            }
            Self::Pkg => {
                let manifest: pkg::CompactManifest = metadata.common.try_into()?;
                let package = pkg::Package::new(manifest, rootfs_dir.to_path_buf());
                let output_file = output_dir.join(package.file_name());
                // Only repositories are signed.
                let file = File::create(&output_file)?;
                package.write(file)?;
            }
            Self::MacOs => {
                let (signing_key, _) = signing_key_generator.macos()?;
                // TODO
                let certs = Vec::new();
                let signer =
                    macos::PackageSigner::new(macos::ChecksumAlgo::Sha256, signing_key, certs)?;
                let package = macos::Package {
                    // TODO this is not enough
                    identifier: metadata.common.name,
                    version: metadata.common.version,
                };
                let output_file = output_dir.join(package.file_name());
                let file = File::create(&output_file)?;
                package.write(file, rootfs_dir, &signer)?;
            }
            Self::Msix => {
                // TODO signing
                let package: msix::Package = metadata.common.try_into()?;
                let output_file = output_dir.join(package.file_name());
                package.write(&output_file, rootfs_dir)?;
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageMetadata {
    #[serde(flatten)]
    pub common: wolf::Metadata,
    // TODO deb overrides
}
