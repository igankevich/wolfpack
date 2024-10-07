use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use flate2::write::GzEncoder;
use flate2::Compression;
use normalize_path::NormalizePath;
use walkdir::WalkDir;

use crate::archive::ArchiveRead;
use crate::archive::ArchiveWrite;
use crate::compress::AnyDecoder;
use crate::deb::ControlData;
use crate::deb::Error;
use crate::sign::Signer;
use crate::sign::Verifier;

pub(crate) struct BasicPackage;

impl BasicPackage {
    pub(crate) fn write<W: Write, A: ArchiveWrite<W>, S: Signer, P: AsRef<Path>>(
        control_data: &ControlData,
        directory: P,
        writer: W,
        signer: &S,
        signature_kind: SignatureKind,
    ) -> Result<(), std::io::Error> {
        let directory = directory.as_ref();
        let mut data = tar::Builder::new(GzEncoder::new(
            Vec::with_capacity(4096),
            Compression::best(),
        ));
        let mut control = tar::Builder::new(GzEncoder::new(
            Vec::with_capacity(4096),
            Compression::best(),
        ));
        for entry in WalkDir::new(directory).into_iter() {
            let entry = entry?;
            let relative_path = Path::new(".").join(
                entry
                    .path()
                    .strip_prefix(directory)
                    .map_err(std::io::Error::other)?
                    .normalize(),
            );
            let mut header = tar::Header::new_old();
            header.set_metadata(&std::fs::metadata(entry.path())?);
            header.set_path(relative_path.as_path())?;
            header.set_uid(0);
            header.set_gid(0);
            header.set_cksum();
            if entry.file_type().is_dir() {
                data.append::<&[u8]>(&header, &[])?;
            } else {
                let mut reader = File::open(entry.path())?;
                data.append(&header, &mut reader)?;
            }
        }
        let data = data.into_inner()?.finish()?;
        control.add_regular_file("control", control_data.to_string())?;
        let control = control.into_inner()?.finish()?;
        let debian_binary = "2.0\n";
        let mut message_bytes: Vec<u8> = Vec::new();
        message_bytes.extend(debian_binary.as_bytes());
        message_bytes.extend(&control);
        message_bytes.extend(&data);
        let signature = signer
            .sign(&message_bytes[..])
            .map_err(|_| std::io::Error::other("failed to sign the archive"))?;
        let mut package = A::new(writer);
        package.add_regular_file("debian-binary", debian_binary)?;
        package.add_regular_file("control.tar.gz", control)?;
        package.add_regular_file("data.tar.gz", data)?;
        match signature_kind {
            SignatureKind::Bundled { file_name } => {
                package.add_regular_file(file_name, signature)?;
                package.into_inner()?;
            }
            SignatureKind::Detached { mut writer } => {
                package.into_inner()?;
                writer.write_all(&signature[..])?;
                writer.flush()?;
            }
        }
        Ok(())
    }

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
                Some("debian-binary") => {
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

pub enum SignatureKind {
    Bundled { file_name: PathBuf },
    Detached { writer: Box<dyn Write> },
}
