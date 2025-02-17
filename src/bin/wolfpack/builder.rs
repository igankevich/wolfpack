use std::collections::HashSet;
use std::fs::File;
use std::path::Path;

use serde::Deserialize;
use serde::Serialize;
use wolfpack::deb;
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
}

impl PackageFormat {
    pub fn all() -> &'static [Self] {
        use PackageFormat::*;
        &[Deb]
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
                let file = File::create(&output_file)?;
                let (signing_key, _) = signing_key_generator.deb()?;
                let signer = deb::PackageSigner::new(signing_key);
                package.write(rootfs_dir, file, &signer)?;
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
