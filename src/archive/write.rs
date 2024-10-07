use std::fs::Metadata;
use std::io::Error;
use std::io::Write;
use std::path::Path;

pub trait ArchiveWrite<W: Write> {
    fn new(writer: W) -> Self;

    fn add_regular_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        contents: C,
    ) -> Result<(), Error>;

    fn add_regular_file_with_metadata<P: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        path: P,
        metadata: &Metadata,
        contents: C,
    ) -> Result<(), Error>;

    fn into_inner(self) -> Result<W, Error>;
}
