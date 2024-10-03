use std::io::Read;
use std::io::Write;
use std::path::Path;

use crate::deb::BasicPackage;
use crate::deb::ControlData;
use crate::deb::Error;

pub struct Package;

impl Package {
    pub fn write<W: Write, P: AsRef<Path>>(
        control_data: &ControlData,
        directory: P,
        writer: W,
    ) -> Result<(), std::io::Error> {
        BasicPackage::write::<W, ar::Builder<W>, P>(control_data, directory, writer)
    }

    pub fn read_control<R: Read>(reader: R) -> Result<ControlData, Error> {
        BasicPackage::read_control::<R, ar::Archive<R>>(reader)
    }
}

#[cfg(test)]
mod tests {
    use arbtest::arbtest;

    use super::*;
    use crate::test::DirectoryOfFiles;

    #[test]
    fn write_read() {
        arbtest(|u| {
            let control: ControlData = u.arbitrary()?;
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let mut buf: Vec<u8> = Vec::new();
            Package::write(&control, directory.path(), &mut buf).unwrap();
            let actual = Package::read_control(&buf[..]).unwrap();
            assert_eq!(control, actual);
            Ok(())
        });
    }
}
