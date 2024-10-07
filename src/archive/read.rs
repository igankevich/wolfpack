use std::io::Error;
use std::io::Read;
use std::path::PathBuf;

pub trait ArchiveRead<'a, R: 'a + Read> {
    fn new(reader: R) -> Self;
    fn find<F, E>(&mut self, f: F) -> Result<Option<E>, Error>
    where
        F: FnMut(&mut dyn ArchiveEntry) -> Result<Option<E>, Error>;
}

pub trait ArchiveEntry: Read {
    fn normalized_path(&self) -> Result<PathBuf, Error>;
}
