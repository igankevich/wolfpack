use std::borrow::Cow;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid package name {0:?}")]
    PackageName(String),
    #[error("invalid package version {0:?}")]
    PackageVersion(String),
    #[error("invalid field name {0:?}")]
    FieldName(String),
    #[error("invalid field value {0:?}")]
    FieldValue(String),
    #[error("invalid line in control data: {0:?}")]
    Package(String),
    #[error("{0:?} is missing")]
    MissingField(&'static str),
    #[error("duplicate field {0:?}")]
    DuplicateField(String),
    #[error("`{0}` is missing in the archive")]
    MissingFile(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("directory traversing error: {0}")]
    WalkDir(#[from] walkdir::Error),
    #[error("md5sums parsing error")]
    Md5Sums,
    #[error("invalid md5 hash")]
    InvalidMd5,
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn other<'a, S>(s: S) -> Self
    where
        S: Into<Cow<'a, str>>,
    {
        let s: Cow<'a, str> = s.into();
        Self::Other(s.into_owned())
    }
}
