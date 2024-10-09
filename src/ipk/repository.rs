use std::fs::create_dir_all;
use std::path::Path;

use ksign::IO;

use crate::deb::Error;
use crate::ipk::PackageSigner;
use crate::ipk::PackageVerifier;
use crate::ipk::Packages;
use crate::ipk::SimpleValue;

pub struct Repository;

impl Repository {
    pub fn write<I, P, P2>(
        output_dir: P2,
        suite: SimpleValue,
        paths: I,
        verifier: &PackageVerifier,
        signer: &PackageSigner,
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
        let signature = signer.sign(packages_string.as_bytes());
        signature
            .write_to_file(output_dir.join("Packages.sig"))
            .map_err(|e| Error::other(e.to_string()))?;
        Ok(())
    }
}
