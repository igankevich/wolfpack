use std::collections::HashMap;
use std::ffi::CString;
use std::io::Error;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use flate2::write::GzEncoder;
use flate2::Compression;
use normalize_path::NormalizePath;
use walkdir::WalkDir;

//use zstd::stream::write::Encoder as ZstdEncoder;
use crate::archive::ArchiveWrite;
use crate::archive::CpioBuilder;
use crate::hash::Hasher;
use crate::hash::Sha256Hash;
use crate::rpm::pad;
use crate::rpm::Entry;
use crate::rpm::HashAlgorithm;
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
        // TODO + Seek
        W: Write,
        P: AsRef<Path>,
    {
        let lead = Lead::new(self.name.clone());
        eprintln!("write {lead:?}");
        lead.write(writer.by_ref())?;
        let mut basenames = Vec::<CString>::new();
        let mut dirnames = Vec::<CString>::new();
        let mut dirindices = Vec::<u32>::new();
        let mut usernames = Vec::<CString>::new();
        let mut groupnames = Vec::<CString>::new();
        let mut filedigests = Vec::<CString>::new();
        let mut filemodes = Vec::<u16>::new();
        let mut filesizes = Vec::<u32>::new();
        // TODO do not repeat walkdir in from_directory
        for entry in WalkDir::new(&directory).into_iter() {
            let entry = entry?;
            //let meta = entry.metadata()?;
            let path = entry.path();
            let entry_path = entry
                .path()
                .strip_prefix(&directory)
                .map_err(std::io::Error::other)?
                .normalize();
            if entry_path == Path::new("") {
                continue;
            }
            //let entry_path = Path::new(".").join(entry_path);
            let entry_path = Path::new("/tmp/rpm").join(entry_path);
            let meta = entry.metadata()?;
            if let (Some(file_name), Some(parent)) = (
                entry_path.file_name().and_then(|x| x.to_str()),
                entry_path.parent().and_then(|x| x.to_str()),
            ) {
                let parent = if parent.is_empty() {
                    parent.to_string()
                } else {
                    format!("{}/", parent)
                };
                let i = basenames.len();
                basenames.push(CString::new(file_name).unwrap());
                dirnames.push(CString::new(parent).unwrap());
                dirindices.push(i as u32);
                usernames.push(c"root".into());
                groupnames.push(c"root".into());
                filemodes.push(meta.mode() as u16);
                filesizes.push(meta.size() as u32);
                let hash = if path.is_dir() {
                    String::new()
                } else {
                    sha2::Sha256::compute(&std::fs::read(path)?).to_string()
                };
                filedigests.push(CString::new(hash).unwrap());
            }
        }
        let mut header2 = Header::new(self.into());
        header2.insert(Entry::BaseNames(basenames));
        header2.insert(Entry::DirNames(dirnames));
        header2.insert(Entry::DirIndexes(dirindices));
        header2.insert(Entry::FileUserName(usernames));
        header2.insert(Entry::FileGroupName(groupnames));
        header2.insert(Entry::FileDigestAlgo(HashAlgorithm::Sha256));
        header2.insert(Entry::FileDigests(filedigests));
        header2.insert(Entry::FileModes(filemodes));
        header2.insert(Entry::FileSizes(filesizes));
        let mut payload = Vec::new();
        CpioBuilder::from_directory(
            directory,
            GzEncoder::new(&mut payload, Compression::best()),
            // TODO
            //ZstdEncoder::new(&mut payload, COMPRESSION_LEVEL)?,
        )?
        .finish()?;
        let payload_sha256 = sha2::Sha256::compute(&payload);
        header2.insert(Entry::PayloadDigestAlgo(HashAlgorithm::Sha256));
        header2.insert(Entry::PayloadDigest(payload_sha256.clone()));
        header2.insert(Entry::PayloadDigestAlt(payload_sha256));
        let mut header2 = header2.to_vec()?;
        let header_sha256 = sha2::Sha256::compute(&header2);
        // sign second header without the leading padding
        let signature_v4 = signer
            .sign(&header2)
            .map_err(|_| Error::other("failed to sign rpm"))?;
        header2.extend(payload);
        // sign second header without the leading padding and the rest of the file
        let signature_v3 = signer
            .sign(&header2)
            .map_err(|_| Error::other("failed to sign rpm"))?;
        eprintln!("header2 len {}", header2.len());
        let header1 = Header::new(
            Signatures {
                signature_v3,
                signature_v4,
                header_sha256,
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
            Entry::Name(CString::new(other.name).unwrap()).into(),
            Entry::Version(CString::new(other.version).unwrap()).into(),
            Entry::Release(c"1".into()).into(),
            Entry::Summary(CString::new(other.summary).unwrap()).into(),
            Entry::Description(CString::new(other.description).unwrap()).into(),
            Entry::License(CString::new(other.license).unwrap()).into(),
            Entry::Url(CString::new(other.url).unwrap()).into(),
            Entry::Os(c"linux".into()).into(),
            Entry::Arch(CString::new(other.arch).unwrap()).into(),
            Entry::PayloadFormat(c"cpio".into()).into(),
            Entry::PayloadCompressor(c"gzip".into()).into(),
        ]
        .into()
    }
}

pub struct Signatures {
    pub signature_v3: Vec<u8>,
    pub signature_v4: Vec<u8>,
    pub header_sha256: Sha256Hash,
}

impl From<Signatures> for HashMap<SignatureTag, SignatureEntry> {
    fn from(other: Signatures) -> Self {
        [
            SignatureEntry::Gpg(other.signature_v3).into(),
            SignatureEntry::Dsa(other.signature_v4).into(),
            SignatureEntry::Sha256(other.header_sha256).into(),
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
            Command::new(RPMKEYS)
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
            let mut package: Package = u.arbitrary()?;
            package.arch = "x86_64".into();
            package.name = "test".into();
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
            //assert!(
            //    Command::new("cp")
            //        .arg(package_file.as_path())
            //        .arg("/src/test.rpm")
            //        .status()
            //        .unwrap()
            //        .success(),
            //    "manifest:\n========{:?}========",
            //    package
            //);
            assert!(
                Command::new(RPM)
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
                Command::new(RPM)
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
                Command::new(RPM)
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

    const RPM: &str = "/home/igankevich/workspace/etd/rpm/tmp/tools/rpm";
    const RPMKEYS: &str = "/home/igankevich/workspace/etd/rpm/tmp/tools/rpmkeys";
    //const RPM: &str = "rpm";
    //const RPMKEYS: &str = "rpmkeys";
}
