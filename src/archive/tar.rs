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

/// This builder retains original paths in the tar file.
///
/// This is in contrast to the default builder that removes leading `/` and `./`.
/// FreeBSD `pkg` needs leading `/` and `ustar` tar archive format.
pub struct TarBuilder<W: Write> {
    inner: tar::Builder<W>,
}

impl<W: Write> ArchiveWrite<W> for TarBuilder<W> {
    fn new(writer: W) -> Self {
        Self {
            inner: tar::Builder::new(writer),
        }
    }

    fn add_regular_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        contents: C,
    ) -> Result<(), Error> {
        let path = path.as_ref().normalize();
        let relative_path = match path.strip_prefix("/") {
            Ok(relative_path) => relative_path,
            Err(_) => &path,
        };
        let contents = contents.as_ref();
        let mut header = tar::Header::new_ustar();
        header.set_size(contents.len() as u64);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_path(relative_path)?;
        let actual_path = &mut header.as_old_mut().name;
        fix_path(&mut actual_path[..], &path)?;
        header.set_cksum();
        self.inner.append(&header, contents)?;
        Ok(())
    }

    fn add_regular_file_with_metadata<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        metadata: &Metadata,
        contents: C,
    ) -> Result<(), Error> {
        let path = path.as_ref().normalize();
        let relative_path = match path.strip_prefix("/") {
            Ok(relative_path) => relative_path,
            Err(_) => &path,
        };
        let contents = contents.as_ref();
        let mut header = tar::Header::new_ustar();
        header.set_metadata(metadata);
        header.set_size(contents.len() as u64);
        header.set_uid(0);
        header.set_gid(0);
        header.set_path(relative_path)?;
        let actual_path = &mut header.as_old_mut().name;
        fix_path(&mut actual_path[..], &path)?;
        header.set_cksum();
        self.inner.append(&header, contents)?;
        Ok(())
    }

    fn into_inner(self) -> Result<W, Error> {
        self.inner.into_inner()
    }
}

fn fix_path(actual_path: &mut [u8], original_path: &Path) -> Result<(), Error> {
    let n = actual_path.len();
    if original_path.as_os_str().len() > n {
        return Err(Error::other(format!(
            "path is too long: {}",
            original_path.display()
        )));
    }
    if original_path.starts_with("./") {
        actual_path.copy_within(..(n - 2), 2);
        actual_path[0] = b'.';
        actual_path[1] = b'/';
    } else if original_path.starts_with("/") {
        actual_path.copy_within(..(n - 1), 1);
        actual_path[0] = b'/';
    }
    Ok(())
}

impl<W: Write> ArchiveWrite<W> for tar::Builder<W> {
    fn new(writer: W) -> Self {
        Self::new(writer)
    }

    fn add_regular_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        contents: C,
    ) -> Result<(), Error> {
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
    ) -> Result<(), Error> {
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

    fn into_inner(self) -> Result<W, Error> {
        tar::Builder::into_inner(self)
    }
}

impl<'a, R: 'a + Read> ArchiveRead<'a, R> for tar::Archive<R> {
    fn new(reader: R) -> Self {
        tar::Archive::<R>::new(reader)
    }

    fn find<F, E>(&mut self, mut f: F) -> Result<Option<E>, Error>
    where
        F: FnMut(&mut dyn ArchiveEntry) -> Result<Option<E>, Error>,
    {
        for entry in self.entries()? {
            if let Some(ret) = f(&mut entry?)? {
                return Ok(Some(ret));
            }
        }
        Ok(None)
    }
}

impl<'a, R: Read> ArchiveEntry for tar::Entry<'a, R> {
    fn normalized_path(&self) -> Result<PathBuf, Error> {
        Ok(self.path()?.normalize())
    }
}
