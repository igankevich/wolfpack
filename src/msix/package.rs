use std::fs::File;
use std::fs::OpenOptions;
use std::io::Error;
use std::io::Write;
use std::path::Path;

use normalize_path::NormalizePath;
use walkdir::WalkDir;
use zip::read::ZipArchive;
use zip::write::SimpleFileOptions;
use zip::write::ZipWriter;

use crate::hash::Sha256Reader;
use crate::msix::xml;
use crate::wolf;

#[derive(Clone)]
#[cfg_attr(test, derive(arbitrary::Arbitrary, PartialEq, Eq, Debug))]
#[allow(unused)]
pub struct Package {
    pub name: String,
    pub description: String,
    pub publisher: String,
    pub version: String,
    pub executable: String,
    pub logo: String,
}

impl Package {
    #[allow(unused)]
    pub fn write<P2: AsRef<Path>, P: AsRef<Path>>(
        &self,
        file: P2,
        directory: P,
        //signer: &PackageSigner,
    ) -> Result<(), Error> {
        let file = file.as_ref();
        let directory = directory.as_ref();
        let mut writer = ZipWriter::new(File::create(file)?);
        for entry in WalkDir::new(directory).into_iter() {
            let entry = entry?;
            let entry_path = entry
                .path()
                .strip_prefix(directory)
                .map_err(Error::other)?
                .normalize();
            if entry_path == Path::new("") {
                continue;
            }
            let relative_path = Path::new(".").join(entry_path);
            // TODO symlinks
            if entry.file_type().is_dir() {
                writer.add_directory_from_path(relative_path, SimpleFileOptions::default())?;
            } else {
                writer.start_file_from_path(relative_path, SimpleFileOptions::default())?;
                std::io::copy(&mut File::open(entry.path())?, writer.by_ref())?;
            }
        }
        writer.finish()?;
        let mut archive = ZipArchive::new(File::open(file)?)?;
        let mut files = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            // TODO raw affects size or not ???
            let mut file = archive.by_index_raw(i)?;
            if file.is_dir() {
                continue;
            }
            let sha256_reader = Sha256Reader::new(&mut file);
            let (hash, _) = sha256_reader.digest()?;
            files.push(xml::File {
                name: file.name().into(),
                size: file.size(),
                lfh_size: file.data_start() - file.header_start(),
                blocks: vec![xml::Block {
                    hash: hash.to_base64(),
                    size: file.compressed_size(),
                }],
            });
        }
        drop(archive);
        let block_map = xml::BlockMap {
            hash_method: "http://www.w3.org/2001/04/xmlenc#sha256".into(),
            files,
        };
        let content_types = xml::Types {
            overrides: vec![xml::Override {
                content_type: "application/vnd.ms-appx.blockmap+xml".into(),
                part_name: "/AppxBlockMap.xml".into(),
            }],
            defaults: vec![],
        };
        let manifest = xml::Package {
            identity: xml::Identity {
                name: self.name.clone(),
                publisher: self.publisher.clone(),
                version: self.version.clone(),
            },
            properties: xml::Properties {
                display_name: self.name.clone(),
                publisher_display_name: self.publisher.clone(),
                description: self.description.clone(),
                logo: self.logo.clone(),
            },
            resources: xml::Resources {
                resources: vec![xml::Resource {
                    language: "x-generate".into(),
                }],
            },
            dependencies: xml::Dependencies {
                target_device_families: vec![xml::TargetDeviceFamily {
                    name: "Platform.All".into(),
                    min_version: "0.0.0.0".into(),
                    max_version_tested: "0.0.0.0".into(),
                }],
            },
            applications: xml::Applications {
                applications: vec![xml::Application {
                    id: self.name.clone(),
                    executable: self.executable.clone(),
                    visual_elements: xml::VisualElements {
                        display_name: self.name.clone(),
                        description: self.description.clone(),
                        background_color: "white".into(),
                        square150x150_logo: self.logo.clone(),
                        square44x44_logo: self.logo.clone(),
                        app_list_entry: "none".into(),
                    },
                }],
            },
        };
        let mut writer =
            ZipWriter::new_append(OpenOptions::new().read(true).write(true).open(file)?)?;
        writer.start_file_from_path("AppxBlockMap.xml", SimpleFileOptions::default())?;
        block_map.write(writer.by_ref())?;
        writer.start_file_from_path("[Content_Types].xml", SimpleFileOptions::default())?;
        content_types.write(writer.by_ref())?;
        writer.start_file_from_path("AppxManifest.xml", SimpleFileOptions::default())?;
        manifest.write(writer.by_ref())?;
        writer.finish()?;
        Ok(())
    }

    pub fn file_name(&self) -> String {
        // TODO arch?
        format!("{}_{}.msix", self.name, self.version)
    }
}

impl TryFrom<wolf::Metadata> for Package {
    type Error = Error;
    fn try_from(other: wolf::Metadata) -> Result<Self, Self::Error> {
        Ok(Self {
            name: other.name,
            version: other.version,
            description: other.description,
            publisher: Default::default(),
            executable: Default::default(),
            logo: Default::default(),
        })
    }
}

#[cfg(test)]
mod tests {

    use std::process::Command;

    use arbtest::arbtest;
    use tempfile::TempDir;

    use super::*;
    use crate::test::prevent_concurrency;
    use crate::test::DirectoryOfFiles;

    #[ignore = "Needs `msixmgr`"]
    #[test]
    fn msixmgr_installs_random_package() {
        let _guard = prevent_concurrency("wine");
        //let (signing_key, _verifying_key) = SigningKey::generate("wolfpack".into()).unwrap();
        //let signer = PackageSigner::new(signing_key);
        let workdir = TempDir::new().unwrap();
        let package_file = workdir.path().join("test.msix");
        //let verifying_key_file = workdir.path().join("verifying-key");
        //verifying_key
        //    .write_armored(File::create(verifying_key_file.as_path()).unwrap())
        //    .unwrap();
        arbtest(|u| {
            let package: Package = u.arbitrary()?;
            let directory: DirectoryOfFiles = u.arbitrary()?;
            package
                .clone()
                .write(&package_file, directory.path())
                .unwrap();
            assert!(
                Command::new("wine")
                    .arg("msixmgr")
                    .arg("-AddPackage")
                    .arg(&package_file)
                    .status()
                    .unwrap()
                    .success(),
                "manifest:\n========{:?}========",
                package
            );
            Ok(())
        });
    }
}
