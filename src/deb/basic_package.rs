use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use normalize_path::NormalizePath;
use walkdir::WalkDir;
use xz::read::XzDecoder;

use crate::archive::ArchiveRead;
use crate::archive::ArchiveWrite;
use crate::deb::ControlData;
use crate::deb::Error;
use crate::deb::Md5Reader;
use crate::deb::Md5Sums;

pub(crate) struct BasicPackage {
    pub(crate) control: ControlData,
    pub(crate) directory: PathBuf,
}

impl BasicPackage {
    pub(crate) fn build<W1: Write, A1: ArchiveWrite<W1>>(
        &self,
        writer: W1,
    ) -> Result<(), std::io::Error> {
        let mut data = tar::Builder::new(GzEncoder::new(
            Vec::with_capacity(4096),
            Compression::best(),
        ));
        let mut control = tar::Builder::new(GzEncoder::new(
            Vec::with_capacity(4096),
            Compression::best(),
        ));
        let mut md5sums = Md5Sums::new();
        for entry in WalkDir::new(self.directory.as_path()).into_iter() {
            let entry = entry?;
            let relative_path = entry
                .path()
                .strip_prefix(self.directory.as_path())
                .map_err(std::io::Error::other)?;
            if !(entry.file_type().is_file() || entry.file_type().is_symlink()) {
                continue;
            }
            let mut header = tar::Header::new_old();
            header.set_metadata(&std::fs::metadata(entry.path())?);
            header.set_path(relative_path)?;
            header.set_uid(0);
            header.set_gid(0);
            header.set_mtime(0);
            header.set_cksum();
            let mut reader = Md5Reader::new(File::open(entry.path())?);
            data.append(&header, &mut reader)?;
            md5sums.append_file(relative_path, reader.digest());
        }
        let data = data.into_inner()?.finish()?;
        control.add_regular_file("control", self.control.to_string())?;
        control.add_regular_file("md5sums", md5sums.as_bytes())?;
        let control = control.into_inner()?.finish()?;
        let mut package = A1::new(writer);
        package.add_regular_file("debian-binary", "2.0")?;
        package.add_regular_file("control.tar.gz", control)?;
        package.add_regular_file("data.tar.gz", data)?;
        package.into_inner()?;
        Ok(())
    }

    pub(crate) fn read_control<R1: Read, R2: ArchiveRead<R1>>(
        reader: R1,
    ) -> Result<ControlData, Error> {
        let mut reader = ar::Archive::new(reader);
        while let Some(entry) = reader.next_entry() {
            let entry = entry?;
            let file_name = String::from_utf8_lossy(entry.header().identifier());
            eprintln!(
                "file {} {:?}",
                file_name.as_ref(),
                file_name.as_ref() == "control.tar.xz"
            );
            let decoder: Box<dyn Read> = match file_name.as_ref() {
                "control.tar.gz" => Box::new(GzDecoder::new(entry)),
                "control.tar.xz" => {
                    eprintln!("xz");
                    Box::new(XzDecoder::new(entry))
                }
                _ => continue,
            };
            let mut tar_archive = tar::Archive::new(decoder);
            for entry in tar_archive.entries()? {
                let mut entry = entry?;
                let path = entry.path()?.normalize();
                eprintln!("tar path {}", path.display());
                if path == Path::new("control") {
                    let mut buf = String::with_capacity(4096);
                    entry.read_to_string(&mut buf)?;
                    return Ok(buf.parse()?);
                }
            }
        }
        Err(Error::MissingFile("control.tar.(gz|xz)".into()))
    }
}
