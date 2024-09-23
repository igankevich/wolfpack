use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid package name `{0}`")]
    PackageName(String),
    #[error("invalid package version `{0}`")]
    PackageVersion(String),
    #[error("invalid field name `{0}`")]
    FieldName(String),
    #[error("invalid field value `{0}`")]
    FieldValue(String),
}
