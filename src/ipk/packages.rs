use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fs::create_dir_all;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use walkdir::WalkDir;

use crate::hash::Sha256Hash;
use crate::hash::Sha256Reader;
use crate::ipk::Error;
use crate::ipk::Package;
use crate::ipk::PackageVerifier;
use crate::ipk::SimpleValue;

pub struct Packages {
    packages: HashMap<SimpleValue, PerArchPackages>,
}

impl Packages {
    pub fn new<I, P, P2>(
        output_dir: P2,
        paths: I,
        verifier: &PackageVerifier,
    ) -> Result<Self, Error>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let mut packages: HashMap<SimpleValue, PerArchPackages> = HashMap::new();
        let mut push_package = |path: &Path| -> Result<(), Error> {
            eprintln!("reading {}", path.display());
            let mut reader = Sha256Reader::new(File::open(path)?);
            let control = Package::read_control(&mut reader, path, verifier)?;
            let (hash, size) = reader.digest()?;
            let mut filename = PathBuf::new();
            filename.push("data");
            filename.push(hash.to_string());
            create_dir_all(output_dir.as_ref().join(&filename))?;
            filename.push(path.file_name().unwrap());
            let new_path = output_dir.as_ref().join(&filename);
            std::fs::rename(path, new_path)?;
            let control = ExtendedControlData {
                control,
                size,
                hash,
                filename,
            };
            packages
                .entry(control.control.architecture.clone())
                .or_insert_with(|| PerArchPackages {
                    packages: Vec::new(),
                })
                .packages
                .push(control);
            Ok(())
        };
        for path in paths.into_iter() {
            let path = path.as_ref();
            if path.is_dir() {
                for entry in WalkDir::new(path).into_iter() {
                    let entry = entry?;
                    if entry.file_type().is_dir()
                        || entry.path().extension() != Some(OsStr::new("deb"))
                    {
                        continue;
                    }
                    push_package(entry.path())?
                }
            } else {
                push_package(path)?
            }
        }
        Ok(Self { packages })
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SimpleValue, &PerArchPackages)> {
        self.packages.iter()
    }

    pub fn architectures(&self) -> HashSet<SimpleValue> {
        self.packages.keys().cloned().collect()
    }
}

impl Display for Packages {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for (_, per_arch_packages) in self.packages.iter() {
            Display::fmt(per_arch_packages, f)?;
        }
        Ok(())
    }
}

pub struct PerArchPackages {
    packages: Vec<ExtendedControlData>,
}

impl Display for PerArchPackages {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for control in self.packages.iter() {
            writeln!(f, "{}", control)?;
        }
        Ok(())
    }
}

pub struct ExtendedControlData {
    pub control: Package,
    hash: Sha256Hash,
    filename: PathBuf,
    size: usize,
}

impl Display for ExtendedControlData {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.control)?;
        writeln!(f, "Filename: {}", self.filename.display())?;
        writeln!(f, "Size: {}", self.size)?;
        writeln!(f, "SHA256sum: {}", self.hash)?;
        Ok(())
    }
}
