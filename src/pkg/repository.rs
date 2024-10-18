use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use normalize_path::NormalizePath;

use serde::Deserialize;
use serde::Serialize;
use walkdir::WalkDir;
use xz::write::XzEncoder;

use crate::archive::ArchiveWrite;
use crate::archive::TarBuilder;
use crate::hash::Sha256Reader;
use crate::pkg::Package;
use crate::pkg::PackageMeta;
use crate::pkg::SigningKey;

pub struct Repository {
    packages: Vec<PackageMeta>,
}

impl Repository {
    pub fn new<I, P>(paths: I) -> Result<Self, std::io::Error>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut packages = Vec::new();
        let mut push_package = |directory: &Path, path: &Path| -> Result<(), std::io::Error> {
            eprintln!("reading {}", path.display());
            let relative_path = Path::new(".").join(
                path.strip_prefix(directory)
                    .map_err(std::io::Error::other)?
                    .normalize(),
            );
            let mut reader = Sha256Reader::new(File::open(path)?);
            let compact = Package::read_compact_manifest(&mut reader)?;
            let (sha256, size) = reader.digest()?;
            let meta = PackageMeta {
                compact,
                pkgsize: size as u32,
                sum: sha256.to_string(),
                path: relative_path.clone(),
                repopath: relative_path,
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
                    push_package(path, entry.path())?
                }
            } else {
                // TODO
                push_package(Path::new("."), path)?
            }
        }
        Ok(Self { packages })
    }

    pub fn build<W: Write>(
        &self,
        writer: W,
        signing_key: &SigningKey,
    ) -> Result<(), std::io::Error> {
        // TODO meta.txz
        // TODO data.pkg
        let mut packagesite = TarBuilder::new(XzEncoder::new(writer, COMPRESSION_LEVEL));
        let mut contents = Vec::new();
        for manifest in self.packages.iter() {
            contents.extend(manifest.to_vec()?);
            contents.push(b'\n');
        }
        let signature = signing_key
            .sign(&contents[..])
            .map_err(|_| std::io::Error::other("signing failed"))?;
        let mut signature_encoded = Vec::new();
        signature_encoded.extend(b"$PKGSIGN:ecdsa$");
        // TODO format
        signature_encoded.extend(signature.serialize_der());
        packagesite.add_regular_file("signature", signature_encoded)?;
        packagesite.add_regular_file("packagesite.yaml", contents)?;
        //let verifying_key = signing_key.verifying_key();
        //packagesite.add_regular_file(
        //    "packagesite.yaml.pub",
        //    verifying_key
        //        .to_armored_bytes(Default::default())
        //        .map_err(std::io::Error::other)?,
        //)?;
        packagesite.into_inner()?.finish()?;
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &PackageMeta> {
        self.packages.iter()
    }
}

impl IntoIterator for Repository {
    type Item = <Vec<PackageMeta> as IntoIterator>::Item;
    type IntoIter = <Vec<PackageMeta> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.packages.into_iter()
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MirrorType {
    Srv,
    Http,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SignatureType {
    Pubkey,
    Fingerprints,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RepoConf {
    #[serde(skip)]
    pub name: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
    pub url: String,
    #[serde(skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirror_type: Option<MirrorType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_type: Option<SignatureType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pubkey: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprints: Option<PathBuf>,
    #[serde(skip_serializing_if = "is_zero")]
    pub ip_version: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub priority: u32,
}

impl RepoConf {
    pub fn new(name: String, url: String, pubkey: PathBuf) -> Self {
        Self {
            name,
            env: Default::default(),
            url,
            enabled: true,
            mirror_type: None,
            signature_type: Some(SignatureType::Pubkey),
            pubkey: Some(pubkey),
            fingerprints: None,
            ip_version: 0,
            priority: 0,
        }
    }
}

impl ToString for RepoConf {
    fn to_string(&self) -> String {
        let mut wrapper = HashMap::new();
        wrapper.insert(self.name.clone(), self);
        serde_json::to_string_pretty(&wrapper).unwrap()
    }
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

fn is_true(value: &bool) -> bool {
    *value
}

const COMPRESSION_LEVEL: u32 = 9;

#[cfg(test)]
mod tests {
    use std::fs::create_dir_all;
    use std::process::Command;

    //use std::time::Duration;
    use arbtest::arbtest;
    use tempfile::TempDir;

    use super::*;
    use crate::pkg::CompactManifest;
    use crate::test::DirectoryOfFiles;

    #[test]
    fn write_read() {
        arbtest(|u| {
            let package: CompactManifest = u.arbitrary()?;
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let mut buf: Vec<u8> = Vec::new();
            Package::new(package.clone(), directory.path().into())
                .write(&mut buf)
                .unwrap();
            let actual = Package::read_compact_manifest(&buf[..]).unwrap();
            assert_eq!(package, actual);
            Ok(())
        });
    }

    #[ignore]
    #[test]
    fn freebsd_pkg_adds_repo() {
        let workdir = TempDir::new().unwrap();
        let package_file = workdir.path().join("test.pkg");
        let verifying_key_file = workdir.path().join("verifying-key");
        arbtest(|u| {
            let mut package: CompactManifest = u.arbitrary()?;
            package.flatsize = 100;
            package.deps.clear(); // missing dependencies
            package.arch = "Linux:3.2.0:amd64".into();
            package.abi = "Linux:3.2.0:amd64".into();
            let directory: DirectoryOfFiles = u.arbitrary()?;
            Package::new(package.clone(), directory.path().into())
                .write(File::create(package_file.as_path()).unwrap())
                .unwrap();
            let (signing_key, verifying_key) = SigningKey::generate();
            std::fs::write(&verifying_key_file, verifying_key.to_der().unwrap()).unwrap();
            let repository = Repository::new([workdir.path()]).unwrap();
            repository
                .build(
                    File::create(workdir.path().join("packagesite.pkg")).unwrap(),
                    &signing_key,
                )
                .unwrap();
            create_dir_all("/etc/pkg").unwrap();
            let repo_conf = RepoConf::new(
                "test".into(),
                format!("file://{}", workdir.path().display()),
                verifying_key_file.clone(),
            );
            std::fs::write("/etc/pkg/test.conf", repo_conf.to_string()).unwrap();
            assert!(Command::new("find")
                .arg(workdir.path())
                .status()
                .unwrap()
                .success());
            assert!(Command::new("cat")
                .arg("/etc/pkg/test.conf")
                .status()
                .unwrap()
                .success());
            assert!(
                Command::new("pkg")
                    .arg("--debug")
                    .arg("update")
                    .arg("--force")
                    .arg("--repository")
                    .arg("test")
                    .status()
                    .unwrap()
                    .success(),
                "repo.conf = {:?}",
                repo_conf
            );
            assert!(
                Command::new("pkg")
                    .arg("install")
                    .arg("-y")
                    .arg(package.name.to_string())
                    .status()
                    .unwrap()
                    .success(),
                "manifest:\n========{:?}========",
                package
            );
            assert!(
                Command::new("pkg")
                    .arg("remove")
                    .arg("-y")
                    .arg(package.name.to_string())
                    .status()
                    .unwrap()
                    .success(),
                "manifest:\n========{:?}========",
                package
            );
            Ok(())
        });
        //.budget(Duration::from_secs(5));
    }
}
