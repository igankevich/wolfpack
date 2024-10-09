use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

use crate::archive::ArchiveWrite;
use crate::deb::BasicPackage;
use crate::deb::DEBIAN_BINARY;
use crate::ipk::ControlData;
use crate::ipk::Error;
use crate::ipk::PackageSigner;
use crate::ipk::PackageVerifier;
use crate::sign::SignatureWriter;

pub struct Package;

impl Package {
    pub fn write<P1: AsRef<Path>, P2: Into<PathBuf>>(
        control_data: &ControlData,
        directory: P1,
        output_file: P2,
        signer: &PackageSigner,
    ) -> Result<(), std::io::Error> {
        let output_file: PathBuf = output_file.into();
        let writer = File::create(output_file.as_path())?;
        let writer = GzEncoder::new(writer, Compression::best());
        let mut signature_output_file = output_file;
        match signature_output_file.file_name() {
            Some(file_name) => {
                let mut file_name = file_name.to_os_string();
                file_name.push(".sig");
                signature_output_file.set_file_name(file_name);
            }
            None => signature_output_file.set_file_name("sig"),
        };
        let writer = SignatureWriter::new(writer, signer, signature_output_file);
        let data = tar::Builder::from_directory(directory, gz_writer())?.finish()?;
        let control =
            tar::Builder::from_files([("control", control_data.to_string())], gz_writer())?
                .finish()?;
        tar::Builder::from_files(
            [
                ("debian-binary", DEBIAN_BINARY.as_bytes()),
                ("control.tar.gz", &control),
                ("data.tar.gz", &data),
            ],
            writer,
        )?
        .write_signature()?;
        Ok(())
    }

    pub fn read_control<R: Read>(
        reader: R,
        verifier: &PackageVerifier,
    ) -> Result<ControlData, Error> {
        let gz = GzDecoder::new(reader);
        BasicPackage::read_control::<GzDecoder<R>, tar::Archive<GzDecoder<R>>, PackageVerifier>(
            gz, verifier,
        )
    }
}

fn gz_writer() -> GzEncoder<Vec<u8>> {
    GzEncoder::new(Vec::new(), Compression::best())
}

#[cfg(test)]
mod tests {

    use arbtest::arbtest;
    use tempfile::TempDir;

    use super::*;
    use crate::ipk::SigningKey;
    use crate::test::DirectoryOfFiles;

    // TODO BasicPackage is weak
    #[test]
    fn write_read() {
        let workdir = TempDir::new().unwrap();
        let signing_key = SigningKey::generate(Some("wolfpack".into()));
        let verifying_key = signing_key.to_verifying_key();
        arbtest(|u| {
            let control: ControlData = u.arbitrary()?;
            let directory: DirectoryOfFiles = u.arbitrary()?;
            let file_path = workdir.path().join("test.ipk");
            Package::write(
                &control,
                directory.path(),
                file_path.as_path(),
                &signing_key,
            )
            .unwrap();
            let actual =
                Package::read_control(File::open(file_path.as_path()).unwrap(), &verifying_key)
                    .unwrap();
            assert_eq!(control, actual);
            Ok(())
        });
    }
}
