use std::collections::HashSet;
use std::io::Error;
use std::io::Write;
use std::path::Path;

use crate::rpm::Entry;

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub summary: String,
    pub description: String,
    pub license: String,
    pub url: String,
    pub arch: String,
}

impl Package {
    pub fn write<W, P>(&self, writer: &mut W, directory: P) -> Result<(), Error>
    where
        W: Write,
        P: AsRef<Path>,
    {
        let signature_header = Header::new(HashSet::<SignatureEntry>::new());
        signature_header.write(writer)?;
        let header = Header::new(self.clone().into());
        header.write(writer)?;
        // TODO impl ArchiveWrite for CpioBuilder
        Ok(())
    }
}

impl From<Package> for HashSet<Entry> {
    fn from(other: Package) -> Self {
        [
            Entry::Name(other.name),
            Entry::Version(other.version),
            Entry::Summary(other.summary),
            Entry::Description(other.description),
            Entry::License(other.license),
            Entry::Url(other.url),
            Entry::Arch(other.arch),
        ]
        .into()
    }
}
