use log::error;
use log::trace;
use parking_lot::Mutex;
use rusqlite::functions::FunctionFlags;
use rusqlite::params_from_iter;
use rusqlite::types::FromSql;
use rusqlite::types::FromSqlResult;
use rusqlite::types::ToSql;
use rusqlite::types::ToSqlOutput;
use rusqlite::types::ValueRef;
use rusqlite::DatabaseName;
use rusqlite::OptionalExtension;
use rusqlite_migration::{Migrations, M};
use sql_minifier::macros::load_sql;
use std::cmp::Ordering;
use std::ffi::OsString;
use std::fs::create_dir_all;
use std::hash::Hash;
use std::hash::Hasher;
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
    inner: rusqlite::Connection,
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
        filename: &Path,
        component_id: Id,
    ) -> Result<(), Error> {
        let hash = package.hash();
        let id = self
            .inner
            .prepare_cached(
                "INSERT INTO deb_packages(name, version, architecture, description, \
                installed_size, depends, url, filename, hash, homepage, component_id) \
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
                ON CONFLICT(url) DO NOTHING
                RETURNING id",
            )?
            .query_row(
                (
                    package.inner.name.as_str(),
                    package.inner.version.to_string(),
                    package.inner.architecture.as_str(),
                    package.inner.description.as_str(),
                    package.inner.installed_size.map(|s| s.saturating_mul(1024)), // Convert from KiB.
                    if !package.inner.depends.is_empty() {
                        Some(package.inner.depends.to_string())
                    } else {
                        None
                    },
                    url,
                    filename.as_bytes(),
                    hash.as_ref().map(|x| x.as_bytes()).ok_or(Error::NoHash)?,
                    package.inner.homepage.as_ref().map(|x| x.as_str()),
                    component_id,
                ),
                |row| {
                    let id: Id = row.get(0)?;
                    Ok(id)
                },
            )
            .optional()?;
        if let Some(id) = id {
            for dep in package.inner.provides.iter() {
                self.inner
                    .prepare_cached(
                        "INSERT INTO deb_provisions(package_id, name, version) VALUES(?1, ?2, ?3) ON CONFLICT DO NOTHING",
                    )?
                    .execute((id, dep.name.as_str(), dep.version.as_ref().map(|v| v.version.to_string())))
                    .optional()?;
            }
        }
        Ok(())
    }

    pub fn find_deb_packages(
        &self,
        repo_name: &str,
        architecture: &str,
        query: &str,
    ) -> Result<Vec<DebMatch>, Error> {
        self.inner
            .prepare_cached(
                "SELECT deb_packages.name, deb_packages.version, deb_packages.description
                FROM deb_packages_fts
                JOIN deb_packages
                  ON id=deb_packages_fts.rowid
                WHERE architecture IN (?3, 'all')
                  AND EXISTS(SELECT repo_name FROM deb_components WHERE component_id=id AND repo_name=?2)
                  AND deb_packages_fts MATCH ?1
                ORDER BY rank",
            )?
            .query_map((query, repo_name, architecture), |row| {
                Ok(DebMatch {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    description: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn find_deb_packages_by_name(
        &self,
        repo_name: &str,
        name: &str,
    ) -> Result<Vec<DebDependencyMatch>, Error> {
        self.inner
            .prepare_cached("SELECT name, version, description, depends, url ,filename, hash, id
                FROM deb_packages
                WHERE name=?1
                  AND EXISTS(SELECT repo_name FROM deb_components WHERE component_id=id AND repo_name=?2)
                ORDER BY name ASC, version DESC")?
            .query_map((name, repo_name), |row| {
                Ok(DebDependencyMatch {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    description: row.get(2)?,
                    depends: row.get::<usize, DebDependencies>(3)?.0,
                    url: row.get(4)?,
                    filename: PathBuf::from_bytes(row.get::<usize, Vec<u8>>(5)?),
                    hash: row.get::<usize, OptionAnyHash>(6)?.0,
                    id: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn select_deb_dependencies(
        &self,
        repo_name: &str,
        choices: &deb::DependencyChoice,
    ) -> Result<Vec<DebDependencyMatch>, Error> {
        let mut matches = self.select_deb_dependencies_by_name(repo_name, choices)?;
        //if !matches.is_empty() {
        //    return Ok(matches);
        //}
        matches.extend(self.select_deb_dependencies_by_provision(repo_name, choices)?);
        matches.sort_unstable_by_key(|dep| dep.id);
        matches.dedup();
        Ok(matches)
    }

    pub fn select_deb_dependencies_by_name(
        &self,
        repo_name: &str,
        choices: &deb::DependencyChoice,
    ) -> Result<Vec<DebDependencyMatch>, Error> {
        use std::fmt::Write;
        // Convert choices to string.
        let mut condition = String::new();
        let mut params = Vec::new();
        params.push(repo_name.to_string());
        for (i, dep) in choices.iter().enumerate() {
            if i != 0 {
                let _ = write!(&mut condition, " OR ");
            }
            params.push(dep.name.to_string());
            // Compare name.
            let _ = write!(&mut condition, "(name = ?{}", params.len());
            if let Some(version) = dep.version.as_ref() {
                let operator = dependency_operator_to_sql(version.operator);
                let _ = write!(
                    &mut condition,
                    " AND deb_version_compare(version, {}){}",
                    &version.version as *const deb::Version as i64, operator,
                );
            }
            let _ = write!(&mut condition, ")");
        }
        trace!("Condition: {:?}", condition);
        trace!("Params: {:?}", params);
        self.inner
            .prepare(
                &format!("SELECT name, version, description, depends, url, filename, hash, id
                FROM deb_packages
                WHERE EXISTS(SELECT repo_name FROM deb_components WHERE component_id=id AND repo_name=?1)
                  AND ({})
                ORDER BY name ASC, version DESC", condition)
            )?
            .query_map(params_from_iter(params.into_iter()), |row| {
                Ok(DebDependencyMatch {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    description: row.get(2)?,
                    depends: row.get::<usize, DebDependencies>(3)?.0,
                    url: row.get(4)?,
                    filename: PathBuf::from_bytes(row.get::<usize, Vec<u8>>(5)?),
                    hash: row.get::<usize, OptionAnyHash>(6)?.0,
                    id: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn select_deb_dependencies_by_provision(
        &self,
        repo_name: &str,
        choices: &deb::DependencyChoice,
    ) -> Result<Vec<DebDependencyMatch>, Error> {
        use std::fmt::Write;
        // Convert choices to string.
        let mut condition = String::new();
        let mut params = Vec::new();
        params.push(repo_name.to_string());
        let _ = write!(
            &mut condition,
            " id IN (SELECT package_id FROM deb_provisions WHERE"
        );
        for (i, dep) in choices.iter().enumerate() {
            if i != 0 {
                let _ = write!(&mut condition, " OR");
            }
            params.push(dep.name.to_string());
            let _ = write!(&mut condition, " (deb_provisions.name = ?{}", params.len(),);
            if let Some(version) = dep.version.as_ref() {
                let operator = dependency_operator_to_sql(version.operator);
                let _ = write!(
                    &mut condition,
                    " AND version IS NOT NULL AND deb_version_compare(version, {}){}",
                    &version.version as *const deb::Version as i64, operator
                );
            }
            let _ = write!(&mut condition, ")");
        }
        let _ = write!(&mut condition, ")");
        trace!("Condition: {:?}", condition);
        trace!("Params: {:?}", params);
        self.inner
            .prepare(
                &format!("SELECT name, version, description, depends, url, filename, hash, id
                FROM deb_packages
                WHERE EXISTS(SELECT repo_name FROM deb_components WHERE component_id=id AND repo_name=?1)
                  AND ({})
                ORDER BY name ASC, version DESC", condition)
            )?
            .query_map(params_from_iter(params.into_iter()), |row| {
                Ok(DebDependencyMatch {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    description: row.get(2)?,
                    depends: row.get::<usize, DebDependencies>(3)?.0,
                    url: row.get(4)?,
                    filename: PathBuf::from_bytes(row.get::<usize, Vec<u8>>(5)?),
                    hash: row.get::<usize, OptionAnyHash>(6)?.0,
                    id: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn select_resolved_deb_dependencies(
        &self,
        repo_name: &str,
        package_name: &str,
    ) -> Result<Vec<DebDependencyMatch>, Error> {
        self.inner
            .prepare_cached(
                "SELECT name, version, description, depends, url, filename, hash, id
                FROM deb_packages
                WHERE id IN (
                    SELECT parent AS dependency
                    FROM deb_dependencies
                    WHERE child IN (
                        SELECT id
                        FROM deb_packages
                        WHERE name=?1 AND component_id IN
                            (SELECT component_id FROM deb_components WHERE repo_name=?2)))",
            )?
            .query_map((package_name, repo_name), |row| {
                Ok(DebDependencyMatch {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    description: row.get(2)?,
                    depends: row.get::<usize, DebDependencies>(3)?.0,
                    url: row.get(4)?,
                    filename: PathBuf::from_bytes(row.get::<usize, Vec<u8>>(5)?),
                    hash: row.get::<usize, OptionAnyHash>(6)?.0,
                    id: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn insert_deb_dependency(
        &self,
        repo_name: &str,
        child: &str,
        parent: Id,
    ) -> Result<(), Error> {
        self.inner
            .prepare_cached(
                "INSERT INTO deb_dependencies(child, parent)
                VALUES(
                    (SELECT id FROM deb_packages
                     WHERE component_id IN (SELECT component_id FROM deb_components WHERE repo_name=?1)
                       AND name=?2),
                    ?3
                )
                ON CONFLICT DO NOTHING",
            )?
            .execute((repo_name, child, parent))
            .optional()?;
        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if self.inner.is_readonly(DatabaseName::Main).unwrap_or(true) {
            return;
        }
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

struct OptionAnyHash(Option<AnyHash>);

impl FromSql for OptionAnyHash {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let Some(bytes) = value.as_bytes_or_null()? else {
            return Ok(OptionAnyHash(None));
        };
        let hash: Option<AnyHash> = bytes.try_into().ok();
        Ok(OptionAnyHash(hash))
    }
}

struct DebDependencies(deb::Dependencies);

impl FromSql for DebDependencies {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str_or_null()? {
            Some(s) => Ok(DebDependencies(
                s.parse::<deb::Dependencies>().unwrap_or_default(),
            )),
            None => Ok(DebDependencies(Default::default())),
        }
    }
}

pub struct DebMatch {
    pub name: String,
    pub version: String,
    pub description: String,
}

pub struct DebDependencyMatch {
    pub id: Id,
    pub name: String,
    pub version: String,
    pub description: String,
    pub depends: deb::Dependencies,
    pub url: String,
    pub filename: PathBuf,
    pub hash: Option<AnyHash>,
}

impl PartialEq for DebDependencyMatch {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl Eq for DebDependencyMatch {}

impl Hash for DebDependencyMatch {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.id.hash(state);
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

const fn dependency_operator_to_sql(operator: deb::DependencyVersionOp) -> &'static str {
    match operator {
        deb::DependencyVersionOp::Lesser => " < 0",
        deb::DependencyVersionOp::LesserEqual => " <= 0",
        deb::DependencyVersionOp::Equal => " = 0",
        deb::DependencyVersionOp::Greater => " > 0",
        deb::DependencyVersionOp::GreaterEqual => " >= 0",
    }
}

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
