use std::io::Write;
use std::path::PathBuf;

use flate2::write::GzEncoder;
use flate2::Compression;

use crate::deb::BasicPackage;
use crate::deb::ControlData;

pub struct Package {
    inner: BasicPackage,
}

impl Package {
    pub fn new(control: ControlData, directory: PathBuf) -> Self {
        Self {
            inner: BasicPackage { control, directory },
        }
    }

    pub fn build<W: Write>(&self, writer: W) -> Result<(), std::io::Error> {
        let gz = GzEncoder::new(writer, Compression::best());
        self.inner
            .build::<GzEncoder<W>, tar::Builder<GzEncoder<W>>>(gz)
    }
}
