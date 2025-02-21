use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::create_dir_all;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::Deserialize;
use serde::Serialize;
use walkdir::WalkDir;
use wolfpack::deb;
use wolfpack::ipk;
use wolfpack::macos;
use wolfpack::msix;
use wolfpack::pkg;
use wolfpack::rpm;
use wolfpack::sign;
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

    pub fn build_package(
        &self,
        metadata_file: &Path,
        rootfs_dir: &Path,
        output_dir: &Path,
        signing_key_generator: &SigningKeyGenerator,
    ) -> Result<(), Error> {
        let metadata = std::fs::read_to_string(metadata_file)?;
        let metadata: PackageMetadata = toml::from_str(&metadata)?;
        for format in self.formats.iter() {
            format.build_package(
                metadata.clone(),
                rootfs_dir,
                output_dir,
                signing_key_generator,
            )?;
        }
        Ok(())
    }

    pub fn build_repo(
        &self,
        metadata_file: &Path,
        input_dir: &Path,
        output_dir: &Path,
        signing_key_generator: &SigningKeyGenerator,
    ) -> Result<(), Error> {
        let metadata = std::fs::read_to_string(metadata_file)?;
        let metadata: RepoMetadata = toml::from_str(&metadata)?;
        for format in self.formats.iter() {
            format.build_repo(
                metadata.clone(),
                input_dir,
                output_dir,
                signing_key_generator,
            )?;
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

    pub fn build_package(
        &self,
        metadata: PackageMetadata,
        rootfs_dir: &Path,
        output_dir: &Path,
        signing_key_generator: &SigningKeyGenerator,
    ) -> Result<(), Error> {
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

    pub fn build_repo(
        &self,
        metadata: RepoMetadata,
        input_dir: &Path,
        output_dir: &Path,
        signing_key_generator: &SigningKeyGenerator,
    ) -> Result<(), Error> {
        match self {
            Self::Deb => {
                let (signing_key, verifying_key) = signing_key_generator.deb()?;
                let verifier = deb::PackageVerifier::new(verifying_key);
                let repo = deb::Repository::new(output_dir, [input_dir], &verifier)?;
                let suite = metadata.name.try_into()?;
                let signer = sign::PgpCleartextSigner::new(signing_key.into());
                repo.write(output_dir, suite, &signer)?;
            }
            Self::Rpm => {
                let (signing_key, _verifying_key) = signing_key_generator.rpm()?;
                let signer = rpm::PackageSigner::new(signing_key);
                let repo = rpm::Repository::new([input_dir])?;
                repo.write(output_dir, &signer)?;
            }
            Self::Ipk => {
                let (signing_key, verifying_key) = signing_key_generator.ipk()?;
                let repo = ipk::Repository::new(output_dir, [input_dir], &verifying_key)?;
                repo.write(output_dir, &signing_key)?;
            }
            Self::Pkg => {
                let (signing_key, _verifying_key) = signing_key_generator.pkg()?;
                let repo = pkg::Repository::new([input_dir])?;
                repo.build(output_dir, &signing_key)?;
            }
            Self::MacOs => {
                let macos_repo_dir = output_dir.join("macos");
                create_dir_all(&macos_repo_dir)?;
                for entry in WalkDir::new(input_dir).into_iter() {
                    let entry = entry.map_err(std::io::Error::other)?;
                    let entry_path = entry.path();
                    if entry_path.extension() != Some(OsStr::new("pkg")) {
                        continue;
                    }
                    {
                        let mut file = File::open(entry_path)?;
                        let mut magic = [0_u8; 4];
                        let n = file.read(&mut magic[..])?;
                        if n != magic.len() || magic != *b"xar!" {
                            continue;
                        }
                    }
                    std::fs::rename(
                        entry_path,
                        macos_repo_dir.join(entry_path.file_name().expect("File name exists")),
                    )?;
                }
            }
            Self::Msix => {
                let msix_repo_dir = output_dir.join("msix");
                create_dir_all(&msix_repo_dir)?;
                for entry in WalkDir::new(input_dir).into_iter() {
                    let entry = entry.map_err(std::io::Error::other)?;
                    let entry_path = entry.path();
                    if entry_path.extension() != Some(OsStr::new("msix")) {
                        continue;
                    }
                    std::fs::rename(
                        entry_path,
                        msix_repo_dir.join(entry_path.file_name().expect("File name exists")),
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageMetadata {
    #[serde(flatten)]
    pub common: wolf::Metadata,
    // TODO deb overrides
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoMetadata {
    pub name: String,
}
