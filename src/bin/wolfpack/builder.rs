use fs_err::create_dir_all;
use fs_err::File;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use walkdir::WalkDir;
use wolfpack::deb;
use wolfpack::elf;
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
    formats: BTreeSet<PackageFormat>,
}

impl PackageBuilder {
    pub fn new(formats: BTreeSet<PackageFormat>) -> Self {
        Self { formats }
    }

    pub fn build_packages(
        &self,
        input_dir: &Path,
        output_dir: &Path,
        signing_key_generator: &SigningKeyGenerator,
    ) -> Result<(), Error> {
        for entry in WalkDir::new(input_dir).into_iter() {
            let entry = entry?;
            let rootfs_dir = entry.path();
            if rootfs_dir.file_name() != Some(OsStr::new("rootfs")) {
                continue;
            }
            let parent = rootfs_dir.parent().expect("Parent exists");
            let metadata_file = parent.join("wolfpack.toml");
            if !metadata_file.exists() {
                continue;
            }
            self.build_package(
                &metadata_file,
                rootfs_dir,
                output_dir,
                signing_key_generator,
            )?;
        }
        Ok(())
    }

    pub fn build_package(
        &self,
        metadata_file: &Path,
        rootfs_dir: &Path,
        output_dir: &Path,
        signing_key_generator: &SigningKeyGenerator,
    ) -> Result<(), Error> {
        let metadata = fs_err::read_to_string(metadata_file)?;
        let metadata: PackageMetadata = toml::from_str(&metadata)?;
        let elf_targets = elf::Target::scan_dir(rootfs_dir)?;
        let elf_target = match elf_targets.len() {
            0 => None,
            1 => Some(elf_targets.into_iter().next().expect("Checked length")),
            _ => {
                use std::fmt::Write;
                let mut buf = String::with_capacity(4096);
                buf.push_str("Multiple ELF targets found: ");
                let mut iter = elf_targets.into_iter();
                let _ = write!(&mut buf, "{}", iter.next().expect("More than one target"));
                for target in iter {
                    let _ = write!(&mut buf, ", {}", target);
                }
                return Err(std::io::Error::other(buf).into());
            }
        };
        for format in self.formats.iter() {
            format.build_package(
                metadata.clone(),
                elf_target,
                rootfs_dir,
                output_dir,
                signing_key_generator,
            )?;
        }
        Ok(())
    }

    pub fn build_repo(
        &self,
        metadata_file: Option<&Path>,
        input_dir: &Path,
        output_dir: &Path,
        signing_key_generator: &SigningKeyGenerator,
    ) -> Result<(), Error> {
        let metadata = match metadata_file {
            Some(metadata_file) => {
                let metadata = fs_err::read_to_string(metadata_file)?;
                let metadata: RepoMetadata = toml::from_str(&metadata)?;
                metadata
            }
            None => Default::default(),
        };
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
    FreeBsdPkg,
    MacOsPkg,
    Msix,
}

impl PackageFormat {
    pub const fn linux() -> &'static [Self] {
        use PackageFormat::*;
        &[Deb, Rpm, Ipk]
    }

    pub fn freebsd() -> &'static [Self] {
        use PackageFormat::*;
        &[FreeBsdPkg]
    }

    pub fn macos() -> &'static [Self] {
        use PackageFormat::*;
        &[MacOsPkg]
    }

    pub fn windows() -> &'static [Self] {
        use PackageFormat::*;
        &[Msix]
    }

    pub const NATIVE: &'static str = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "freebsd") {
        "freebsd"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };

    pub const fn file_extension(&self) -> &'static str {
        use PackageFormat::*;
        match self {
            Deb => "deb",
            Rpm => "rpm",
            Ipk => "ipk",
            FreeBsdPkg => "pkg",
            MacOsPkg => "pkg",
            Msix => "msix",
        }
    }

    pub const fn os_name(&self) -> &'static str {
        use PackageFormat::*;
        match self {
            Deb | Rpm | Ipk => "linux",
            FreeBsdPkg => "freebsd",
            MacOsPkg => "macos",
            Msix => "windows",
        }
    }

    pub fn build_package(
        &self,
        metadata: PackageMetadata,
        elf_target: Option<elf::Target>,
        rootfs_dir: &Path,
        output_dir: &Path,
        signing_key_generator: &SigningKeyGenerator,
    ) -> Result<(), Error> {
        log::info!("Building {} {:?}...", metadata.common.name, self);
        create_dir_all(output_dir)?;
        let output_file = match self {
            Self::Deb => {
                let mut package: deb::Package = metadata.common.try_into()?;
                package.architecture = elf_target.into();
                let output_file = output_dir.join(package.file_name());
                let (signing_key, _verifying_key) = signing_key_generator.deb()?;
                let signer = deb::PackageSigner::new(signing_key);
                let file = File::create(&output_file)?;
                package.write(file, rootfs_dir, &signer)?;
                output_file
            }
            Self::Rpm => {
                let mut package: rpm::Package = metadata.common.try_into()?;
                package.arch = elf_target.into();
                let output_file = output_dir.join(package.file_name());
                let (signing_key, _) = signing_key_generator.rpm()?;
                let signer = rpm::PackageSigner::new(signing_key);
                let file = File::create(&output_file)?;
                package.write(file, rootfs_dir, &signer)?;
                output_file
            }
            Self::Ipk => {
                let mut package: ipk::Package = metadata.common.try_into()?;
                package.arch = elf_target.into();
                let output_file = output_dir.join(package.file_name());
                let (signing_key, _) = signing_key_generator.ipk()?;
                let signer = signing_key;
                package.write(&output_file, rootfs_dir, &signer)?;
                output_file
            }
            Self::FreeBsdPkg => {
                let manifest: pkg::CompactManifest = metadata.common.try_into()?;
                let package = pkg::Package::new(manifest, rootfs_dir.to_path_buf());
                let output_file = output_dir.join(package.file_name());
                // Only repositories are signed.
                let file = File::create(&output_file)?;
                package.write(file)?;
                output_file
            }
            Self::MacOsPkg => {
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
                output_file
            }
            Self::Msix => {
                // TODO signing
                let package: msix::Package = metadata.common.try_into()?;
                let output_file = output_dir.join(package.file_name());
                package.write(&output_file, rootfs_dir)?;
                output_file
            }
        };
        log::info!("Wrote {}", output_file.display());
        Ok(())
    }

    pub fn build_repo(
        &self,
        metadata: RepoMetadata,
        input_dir: &Path,
        output_dir: &Path,
        signing_key_generator: &SigningKeyGenerator,
    ) -> Result<(), Error> {
        create_dir_all(output_dir)?;
        {
            let canonical_input_dir = fs_err::canonicalize(input_dir)?;
            let canonical_output_dir = fs_err::canonicalize(output_dir)?;
            if canonical_input_dir.starts_with(&canonical_output_dir)
                || canonical_output_dir.starts_with(&canonical_input_dir)
            {
                return Err(Error::Other(format!(
                    "Input directory {input_dir:?} and output directory {output_dir:?} \
                    can't be nested and can't be the same"
                )));
            }
        }
        let mut output_dir = output_dir.to_path_buf();
        output_dir.push(self.os_name());
        output_dir.push(self.file_extension());
        create_dir_all(&output_dir)?;
        match self {
            Self::Deb => {
                let (signing_key, verifying_key) = signing_key_generator.deb()?;
                let verifier = deb::PackageVerifier::new(verifying_key);
                let repo = deb::Repository::new(&output_dir, [input_dir], &verifier)?;
                let suite = metadata.common.name.try_into()?;
                let signer = sign::PgpCleartextSigner::new(signing_key.into());
                repo.write(&output_dir, suite, &signer)?;
            }
            Self::Rpm => {
                let (signing_key, _verifying_key) = signing_key_generator.rpm()?;
                let signer = rpm::PackageSigner::new(signing_key);
                let repo = rpm::Repository::new(&output_dir, [input_dir])?;
                repo.write(&output_dir, &signer)?;
            }
            Self::Ipk => {
                let (signing_key, verifying_key) = signing_key_generator.ipk()?;
                let repo = ipk::Repository::new(&output_dir, [input_dir], &verifying_key)?;
                repo.write(&output_dir, &signing_key)?;
            }
            Self::FreeBsdPkg => {
                let (signing_key, _verifying_key) = signing_key_generator.pkg()?;
                let repo = pkg::Repository::new([input_dir])?;
                repo.build(&output_dir, &signing_key)?;
            }
            Self::MacOsPkg => {
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
                    fs_err::rename(
                        entry_path,
                        output_dir.join(entry_path.file_name().expect("File name exists")),
                    )?;
                }
            }
            Self::Msix => {
                for entry in WalkDir::new(input_dir).into_iter() {
                    let entry = entry.map_err(std::io::Error::other)?;
                    let entry_path = entry.path();
                    if entry_path.extension() != Some(OsStr::new("msix")) {
                        continue;
                    }
                    fs_err::rename(
                        entry_path,
                        output_dir.join(entry_path.file_name().expect("File name exists")),
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn parse_set(s: &str) -> Result<BTreeSet<Self>, Error> {
        let mut formats = BTreeSet::new();
        for word in s.split(',') {
            match word.trim() {
                "linux" => formats.extend(Self::linux()),
                "freebsd" => formats.extend(Self::freebsd()),
                "macos" => formats.extend(Self::macos()),
                "windows" => formats.extend(Self::windows()),
                other => {
                    formats.insert(other.parse()?);
                }
            }
        }
        Ok(formats)
    }
}

impl FromStr for PackageFormat {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "deb" => Ok(Self::Deb),
            "rpm" => Ok(Self::Rpm),
            "ipk" => Ok(Self::Ipk),
            "freebsd-pkg" => Ok(Self::FreeBsdPkg),
            "macos-pkg" => Ok(Self::MacOsPkg),
            "msix" => Ok(Self::Msix),
            _ => Err(std::io::ErrorKind::InvalidData.into()),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageMetadata {
    #[serde(flatten)]
    pub common: wolf::Metadata,
    // TODO deb overrides
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RepoMetadata {
    #[serde(flatten)]
    pub common: wolf::RepoMetadata,
}
