use std::fs::File;
use std::io::Error;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;

use normalize_path::NormalizePath;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::write::ZipWriter;

use crate::hash::Sha256Reader;
use crate::msix::xml;

pub struct Package {
    pub identifier: String,
    pub version: String,
}

impl Package {
    pub fn write<W: Read + Write + Seek, P: AsRef<Path>>(
        &self,
        file: W,
        directory: P,
        //signer: &PackageSigner,
    ) -> Result<(), Error> {
        let directory = directory.as_ref();
        let mut writer = ZipWriter::new(file);
        for entry in WalkDir::new(directory).into_iter() {
            let entry = entry?;
            let entry_path = entry
                .path()
                .strip_prefix(directory)
                .map_err(Error::other)?
                .normalize();
            if entry_path == Path::new("") {
                continue;
            }
            let relative_path = Path::new(".").join(entry_path);
            // TODO symlinks
            if entry.file_type().is_dir() {
                writer.add_directory_from_path(relative_path, SimpleFileOptions::default())?;
            } else {
                writer.start_file_from_path(relative_path, SimpleFileOptions::default())?;
                std::io::copy(&mut File::open(entry.path())?, writer.by_ref())?;
            }
        }
        let mut archive = writer.finish_into_readable()?;
        let mut files = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            // TODO raw affects size or not ???
            let mut file = archive.by_index_raw(i)?;
            if file.is_dir() {
                continue;
            }
            let sha256_reader = Sha256Reader::new(&mut file);
            let (hash, _) = sha256_reader.digest()?;
            std::io::copy(&mut file, &mut std::io::stdout())?;
            files.push(xml::File {
                name: file.name().into(),
                size: file.size(),
                lfh_size: file.data_start() - file.header_start(),
                blocks: vec![xml::Block {
                    hash: hash.to_base64(),
                    size: file.compressed_size(),
                }],
            });
        }
        let mut file = archive.into_inner();
        file.seek(SeekFrom::Start(0))?;
        let block_map = xml::BlockMap {
            hash_method: "http://www.w3.org/2001/04/xmlenc#sha256".into(),
            files,
        };
        let mut writer = ZipWriter::new(file);
        writer.start_file_from_path("AppxBlockMap.xml", SimpleFileOptions::default())?;
        block_map.write(writer.by_ref())?;
        writer.finish()?;
        Ok(())
    }
}
