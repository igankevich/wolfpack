use std::io::Read;
use std::io::Write;
use std::path::PathBuf;

use crate::deb::BasicPackage;
use crate::deb::ControlData;
use crate::deb::Error;

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
        self.inner.build::<W, ar::Builder<W>>(writer)
    }

    pub fn read_control<R: Read>(reader: R) -> Result<ControlData, Error> {
        BasicPackage::read_control::<R>(reader)
    }
}
