use log::error;
use parking_lot::Mutex;
use rusqlite::types::ToSql;
use rusqlite::types::ToSqlOutput;
use rusqlite::OptionalExtension;
use rusqlite_migration::{Migrations, M};
use sql_minifier::macros::load_sql;
use std::fs::create_dir_all;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use wolfpack::deb;

use crate::Config;
use crate::DownloadedFile;
use crate::Error;

pub type ConnectionArc = Arc<Mutex<Connection>>;
pub type Id = i64;

pub struct Connection {
    inner: rusqlite::Connection,
}

impl Connection {
    pub fn new(config: &Config) -> Result<ConnectionArc, Error> {
        let path = config.database_path();
        if let Some(dirname) = path.parent() {
            create_dir_all(dirname)?;
        }
        let mut conn = rusqlite::Connection::open(&path)?;
        conn.set_prepared_statement_cache_capacity(MAX_CACHED_QUERIES);
        conn.execute_batch(PREAMBLE)?;
        let migrations = Migrations::new(MIGRATIONS.into());
        migrations.to_latest(&mut conn)?;
        conn.execute_batch(POST_MIGRATIONS)?;
        Ok(Arc::new(Mutex::new(Self { inner: conn })))
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

    pub fn insert_deb_component(
        &self,
        url: &str,
        repo_name: &str,
        base_url: &str,
        suite: &str,
        component: &str,
        architecture: &str,
    ) -> Result<Id, Error> {
        let id = self.inner
            .prepare_cached(
                "INSERT INTO deb_components(url, repo_name, base_url, suite, component, architecture) \
                VALUES(?1, ?2, ?3, ?4, ?5, ?6) \
                ON CONFLICT(url) DO NOTHING \
                RETURNING id",
            )?
            .query_row((url, repo_name, base_url, suite, component, architecture), |row| {
                let id: Id = row.get(0)?;
                Ok(id)
            })
            .optional()?;
        match id {
            Some(id) => Ok(id),
            None => Ok(self
                .inner
                .prepare_cached("SELECT id FROM deb_components WHERE url = ?1")?
                .query_row((url,), |row| {
                    let id: Id = row.get(0)?;
                    Ok(id)
                })
                .optional()?
                .expect("Should return id")),
        }
    }

    pub fn insert_deb_package(
        &self,
        package: &deb::ExtendedPackage,
        url: &str,
        component_id: Id,
    ) -> Result<(), Error> {
        self.inner
            .prepare_cached(
                "INSERT INTO deb_packages(name, version, architecture, description, installed_size, url, component_id) \
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                ON CONFLICT(url) DO NOTHING"
            )?
            .execute((
                package.inner.name.as_str(),
                package.inner.version.to_string(),
                package.inner.architecture.as_str(),
                package.inner.description.as_str(),
                package.inner.installed_size,
                url,
                component_id
            ))
            .optional()?;
        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Err(e) = self.inner.execute_batch(POSTAMBLE) {
            error!("Failed to execute SQL postamble: {e}");
        }
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

/*
pub trait PathAsBytes {
    fn as_bytes(&self) -> &[u8];
}

impl PathAsBytes for Path {
    #[cfg(unix)]
    fn as_bytes(&self) -> &[u8] {
        // TODO windows version
        use std::os::unix::ffi::OsStrExt;
        self.as_os_str().as_bytes()
    }
}
*/

const PREAMBLE: &str = load_sql!("src/bin/wolfpack/sql/preamble.sql");
const POSTAMBLE: &str = load_sql!("src/bin/wolfpack/sql/postamble.sql");
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
