mod package;
mod repository;
mod signer;

pub use self::package::*;
pub use self::repository::*;
pub use self::signer::*;

pub type Error = crate::deb::Error;
pub type MultilineValue = crate::deb::MultilineValue;
pub type PackageName = crate::deb::PackageName;
pub type PackageVersion = crate::deb::Version;
pub type SimpleValue = crate::deb::SimpleValue;
