use std::fs::Metadata;
use std::io::Error;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use cpio::newc::trailer;
use cpio::newc::ModeFileType;
use cpio::NewcBuilder as Entry;
use normalize_path::NormalizePath;

use crate::archive::ArchiveWrite;

pub struct CpioBuilder<W: Write> {
    writer: W,
    ino: u32,
}

impl<W: Write> ArchiveWrite<W> for CpioBuilder<W> {
    fn new(writer: W) -> Self {
        Self { writer, ino: 0 }
    }

    fn add_regular_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        contents: C,
    ) -> Result<(), Error> {
        let path = path.as_ref().normalize();
        let path = Path::new("/tmp/rpm").join(path);
        let contents = contents.as_ref();
        if contents.len() > u32::MAX as usize {
            return Err(Error::other(format!(
                "file is too large: {}",
                path.display()
            )));
        }
        eprintln!("cpio add {:?}", path.to_str().unwrap());
        let mut entry_writer = Entry::new(
            path.to_str()
                .ok_or_else(|| Error::other(format!("non utf-8 path: {}", path.display())))?,
        )
        .mode(0o644)
        .set_mode_file_type(ModeFileType::Regular)
        .ino(self.ino)
        .write(&mut self.writer, contents.len() as u32);
        entry_writer.write_all(contents)?;
        let _ = entry_writer.finish();
        self.ino += 1;
        Ok(())
    }

    fn add_regular_file_with_metadata<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        meta: &Metadata,
        contents: C,
    ) -> Result<(), Error> {
        let path = path.as_ref().normalize();
        let path = Path::new("/tmp/rpm").join(path);
        let contents = contents.as_ref();
        if contents.len() > u32::MAX as usize {
            return Err(Error::other(format!(
                "file is too large: {}",
                path.display()
            )));
        }
        eprintln!("cpio add {:?}", path.to_str().unwrap());
        let mut entry_writer = Entry::new(
            path.to_str()
                .ok_or_else(|| Error::other(format!("non utf-8 path: {}", path.display())))?,
        )
        .mode(meta.mode())
        .set_mode_file_type(metadata_to_file_type(meta)?)
        .uid(meta.uid())
        .gid(meta.gid())
        .mtime(meta.mtime() as u32)
        .ino(self.ino)
        .write(&mut self.writer, contents.len() as u32);
        entry_writer.write_all(contents)?;
        let _ = entry_writer.finish();
        self.ino += 1;
        Ok(())
    }

    fn into_inner(self) -> Result<W, Error> {
        trailer(self.writer)
    }
}

fn metadata_to_file_type(metadata: &Metadata) -> Result<ModeFileType, Error> {
    if metadata.is_file() {
        Ok(ModeFileType::Regular)
    } else if metadata.is_dir() {
        Ok(ModeFileType::Directory)
    } else if metadata.is_symlink() {
        Ok(ModeFileType::Symlink)
    } else {
        Err(Error::other(format!(
            "unsupported file type: {:?}",
            metadata.file_type()
        )))
    }
}
