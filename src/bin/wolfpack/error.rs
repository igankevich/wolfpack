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
    Toml(#[from] toml::de::Error),
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
    #[error("Failed to patch {0:?}")]
    Patch(PathBuf),
    #[error("Failed to parse ELF: {0}")]
    Elf(#[from] elf::ParseError),
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
    #[error("Signing error")]
    Sign(wolfpack::sign::Error),
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
    fn from(other: wolfpack::sign::Error) -> Self {
        Self::Sign(other)
    }
}
