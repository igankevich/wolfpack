use fs_err::create_dir_all;
use parking_lot::Mutex;
use rusqlite::functions::FunctionFlags;
use rusqlite::types::FromSql;
use rusqlite::types::FromSqlResult;
use rusqlite::types::ToSql;
use rusqlite::types::ToSqlOutput;
use rusqlite::types::ValueRef;
use rusqlite::OptionalExtension;
use rusqlite_migration::{Migrations, M};
use sql_minifier::macros::load_sql;
use std::cmp::Ordering;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use wolfpack::deb;
use wolfpack::hash::AnyHash;

use crate::Config;
use crate::DownloadedFile;
use crate::Error;

pub type ConnectionArc = Arc<Mutex<Connection>>;
pub type Id = i64;

pub struct Connection {
    pub(crate) inner: rusqlite::Connection,
}

impl Connection {
    pub fn new(config: &Config) -> Result<ConnectionArc, Error> {
        let path = config.database_path();
        if let Some(dirname) = path.parent() {
            create_dir_all(dirname)?;
        }
        let mut conn = rusqlite::Connection::open(&path)?;
        conn.execute_batch(PREAMBLE)?;
        let migrations = Migrations::new(MIGRATIONS.into());
        migrations.to_latest(&mut conn)?;
        conn.execute_batch(POST_MIGRATIONS)?;
        Self::configure(&conn)?;
        Ok(Arc::new(Mutex::new(Self { inner: conn })))
    }

    pub fn clone_read_only(&self) -> Result<ConnectionArc, Error> {
        let conn = rusqlite::Connection::open_with_flags(
            self.inner.path().expect("Was created with path"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        Self::configure(&conn)?;
        Ok(Arc::new(Mutex::new(Self { inner: conn })))
    }

    pub fn clone_read_write(&self) -> Result<ConnectionArc, Error> {
        let conn = rusqlite::Connection::open(self.inner.path().expect("Was created with path"))?;
        Self::configure(&conn)?;
        Ok(Arc::new(Mutex::new(Self { inner: conn })))
    }

    fn configure(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
        conn.set_prepared_statement_cache_capacity(MAX_CACHED_QUERIES);
        conn.create_scalar_function(
            "deb_version_compare",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            move |ctx| {
                use rusqlite::Error::UserFunctionError;
                debug_assert_eq!(ctx.len(), 2);
                let version0 = ctx
                    .get_raw(0)
                    .as_str()
                    .map_err(|e| UserFunctionError(e.into()))?
                    .parse::<deb::Version>()
                    .map_err(|e| UserFunctionError(e.into()))?;
                let ret = match ctx.get_raw(1) {
                    ValueRef::Integer(address) => {
                        let ptr = address as *const deb::Version;
                        let version1 = unsafe { ptr.as_ref() }.expect("address is valid");
                        version0.cmp(version1)
                    }
                    other => {
                        let version1 = other
                            .as_str()
                            .map_err(|e| UserFunctionError(e.into()))?
                            .parse::<deb::Version>()
                            .map_err(|e| UserFunctionError(e.into()))?;
                        version0.cmp(&version1)
                    }
                };
                let ret: i64 = match ret {
                    Ordering::Equal => 0,
                    Ordering::Less => -1,
                    Ordering::Greater => 1,
                };
                Ok(ret)
            },
        )?;
        Ok(())
    }

    pub fn select_downloaded_file(&self, url: &str) -> Result<Option<DownloadedFile>, Error> {
        self.inner
            .prepare_cached("SELECT etag, last_modified, expires, file_size FROM downloaded_files WHERE url = ?1")?
            .query_row((url,), |row| {
                let etag: Option<Vec<u8>> = row.get(0)?;
                let last_modified: Option<Vec<u8>> = row.get(1)?;
                let expires: Option<u64> = row.get(2)?;
                let file_size: Option<u64> = row.get(3)?;
                Ok(DownloadedFile {
                    etag: match etag {
                        Some(x) => x.try_into().ok(),
                        None => None,
                    },
                    last_modified: match last_modified {
                        Some(x) => x.try_into().ok(),
                        None => None,
                    },
                    expires: match expires {
                        Some(x) => SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(x)),
                        None => None,
                    },
                    file_size,
                })
            })
            .optional()
            .map_err(Into::into)
    }

    pub fn insert_downloaded_file(
        &self,
        url: &str,
        etag: Option<&[u8]>,
        last_modified: Option<&[u8]>,
        max_age: Option<Duration>,
        file_size: Option<u64>,
    ) -> Result<(), Error> {
        let expires = max_age
            .and_then(|max_age| SystemTime::now().checked_add(max_age))
            .map(Timestamp);
        self.inner
            .prepare_cached(
                "INSERT INTO downloaded_files(url, etag, last_modified, expires, file_size) \
                VALUES(?1, ?2, ?3, ?4, ?5) \
                ON CONFLICT(url) \
                DO UPDATE \
                SET etag = excluded.etag, \
                    last_modified = excluded.last_modified, \
                    expires = excluded.expires, \
                    file_size = excluded.file_size",
            )?
            .execute((url, etag, last_modified, expires, file_size))
            .ensure_num_rows_modified(1)?;
        Ok(())
    }

    pub fn optimize(&self) -> Result<(), Error> {
        self.inner.execute_batch(
            "PRAGMA incremental_vacuum; \
PRAGMA analysis_limit=1000; \
PRAGMA optimize; \
ANALYZE;
VACUUM;",
        )?;
        Ok(())
    }
}

pub trait EnsureNumRowsModified {
    fn ensure_num_rows_modified(self, n: usize) -> Self;
}

impl EnsureNumRowsModified for Result<usize, rusqlite::Error> {
    fn ensure_num_rows_modified(self, num_rows: usize) -> Self {
        match self {
            Ok(n) if n != num_rows => Err(rusqlite::Error::QueryReturnedNoRows),
            other => other,
        }
    }
}

pub struct Timestamp(pub SystemTime);

impl From<SystemTime> for Timestamp {
    fn from(other: SystemTime) -> Self {
        Self(other)
    }
}

impl ToSql for Timestamp {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, rusqlite::Error> {
        use rusqlite::Error::ToSqlConversionFailure;
        let secs_since_epoch = self
            .0
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| ToSqlConversionFailure(e.into()))?
            .as_secs();
        let secs_since_epoch: i64 = secs_since_epoch.try_into().map_err(|_| {
            ToSqlConversionFailure(std::io::Error::other("Timestamp overflow").into())
        })?;
        Ok(ToSqlOutput::Owned(secs_since_epoch.into()))
    }
}

pub struct OptionAnyHash(Option<AnyHash>);

impl OptionAnyHash {
    pub fn into_inner(self) -> Option<AnyHash> {
        self.0
    }
}

impl FromSql for OptionAnyHash {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let Some(bytes) = value.as_bytes_or_null()? else {
            return Ok(OptionAnyHash(None));
        };
        let hash: Option<AnyHash> = bytes.try_into().ok();
        Ok(OptionAnyHash(hash))
    }
}

pub trait PathAsBytes {
    fn as_bytes(&self) -> &[u8];
}

impl PathAsBytes for Path {
    fn as_bytes(&self) -> &[u8] {
        // TODO windows version
        use std::os::unix::ffi::OsStrExt;
        self.as_os_str().as_bytes()
    }
}

pub trait PathFromBytes {
    fn from_bytes(data: Vec<u8>) -> Self
    where
        Self: Sized;
}

impl PathFromBytes for PathBuf {
    fn from_bytes(data: Vec<u8>) -> Self
    where
        Self: Sized,
    {
        // TODO windows version
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(data).into()
    }
}

const PREAMBLE: &str = load_sql!("src/bin/wolfpack/sql/preamble.sql");
const POST_MIGRATIONS: &str = load_sql!("src/bin/wolfpack/sql/post-migrations.sql");
const MIGRATIONS: [M<'static>; 1] = [M::up(include_str!("sql/migrations/01-initial.sql"))];

const MAX_CACHED_QUERIES: usize = 100;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations() {
        let migrations = Migrations::new(MIGRATIONS.into());
        assert!(migrations.validate().is_ok());
    }
}
