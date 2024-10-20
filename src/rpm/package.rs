use std::collections::HashMap;
use std::io::Error;
use std::io::Write;
use std::path::Path;

use flate2::write::GzEncoder;
use flate2::Compression;
use walkdir::WalkDir;

//use zstd::stream::write::Encoder as ZstdEncoder;
use crate::archive::ArchiveWrite;
use crate::archive::CpioBuilder;
use crate::rpm::pad;
use crate::rpm::Entry;
use crate::rpm::Header;
use crate::rpm::Lead;
use crate::rpm::PackageSigner;
use crate::rpm::SignatureEntry;
use crate::rpm::SignatureTag;
use crate::rpm::Tag;
use crate::rpm::ALIGN;

#[derive(Debug)]
#[cfg_attr(test, derive(arbitrary::Arbitrary, PartialEq, Eq, Clone))]
pub struct Package {
    pub name: String,
    pub version: String,
    pub summary: String,
    pub description: String,
    pub license: String,
    pub url: String,
    pub arch: String,
}

impl Package {
    pub fn write<W, P>(
        self,
        mut writer: W,
        directory: P,
        signer: &PackageSigner,
    ) -> Result<(), Error>
    where
        W: Write,
        P: AsRef<Path>,
    {
        let lead = Lead::new(self.name.clone());
        eprintln!("write {lead:?}");
        lead.write(writer.by_ref())?;
        let mut basenames = Vec::<String>::new();
        let mut dirnames = Vec::<String>::new();
        let mut dirindices = Vec::<u32>::new();
        // TODO do not repeat walkdir in from_directory
        for entry in WalkDir::new(&directory).into_iter() {
            let entry = entry?;
            let path = entry.path();
            if let (Some(file_name), Some(parent)) = (
                path.file_name().and_then(|x| x.to_str()),
                path.parent().and_then(|x| x.to_str()),
            ) {
                let i = basenames.len();
                basenames.push(file_name.into());
                dirnames.push(parent.into());
                dirindices.push(i as u32);
            }
        }
        let mut header2 = Header::new(self.into());
        header2.insert(Entry::BaseNames(basenames));
        header2.insert(Entry::DirNames(dirnames));
        header2.insert(Entry::DirIndexes(dirindices));
        let mut header2 = header2.to_vec()?;
        // sign second header without the leading padding
        let signature_v4 = signer
            .sign(&header2)
            .map_err(|_| Error::other("failed to sign rpm"))?;

        CpioBuilder::from_directory(
            directory,
            GzEncoder::new(&mut header2, Compression::best()),
            // TODO
            //ZstdEncoder::new(&mut header2, COMPRESSION_LEVEL)?,
        )?
        .finish()?;
        // sign second header without the leading padding and the rest of the file
        let signature_v3 = signer
            .sign(&header2)
            .map_err(|_| Error::other("failed to sign rpm"))?;
        let header1 = Header::new(
            Signatures {
                signature_v3,
                signature_v4,
            }
            .into(),
        );
        let header1 = header1.to_vec()?;
        writer.write_all(&header1)?;
        let padding = pad(header1.len() as u32, ALIGN);
        assert_eq!(0, (header1.len() as u32 + padding) % ALIGN);
        if padding != 0 {
            writer.write_all(&vec![0_u8; padding as usize])?;
        }
        writer.write_all(&header2)?;
        Ok(())
    }
}

impl From<Package> for HashMap<Tag, Entry> {
    fn from(other: Package) -> Self {
        [
            Entry::Name(other.name).into(),
            Entry::Version(other.version).into(),
            Entry::Summary(other.summary).into(),
            Entry::Description(other.description).into(),
            Entry::License(other.license).into(),
            Entry::Url(other.url).into(),
            Entry::Arch(other.arch).into(),
            Entry::PayloadFormat("cpio".into()).into(),
            Entry::PayloadCompressor("gzip".into()).into(),
        ]
        .into()
    }
}

pub struct Signatures {
    pub signature_v3: Vec<u8>,
    pub signature_v4: Vec<u8>,
}

impl From<Signatures> for HashMap<SignatureTag, SignatureEntry> {
    fn from(other: Signatures) -> Self {
        [
            SignatureEntry::Gpg(other.signature_v3).into(),
            SignatureEntry::Dsa(other.signature_v4).into(),
        ]
        .into()
    }
}

const COMPRESSION_LEVEL: i32 = 22;

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::process::Command;
    use std::time::Duration;

    use arbtest::arbtest;
    use tempfile::TempDir;

    use super::*;
    use crate::rpm::SigningKey;
    use crate::test::prevent_concurrency;
    use crate::test::DirectoryOfFiles;

    /*
    #[test]
    fn package_write_read() {
        let (signing_key, _verifying_key) = SigningKey::generate("wolfpack".into()).unwrap();
        let signer = PackageSigner::new(signing_key);
        arbtest(|u| {
            let expected: Package = u.arbitrary()?;
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let mut buf = Vec::new();
            expected.clone()
                .write(&mut buf, directory.path(), &signer)
                .unwrap();
            let actual = Lead::read(&buf).unwrap();
            assert_eq!(expected, actual);
            Ok(())
        });
    }
    */

    #[ignore]
    #[test]
    fn rpm_installs_random_package() {
        let _guard = prevent_concurrency("rpm");
        let (signing_key, verifying_key) = SigningKey::generate("wolfpack".into()).unwrap();
        let signer = PackageSigner::new(signing_key);
        let workdir = TempDir::new().unwrap();
        let package_file = workdir.path().join("test.rpm");
        let verifying_key_file = workdir.path().join("verifying-key");
        verifying_key
            .write_armored(File::create(verifying_key_file.as_path()).unwrap())
            .unwrap();
        let mut verifying_key_vec = Vec::new();
        verifying_key.write_armored(&mut verifying_key_vec).unwrap();
        let verifying_key_str = String::from_utf8(verifying_key_vec).unwrap();
        assert!(
            Command::new("rpm")
                .arg("--verbose")
                .arg("--import")
                .arg(verifying_key_file.as_path())
                .status()
                .unwrap()
                .success(),
            "verifying key:\n{}",
            verifying_key_str
        );
        eprintln!("added public key");
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
            //assert!(
            //    Command::new("xxd")
            //        .arg("-d")
            //        .arg("-l")
            //        .arg("200")
            //        .arg(package_file.as_path())
            //        .status()
            //        .unwrap()
            //        .success(),
            //);
            //assert!(
            //    Command::new("dnf")
            //        .arg("--verbose")
            //        .arg("--disablerepo=*")
            //        .arg("install")
            //        .arg("-y")
            //        .arg(package_file.as_path())
            //        .status()
            //        .unwrap()
            //        .success(),
            //    "manifest:\n========{:?}========",
            //    package
            //);
            assert!(
                Command::new("cp")
                    .arg(package_file.as_path())
                    .arg("/src/test.rpm")
                    .status()
                    .unwrap()
                    .success(),
                "manifest:\n========{:?}========",
                package
            );
            assert!(
                Command::new("rpm")
                    .arg("--verbose")
                    .arg("--query")
                    .arg("--dump")
                    .arg(package_file.as_path())
                    .status()
                    .unwrap()
                    .success(),
                "manifest:\n========{:?}========",
                package
            );
            assert!(
                Command::new("rpm")
                    .arg("--debug")
                    .arg("--verbose")
                    .arg("--install")
                    .arg(package_file.as_path())
                    .status()
                    .unwrap()
                    .success(),
                "manifest:\n========{:?}========",
                package
            );
            assert!(
                Command::new("rpm")
                    .arg("--verbose")
                    .arg("--erase")
                    .arg(&package.name)
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
