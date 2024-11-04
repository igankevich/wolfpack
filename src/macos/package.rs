use std::fs::File;
use std::io::Error;
use std::io::Write;
use std::path::Path;

use flate2::write::GzEncoder;
use flate2::Compression;
use tempfile::TempDir;

use crate::cpio::CpioBuilder;
use crate::macos::xml;
use crate::macos::Bom;
use crate::xar::XarBuilder;

pub struct Package {
    pub identifier: String,
    pub version: String,
}

impl Package {
    pub fn write<W: Write, P: AsRef<Path>>(&self, directory: P, writer: W) -> Result<(), Error> {
        let info = xml::PackageInfo {
            format_version: 2,
            install_location: Some("/".into()),
            identifier: self.identifier.clone(),
            version: self.version.clone(),
            generator_version: Some("wolfpack".into()),
            auth: xml::Auth::Root,
            payload: xml::Payload {
                number_of_files: 0,
                install_kb: 0,
            },
            relocatable: Default::default(),
            bundles: Default::default(),
            bundle_version: Default::default(),
            upgrade_bundle: Default::default(),
            update_bundle: Default::default(),
            atomic_update_bundle: Default::default(),
            strict_identifier: Default::default(),
            relocate: Default::default(),
            scripts: Default::default(),
        };
        let workdir = TempDir::new()?;
        let package_info_file = workdir.path().join("PackageInfo");
        info.write(File::create(&package_info_file)?)?;
        let directory = directory.as_ref();
        let bom = Bom::from_directory(directory)?;
        let bom_file = workdir.path().join("Bom");
        bom.write(File::create(&bom_file)?)?;
        let payload_file = workdir.path().join("Payload");
        CpioBuilder::from_directory(
            GzEncoder::new(File::create(&payload_file)?, Compression::best()),
            directory,
        )?
        .finish()?;
        let mut xar = XarBuilder::new(writer);
        xar.add_file_by_path("PackageInfo".into(), &package_info_file)?;
        xar.add_file_by_path("Bom".into(), &bom_file)?;
        xar.add_file_by_path("Payload".into(), &payload_file)?;
        xar.finish()?;
        // TODO sign
        Ok(())
    }
}
