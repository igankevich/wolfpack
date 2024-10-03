use std::io::Write;
use std::path::Path;

use flate2::write::GzEncoder;
use flate2::Compression;

use crate::deb::BasicPackage;
use crate::deb::ControlData;

pub struct Package;

impl Package {
    pub fn write<W: Write, P: AsRef<Path>>(
        control_data: &ControlData,
        directory: P,
        writer: W,
    ) -> Result<(), std::io::Error> {
        let gz = GzEncoder::new(writer, Compression::best());
        BasicPackage::write::<GzEncoder<W>, tar::Builder<GzEncoder<W>>, P>(
            control_data,
            directory,
            gz,
        )
    }
}
