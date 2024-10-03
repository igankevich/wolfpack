use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use normalize_path::NormalizePath;
use walkdir::WalkDir;
use xz::read::XzDecoder;

use crate::archive::ArchiveWrite;
use crate::deb::ControlData;
use crate::deb::Error;
use crate::deb::Md5Sums;
use crate::hash::Md5Reader;

pub(crate) struct BasicPackage;

impl BasicPackage {
    pub(crate) fn write<W: Write, A: ArchiveWrite<W>, P: AsRef<Path>>(
        control_data: &ControlData,
        directory: P,
        writer: W,
    ) -> Result<(), std::io::Error> {
        let directory = directory.as_ref();
        let mut data = tar::Builder::new(GzEncoder::new(
            Vec::with_capacity(4096),
            Compression::best(),
        ));
        let mut control = tar::Builder::new(GzEncoder::new(
            Vec::with_capacity(4096),
            Compression::best(),
        ));
        let mut md5sums = Md5Sums::new();
        for entry in WalkDir::new(directory).into_iter() {
            let entry = entry?;
            let relative_path = Path::new(".").join(
                entry
                    .path()
                    .strip_prefix(directory)
                    .map_err(std::io::Error::other)?
                    .normalize(),
            );
            let mut header = tar::Header::new_old();
            header.set_metadata(&std::fs::metadata(entry.path())?);
            header.set_path(relative_path.as_path())?;
            header.set_uid(0);
            header.set_gid(0);
            header.set_cksum();
            if entry.file_type().is_dir() {
                data.append::<&[u8]>(&header, &[])?;
            } else {
                let mut reader = Md5Reader::new(File::open(entry.path())?);
                data.append(&header, &mut reader)?;
                md5sums.insert(relative_path, reader.digest()?.0);
            }
        }
        let data = data.into_inner()?.finish()?;
        control.add_regular_file("control", control_data.to_string())?;
        control.add_regular_file("md5sums", md5sums.to_string())?;
        let control = control.into_inner()?.finish()?;
        let mut package = A::new(writer);
        package.add_regular_file("debian-binary", "2.0\n")?;
        package.add_regular_file("control.tar.gz", control)?;
        package.add_regular_file("data.tar.gz", data)?;
        package.into_inner()?;
        Ok(())
    }

    pub(crate) fn read_control<R: Read>(reader: R) -> Result<ControlData, Error> {
        let mut reader = ar::Archive::new(reader);
        while let Some(entry) = reader.next_entry() {
            let entry = entry?;
            let path = PathBuf::from(OsString::from_vec(entry.header().identifier().to_vec()));
            let path = path.normalize();
            let decoder: Box<dyn Read> = match path.to_str() {
                Some("control.tar.gz") => Box::new(GzDecoder::new(entry)),
                Some("control.tar.xz") => Box::new(XzDecoder::new(entry)),
                _ => continue,
            };
            let mut tar_archive = tar::Archive::new(decoder);
            for entry in tar_archive.entries()? {
                let mut entry = entry?;
                let path = entry.path()?.normalize();
                if path == Path::new("control") {
                    let mut buf = String::with_capacity(4096);
                    entry.read_to_string(&mut buf)?;
                    return buf.parse();
                }
            }
        }
        Err(Error::MissingFile("control.tar.(gz|xz)".into()))
    }
}
