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
    use std::fs::File;
    use std::fs::remove_dir_all;
    use std::process::Command;
    use std::process::Stdio;
    use std::time::Duration;

    use arbtest::arbtest;
    use tempfile::TempDir;

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

    #[ignore]
    #[test]
    fn dpkg_installs_random_packages() {
        let workdir = TempDir::new().unwrap();
        arbtest(|u| {
            let mut control: ControlData = u.arbitrary()?;
            control.architecture = "all".parse().unwrap();
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let path = workdir.path().join("test.deb");
            let root = workdir.path().join("root");
            let _ = remove_dir_all(root.as_path());
            eprint!("{}", control);
            Package::write(
                &control,
                directory.path(),
                File::create(path.as_path()).unwrap(),
            )
            .unwrap();
            assert!(Command::new("dpkg")
                .arg("--root")
                .arg(root.as_path())
                .arg("--install")
                .arg(path.as_path())
                .status()
                .unwrap()
                .success(), "control = {:?}", control);
            assert!(Command::new("dpkg-query")
                .arg("--root")
                .arg(root.as_path())
                .arg("-L")
                .arg(control.name().as_str())
                .stdout(Stdio::null())
                .status()
                .unwrap()
                .success());
            Ok(())
        })
        .budget(Duration::from_secs(10));
    }
}
