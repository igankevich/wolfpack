mod package;
mod signer;

pub use self::package::*;
pub use self::signer::*;

pub type PackageVersion = crate::deb::PackageVersion;
pub type PackageName = crate::deb::PackageName;
pub type SimpleValue = crate::deb::SimpleValue;
pub type MultilineValue = crate::deb::MultilineValue;
pub type ControlData = crate::deb::ControlData;
