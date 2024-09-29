use std::fs::Metadata;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub trait ArchiveWrite<W: Write> {
    fn new(writer: W) -> Self;

    fn add_regular_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        contents: C,
    ) -> Result<(), std::io::Error>;

    fn add_regular_file_with_metadata<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        metadata: &Metadata,
        contents: C,
    ) -> Result<(), std::io::Error>;

    fn into_inner(self) -> Result<W, std::io::Error>;
}

impl<W: Write> ArchiveWrite<W> for ar::Builder<W> {
    fn new(writer: W) -> Self {
        Self::new(writer)
    }

    fn add_regular_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        contents: C,
    ) -> Result<(), std::io::Error> {
        let contents = contents.as_ref();
        let mut header = ar::Header::new(
            path.as_ref().as_os_str().as_bytes().to_vec(),
            contents.len() as u64,
        );
        header.set_uid(0);
        header.set_gid(0);
        header.set_mode(0o644);
        self.append(&header, contents)?;
        Ok(())
    }

    fn add_regular_file_with_metadata<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        _metadata: &Metadata,
        contents: C,
    ) -> Result<(), std::io::Error> {
        self.add_regular_file(path, contents)
    }

    fn into_inner(self) -> Result<W, std::io::Error> {
        ar::Builder::into_inner(self)
    }
}

impl<W: Write> ArchiveWrite<W> for tar::Builder<W> {
    fn new(writer: W) -> Self {
        Self::new(writer)
    }

    fn add_regular_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
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
        // TODO this has to be done for ipk only
        let actual_path = &mut header.as_old_mut().name;
        let n = actual_path.len();
        actual_path.copy_within(..(n - 2), 2);
        actual_path[0] = b'.';
        actual_path[1] = b'/';
        header.set_cksum();
        self.append(&header, contents)?;
        Ok(())
    }

    fn add_regular_file_with_metadata<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        metadata: &Metadata,
        contents: C,
    ) -> Result<(), std::io::Error> {
        let contents = contents.as_ref();
        let mut header = tar::Header::new_old();
        header.set_metadata(metadata);
        header.set_size(contents.len() as u64);
        header.set_uid(0);
        header.set_gid(0);
        header.set_path(path)?;
        // TODO this has to be done for pkg only
        let actual_path = &mut header.as_old_mut().name;
        let n = actual_path.len();
        actual_path.copy_within(..(n - 1), 1);
        actual_path[0] = b'/';
        header.set_cksum();
        self.append(&header, contents)?;
        Ok(())
    }

    fn into_inner(self) -> Result<W, std::io::Error> {
        tar::Builder::into_inner(self)
    }
}
