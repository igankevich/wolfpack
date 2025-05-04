use std::io::ErrorKind;
use std::path::PathBuf;

use reqwest::header::InvalidHeaderValue;
use thiserror::Error;
use wolfpack::hash::AnyHash;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Logger error: {0}")]
    Logger(log::SetLoggerError),
    #[error("Toml parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("Toml write error: {0}")]
    TomlToString(#[from] toml::ser::Error),
    #[error("Input/output error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("SQLite migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("Unsupported architecture: {0}")]
    UnsupportedArchitecture(String),
    #[error("DEB error: {0}")]
    Deb(#[from] wolfpack::deb::Error),
    #[error("Failed to verify {0:?}")]
    Verify(PathBuf),
    #[error("Package `{0}` not found")]
    NotFound(String),
    #[error("Dependency `{0}` not found")]
    DependencyNotFound(String),
    #[error("ELF error: {0}")]
    Elf(#[from] elb::Error),
    #[error("ELF dynamic loader error: {0}")]
    ElfDl(#[from] elb_dl::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Invalid header: {0}")]
    Header(#[from] InvalidHeaderValue),
    #[error("Resource `{0}` not found")]
    ResourceNotFound(String),
    #[error("Hash mismatch: expected {0}, actual {1}")]
    HashMismatch(Box<AnyHash>, Box<AnyHash>),
    #[error("No hash")]
    NoHash,
    #[error("Task error: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("Task channel error: {0}")]
    OneShot(#[from] tokio::sync::oneshot::error::RecvError),
    #[error("Signing error")]
    Sign,
    #[error("Failed to read `{0}`: {1}")]
    FileRead(PathBuf, std::io::Error),
    #[error("Invalid installation prefix {0:?}")]
    InstallationPrefix(PathBuf),
    #[error("Cargo error: {0}")]
    Cargo(#[from] wolfpack::cargo::Error),
    #[error("{0}")]
    Command(#[from] command_error::Error),
    #[error("Directory traversing error: {0}")]
    WalkDir(#[from] walkdir::Error),
    #[error("Package {0:?} not found")]
    PackageNotFound(String),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn file_read(file: impl Into<PathBuf>, error: std::io::Error) -> Self {
        Self::FileRead(file.into(), error)
    }
}

impl From<ErrorKind> for Error {
    fn from(other: ErrorKind) -> Self {
        Self::Io(other.into())
    }
}

impl From<log::SetLoggerError> for Error {
    fn from(other: log::SetLoggerError) -> Self {
        Self::Logger(other)
    }
}

impl From<wolfpack::sign::Error> for Error {
    fn from(_: wolfpack::sign::Error) -> Self {
        Self::Sign
    }
}
