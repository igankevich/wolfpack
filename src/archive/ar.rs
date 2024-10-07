use std::fs::Metadata;
use std::io::Error;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use normalize_path::NormalizePath;

use crate::archive::ArchiveEntry;
use crate::archive::ArchiveRead;
use crate::archive::ArchiveWrite;

impl<W: Write> ArchiveWrite<W> for ar::Builder<W> {
    fn new(writer: W) -> Self {
        Self::new(writer)
    }

    fn add_regular_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        contents: C,
    ) -> Result<(), Error> {
        let contents = contents.as_ref();
        let mut header = ar::Header::new(path_to_bytes(path.as_ref()), contents.len() as u64);
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
    ) -> Result<(), Error> {
        self.add_regular_file(path, contents)
    }

    fn into_inner(self) -> Result<W, Error> {
        ar::Builder::into_inner(self)
    }
}

impl<'a, R: 'a + Read> ArchiveRead<'a, R> for ar::Archive<R> {
    fn new(reader: R) -> Self {
        ar::Archive::<R>::new(reader)
    }

    fn find<F, E>(&mut self, mut f: F) -> Result<Option<E>, Error>
    where
        F: FnMut(&mut dyn ArchiveEntry) -> Result<Option<E>, Error>,
    {
        while let Some(entry) = self.next_entry() {
            if let Some(ret) = f(&mut entry?)? {
                return Ok(Some(ret));
            }
        }
        Ok(None)
    }
}

impl<'a, R: Read> ArchiveEntry for ar::Entry<'a, R> {
    #[cfg(unix)]
    fn normalized_path(&self) -> Result<PathBuf, Error> {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let path = Path::new(OsStr::from_bytes(self.header().identifier()));
        Ok(path.normalize())
    }

    #[cfg(not(unix))]
    fn normalized_path(&self) -> Result<PathBuf, Error> {
        let cow = String::from_utf8_lossy(self.header().identifier());
        let path = Path::new(cow.as_ref());
        Ok(path.normalize())
    }
}

#[cfg(unix)]
fn path_to_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_to_bytes(path: &Path) -> Vec<u8> {
    use std::borrow::Cow;
    match path.to_string_lossy() {
        Cow::Borrowed(s) => s.to_string(),
        Cow::Owned(s) => s,
    }
    .into()
}
