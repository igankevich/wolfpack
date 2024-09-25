use std::io::Write;
use std::path::PathBuf;

use flate2::write::GzEncoder;
use flate2::Compression;
use walkdir::WalkDir;

use crate::deb::ControlData;
//use crate::deb::Error;

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
        let mut md5sums = String::with_capacity(4096);
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
            let contents = std::fs::read(entry.path())?;
            data.append(&header, contents.as_slice())?;
            let md5_hash = md5::compute(&contents);
            use std::fmt::Write;
            let _ = writeln!(&mut md5sums, "{:x}  {}", md5_hash, relative_path.display());
        }
        {
            let control_data = self.control.to_string();
            let mut header = tar::Header::new_old();
            header.set_size(control_data.as_bytes().len() as u64);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_path("control")?;
            header.set_cksum();
            control.append(&header, control_data.as_bytes())?;
        }
        {
            let mut header = tar::Header::new_old();
            header.set_size(md5sums.as_bytes().len() as u64);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_path("md5sums")?;
            header.set_cksum();
            control.append(&header, md5sums.as_bytes())?;
        }
        let control_contents = control.into_inner()?.finish()?;
        let data_contents = data.into_inner()?.finish()?;
        let mut package = ar::Builder::new(writer);
        {
            let file_name = "debian-binary";
            let contents = "2.0";
            let mut header = ar::Header::new(file_name.into(), contents.as_bytes().len() as u64);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mode(0o644);
            package.append(&header, contents.as_bytes())?;
        }
        {
            let file_name = "control.tar.gz";
            let mut header = ar::Header::new(file_name.into(), control_contents.len() as u64);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mode(0o644);
            package.append(&header, control_contents.as_slice())?;
        }
        {
            let file_name = "data.tar.gz";
            let mut header = ar::Header::new(file_name.into(), data_contents.len() as u64);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mode(0o644);
            package.append(&header, data_contents.as_slice())?;
        }
        package.into_inner()?;
        Ok(())
    }
}
