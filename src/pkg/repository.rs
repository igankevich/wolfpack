use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::os::unix::fs::symlink;
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
                    if entry.file_type().is_dir() || !is_package(entry.path()) {
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

    pub fn build<P: AsRef<Path>>(
        self,
        output_dir: P,
        signing_key: &SigningKey,
    ) -> Result<(), std::io::Error> {
        let output_dir = output_dir.as_ref();
        let meta = MetaConf::default().to_string();
        std::fs::write(output_dir.join("meta.conf"), &meta)?;
        symlink("meta.conf", output_dir.join("meta"))?;
        tar_xz_from_signed_file(
            Path::new("meta"),
            output_dir.join("meta.txz"),
            &meta,
            signing_key,
        )?;
        let mut packagesite = Vec::new();
        for manifest in self.packages.iter() {
            packagesite.extend(manifest.to_vec()?);
            packagesite.push(b'\n');
        }
        tar_xz_from_signed_file(
            Path::new("packagesite.yaml"),
            output_dir.join("packagesite.pkg"),
            packagesite,
            signing_key,
        )?;
        symlink("packagesite.pkg", output_dir.join("packagesite.txz"))?;
        let data_pkg = DataPkg {
            groups: Default::default(),
            packages: self.packages,
        };
        tar_xz_from_signed_file(
            Path::new("data"),
            output_dir.join("data.pkg"),
            data_pkg.to_vec()?,
            signing_key,
        )?;
        symlink("data.pkg", output_dir.join("data.txz"))?;
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

#[derive(Serialize, Deserialize, Debug)]
pub struct MetaConf {
    version: u32,
    packing_format: PackingFormat,
    manifests: String,
    data: String,
    filesite: String,
    manifests_archive: String,
    filesite_archive: String,
}

impl Default for MetaConf {
    fn default() -> Self {
        Self {
            version: 2,
            packing_format: Default::default(),
            manifests: "packagesite.yaml".into(),
            data: "data".into(),
            filesite: "filesite.yaml".into(),
            manifests_archive: "packagesite".into(),
            filesite_archive: "filesite".into(),
        }
    }
}

impl ToString for MetaConf {
    fn to_string(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum PackingFormat {
    Tzst,
    #[default]
    Txz,
    Tbz,
    Tgz,
    Tar,
}

impl PackingFormat {
    pub fn as_str(&self) -> &str {
        use PackingFormat::*;
        match self {
            Tzst => "tzst",
            Txz => "txz",
            Tbz => "tbz",
            Tgz => "tgz",
            Tar => "tar",
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct DataPkg {
    pub groups: Vec<String>,
    pub packages: Vec<PackageMeta>,
}

impl DataPkg {
    pub fn to_vec(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_package(path: &Path) -> bool {
    let Some(extension) = path.extension() else {
        return false;
    };
    PACKAGE_EXTENSIONS
        .iter()
        .any(|e| OsStr::new(e) == extension)
}

fn tar_xz_from_signed_file<P1, P2, C>(
    inner_path: P1,
    outer_path: P2,
    contents: C,
    signing_key: &SigningKey,
) -> Result<(), std::io::Error>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
    C: AsRef<[u8]>,
{
    let signature = sign(signing_key, contents.as_ref())?;
    TarXzFile::from_files(
        [
            // signature should be the first entry in the archive
            (Path::new("signature"), &signature[..]),
            (inner_path.as_ref(), contents.as_ref()),
        ],
        xz_file(outer_path)?,
    )?
    .finish()?;
    Ok(())
}

fn xz_file<P: AsRef<Path>>(path: P) -> Result<XzFile, std::io::Error> {
    Ok(XzEncoder::new(File::create(path)?, COMPRESSION_LEVEL))
}

fn sign<C: AsRef<[u8]>>(signing_key: &SigningKey, contents: C) -> Result<Vec<u8>, std::io::Error> {
    let signature = signing_key
        .sign(contents.as_ref())
        .map_err(|_| std::io::Error::other("signing failed"))?;
    let mut s = Vec::new();
    s.extend(b"$PKGSIGN:ecdsa$");
    s.extend(signature.serialize_der());
    Ok(s)
}

const COMPRESSION_LEVEL: u32 = 9;
const PACKAGE_EXTENSIONS: [&str; 6] = ["pkg", "tzst", "txz", "tbz", "tgz", "tar"];

type XzFile = XzEncoder<File>;
type TarXzFile = TarBuilder<XzFile>;

#[cfg(test)]
mod tests {
    use std::fs::create_dir_all;
    use std::process::Command;
    use std::time::Duration;

    use arbtest::arbtest;
    use tempfile::TempDir;

    use super::*;
    use crate::pkg::CompactManifest;
    use crate::test::prevent_concurrency;
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
        let _guard = prevent_concurrency("freebsd-pkg");
        arbtest(|u| {
            let workdir = TempDir::new().unwrap();
            let package_file = workdir.path().join("test.pkg");
            let verifying_key_file = workdir.path().join("verifying-key");
            let signing_key_file = workdir.path().join("signing-key");
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
            std::fs::write(&signing_key_file, signing_key.to_der().unwrap()).unwrap();
            let repository = Repository::new([workdir.path()]).unwrap();
            repository.build(workdir.path(), &signing_key).unwrap();
            create_dir_all("/etc/pkg").unwrap();
            let repo_conf = RepoConf::new(
                "test".into(),
                format!("file://{}", workdir.path().display()),
                verifying_key_file.clone(),
            );
            std::fs::write("/etc/pkg/test.conf", format!("{}\n", repo_conf.to_string())).unwrap();
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
        })
        .budget(Duration::from_secs(5));
    }
}
