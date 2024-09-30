pub mod archive;
pub mod deb;
pub mod hash;
pub mod ipk;
pub mod pkg;

pub use self::deb::Package as DebPackage;
pub use self::ipk::Package as IpkPackage;
pub use self::pkg::Package as PkgPackage;
