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
    ControlData(String),
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
}
