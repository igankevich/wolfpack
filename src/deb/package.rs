use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use flate2::write::GzEncoder;
use flate2::Compression;
use walkdir::WalkDir;

use crate::deb::ControlData;
use crate::deb::Md5Reader;
use crate::deb::Md5Sums;

pub struct Package {
    control: ControlData,
    directory: PathBuf,
}

impl Package {
    pub fn new(control: ControlData, directory: PathBuf) -> Self {
        Self { control, directory }
    }

    pub fn build(&self, writer: impl Write) -> Result<(), std::io::Error> {
        let mut data = tar::Builder::new(GzEncoder::new(
            Vec::with_capacity(4096),
            Compression::default(),
        ));
        let mut control = tar::Builder::new(GzEncoder::new(
            Vec::with_capacity(4096),
            Compression::default(),
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
        tar_add_regular_file(&mut control, "control", self.control.to_string())?;
        tar_add_regular_file(&mut control, "md5sums", md5sums.as_bytes())?;
        let control = control.into_inner()?.finish()?;
        let data = data.into_inner()?.finish()?;
        let mut package = ar::Builder::new(writer);
        ar_add_regular_file(&mut package, "debian-binary", "2.0")?;
        ar_add_regular_file(&mut package, "control.tar.gz", control)?;
        ar_add_regular_file(&mut package, "data.tar.gz", data)?;
        package.into_inner()?;
        Ok(())
    }
}

fn tar_add_regular_file<W: Write, P: AsRef<Path>, C: AsRef<[u8]>>(
    archive: &mut tar::Builder<W>,
    path: P,
    contents: C,
) -> Result<(), std::io::Error> {
    let contents = contents.as_ref();
    let mut header = tar::Header::new_old();
    header.set_size(contents.len() as u64);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mode(0o644);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_path(path)?;
    header.set_cksum();
    archive.append(&header, contents)?;
    Ok(())
}

fn ar_add_regular_file<W: Write, P: AsRef<[u8]>, C: AsRef<[u8]>>(
    archive: &mut ar::Builder<W>,
    file_name: P,
    contents: C,
) -> Result<(), std::io::Error> {
    let contents = contents.as_ref();
    let mut header = ar::Header::new(file_name.as_ref().into(), contents.len() as u64);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mode(0o644);
    archive.append(&header, contents)?;
    Ok(())
}
