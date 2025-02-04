use log::error;
use rusqlite::OptionalExtension;
use rusqlite_migration::{Migrations, M};
use sql_minifier::macros::load_sql;
use std::fs::create_dir_all;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::Config;
use crate::DownloadedFile;
use crate::Error;

pub type ConnectionArc = Arc<Mutex<Connection>>;

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
            .prepare_cached("SELECT etag, last_modified FROM downloaded_files WHERE url = ?1")?
            .query_row((url,), |row| {
                let etag: Option<Vec<u8>> = row.get(0)?;
                let last_modified: Option<Vec<u8>> = row.get(1)?;
                Ok(DownloadedFile {
                    etag: match etag {
                        Some(x) => x.try_into().ok(),
                        None => None,
                    },
                    last_modified: match last_modified {
                        Some(x) => x.try_into().ok(),
                        None => None,
                    },
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
    ) -> Result<(), Error> {
        self.inner
            .prepare_cached(
                "INSERT INTO downloaded_files (url, etag, last_modified) VALUES (?1, ?2, ?3)",
            )?
            .execute((url, etag, last_modified))
            .ensure_num_rows_modified(1)?;
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
const MIGRATIONS: [M<'static>; 1] = [M::up(load_sql!(
    "src/bin/wolfpack/sql/migrations/01-initial.sql"
))];

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
