use std::io::Write;
use std::path::Path;

pub trait ArchiveWrite<W: Write> {
    fn new(writer: W) -> Self;

    fn add_regular_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
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
            path.as_ref().as_os_str().as_encoded_bytes().to_vec(),
            contents.len() as u64,
        );
        header.set_uid(0);
        header.set_gid(0);
        header.set_mode(0o644);
        self.append(&header, contents)?;
        Ok(())
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
        header.set_cksum();
        self.append(&header, contents)?;
        Ok(())
    }

    fn into_inner(self) -> Result<W, std::io::Error> {
        tar::Builder::into_inner(self)
    }
}
