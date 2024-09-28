use std::io::Read;

pub trait ArchiveRead<R: Read> {
    fn new(reader: R) -> Self;
}

impl<R: Read> ArchiveRead<R> for ar::Archive<R> {
    fn new(reader: R) -> Self {
        Self::new(reader)
    }
}
