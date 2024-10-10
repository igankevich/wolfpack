use std::io::Read;
use std::path::Path;

use normalize_path::NormalizePath;

use crate::archive::ArchiveRead;
use crate::compress::AnyDecoder;
use crate::deb::ControlData;
use crate::deb::Error;
use crate::sign::Verifier;

pub(crate) struct BasicPackage;

impl BasicPackage {
    pub(crate) fn read_control<'a, R: 'a + Read, A: 'a + ArchiveRead<'a, R>, V: Verifier>(
        reader: R,
        verifier: &V,
    ) -> Result<ControlData, Error> {
        let mut reader = A::new(reader);
        let mut control: Option<Vec<u8>> = None;
        let mut message_parts: [Vec<u8>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        let mut signatures: Vec<Vec<u8>> = Vec::new();
        reader.find(|entry| {
            let path = entry.normalized_path()?;
            match path.to_str() {
                Some(DEBIAN_BINARY_FILE_NAME) => {
                    message_parts[0].clear();
                    entry.read_to_end(&mut message_parts[0])?;
                }
                Some(path) if path.starts_with("control.tar") => {
                    if control.is_some() {
                        return Err(std::io::Error::other("multiple `control.tar*` files"));
                    }
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf)?;
                    message_parts[1] = buf.clone();
                    control = Some(buf);
                }
                Some(path) if path.starts_with("data.tar") => {
                    message_parts[2].clear();
                    entry.read_to_end(&mut message_parts[2])?;
                }
                Some(path) if path.starts_with("_gpg") => {
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf)?;
                    signatures.push(buf);
                }
                _ => {}
            }
            Ok(None::<()>)
        })?;
        let control = control.ok_or_else(|| Error::MissingFile("control.tar*".into()))?;
        let message = message_parts
            .into_iter()
            .reduce(|mut m, part| {
                m.extend(part);
                m
            })
            .expect("array is not empty");
        if verifier
            .verify_any(&message[..], signatures.iter())
            .is_err()
        {
            return Err(Error::other("signature verification failed"));
        }
        let mut tar_archive = tar::Archive::new(AnyDecoder::new(&control[..]));
        for entry in tar_archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.normalize();
            if path == Path::new("control") {
                let mut buf = String::with_capacity(4096);
                entry.read_to_string(&mut buf)?;
                return buf.parse::<ControlData>();
            }
        }
        Err(Error::MissingFile("control.tar*".into()))
    }
}

pub const DEBIAN_BINARY_FILE_NAME: &str = "debian-binary";
pub const DEBIAN_BINARY: &str = "2.0\n";
