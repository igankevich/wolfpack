use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use pgp::composed::cleartext::CleartextSignedMessage;
use pgp::types::SecretKeyTrait;
use pgp::types::SignatureBytes;
use rand::rngs::OsRng;
use walkdir::WalkDir;
use xz::write::XzEncoder;

use crate::archive::ArchiveWrite;
use crate::hash::Sha256Reader;
use crate::pkg::Package;
use crate::pkg::PackageMeta;

pub struct Packages {
    packages: Vec<PackageMeta>,
}

impl Packages {
    pub fn new<I, P>(paths: I) -> Result<Self, std::io::Error>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut packages = Vec::new();
        let mut push_package = |path: &Path| -> Result<(), std::io::Error> {
            eprintln!("reading {}", path.display());
            let mut reader = Sha256Reader::new(File::open(path)?);
            let compact = Package::read_compact_manifest(&mut reader)?;
            let (sha256, size) = reader.digest()?;
            let meta = PackageMeta {
                compact,
                pkgsize: size as u64,
                sum: sha256.to_string(),
                path: path.to_path_buf(),
                repopath: path.to_path_buf(),
            };
            packages.push(meta);
            Ok(())
        };
        for path in paths.into_iter() {
            let path = path.as_ref();
            if path.is_dir() {
                for entry in WalkDir::new(path).into_iter() {
                    let entry = entry?;
                    if entry.file_type().is_dir()
                        || entry.path().extension() != Some(OsStr::new("pkg"))
                    {
                        continue;
                    }
                    push_package(entry.path())?
                }
            } else {
                push_package(path)?
            }
        }
        Ok(Self { packages })
    }

    pub fn build<W: Write>(
        &self,
        writer: W,
        signing_key: &pgp::SignedSecretKey,
    ) -> Result<(), std::io::Error> {
        let mut package = tar::Builder::new(XzEncoder::new(writer, COMPRESSION_LEVEL));
        let mut contents = Vec::new();
        for manifest in self.packages.iter() {
            contents.extend(manifest.to_vec()?);
        }
        let signed_contents = CleartextSignedMessage::sign(
            OsRng,
            std::str::from_utf8(&contents).map_err(std::io::Error::other)?,
            &signing_key,
            String::new,
        )
        .map_err(std::io::Error::other)?;
        package.add_regular_file("packagesite.yaml", contents)?;
        package.add_regular_file(
            "packagesite.yaml.sig",
            to_bytes(&signed_contents.signatures()[0].signature.signature),
        )?;
        let public_key = signing_key
            .public_key()
            .sign(OsRng, signing_key, String::new)
            .map_err(std::io::Error::other)?;
        package.add_regular_file(
            "packagesite.yaml.pub",
            public_key
                .to_armored_bytes(Default::default())
                .map_err(std::io::Error::other)?,
        )?;
        package.into_inner()?.finish()?;
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &PackageMeta> {
        self.packages.iter()
    }
}

impl IntoIterator for Packages {
    type Item = <Vec<PackageMeta> as IntoIterator>::Item;
    type IntoIter = <Vec<PackageMeta> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.packages.into_iter()
    }
}

fn to_bytes(s: &SignatureBytes) -> Vec<u8> {
    match s {
        SignatureBytes::Mpis(x) => x.iter().flat_map(|x| x.as_bytes()).copied().collect(),
        SignatureBytes::Native(x) => x.clone(),
    }
}

const COMPRESSION_LEVEL: u32 = 9;
