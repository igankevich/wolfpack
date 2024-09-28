pub mod archive;
pub mod deb;
pub mod ipk;

pub use self::deb::Package as DebPackage;
pub use self::ipk::Package as IpkPackage;
