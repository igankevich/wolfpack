use log::trace;
use rusqlite::params_from_iter;
use rusqlite::types::FromSql;
use rusqlite::types::FromSqlResult;
use rusqlite::types::ValueRef;
use rusqlite::OptionalExtension;
use std::cmp::Ordering;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use std::path::PathBuf;
use wolfpack::deb;
use wolfpack::hash::AnyHash;

use crate::db::Connection;
use crate::db::Id;
use crate::db::OptionAnyHash;
use crate::db::PathAsBytes;
use crate::db::PathFromBytes;
use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoId(pub Id);

impl Connection {
    pub fn insert_deb_repo(&self, name: &str, url: &str) -> Result<RepoId, Error> {
        let id = self
            .inner
            .prepare_cached(
                "INSERT INTO deb_repos(name, url) \
                VALUES(?1, ?2) \
                ON CONFLICT DO NOTHING \
                RETURNING id",
            )?
            .query_row((name, url), |row| {
                let id: Id = row.get(0)?;
                Ok(id)
            })
            .optional()?;
        match id {
            Some(id) => Ok(RepoId(id)),
            None => self
                .inner
                .prepare_cached("SELECT id FROM deb_repos WHERE name = ?1")?
                .query_row((name,), |row| {
                    let id: Id = row.get(0)?;
                    Ok(id)
                })
                .map(RepoId)
                .map_err(Into::into),
        }
    }

    pub fn insert_deb_component(
        &self,
        url: &str,
        suite: &str,
        component: &str,
        architecture: &str,
        repo_id: RepoId,
    ) -> Result<Id, Error> {
        let id = self
            .inner
            .prepare_cached(
                "INSERT INTO deb_components(url, suite, component, architecture, repo_id) \
                VALUES(?1, ?2, ?3, ?4, ?5) \
                ON CONFLICT(url) DO NOTHING \
                RETURNING id",
            )?
            .query_row((url, suite, component, architecture, repo_id.0), |row| {
                let id: Id = row.get(0)?;
                Ok(id)
            })
            .optional()?;
        match id {
            Some(id) => Ok(id),
            None => self
                .inner
                .prepare_cached("SELECT id FROM deb_components WHERE url = ?1")?
                .query_row((url,), |row| {
                    let id: Id = row.get(0)?;
                    Ok(id)
                })
                .map_err(Into::into),
        }
    }

    pub fn insert_deb_package(
        &self,
        package: &deb::ExtendedPackage,
        url: &str,
        filename: &Path,
        repo_id: RepoId,
    ) -> Result<(), Error> {
        let hash = package.hash();
        let all_dependencies = {
            let mut all = Vec::new();
            all.extend(package.inner.pre_depends.iter().cloned());
            all.extend(package.inner.depends.iter().cloned());
            deb::Dependencies::new(all)
        };
        let id = self
            .inner
            .prepare_cached(
                "INSERT INTO deb_packages(name, version, architecture, description, \
                installed_size, depends, url, filename, hash, homepage, repo_id) \
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
                ON CONFLICT DO NOTHING
                RETURNING id",
            )?
            .query_row(
                (
                    package.inner.name.as_str(),
                    package.inner.version.to_string(),
                    package.inner.architecture.as_str(),
                    package.inner.description.as_str(),
                    package.inner.installed_size.map(|s| s.saturating_mul(1024)), // Convert from KiB.
                    if !all_dependencies.is_empty() {
                        Some(all_dependencies.to_string())
                    } else {
                        None
                    },
                    url,
                    filename.as_bytes(),
                    hash.as_ref().map(|x| x.as_bytes()).ok_or(Error::NoHash)?,
                    package.inner.homepage.as_ref().map(|x| x.as_str()),
                    repo_id.0,
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

    pub fn insert_deb_package_contents(
        &self,
        package_name: &str,
        files: &[PathBuf],
    ) -> Result<(), Error> {
        let package_id = self
            .inner
            .prepare_cached("SELECT id FROM deb_packages WHERE name = ?1")?
            .query_row((package_name,), |row| {
                let id: Id = row.get(0)?;
                Ok(id)
            })?;
        for file in files {
            // Index commands separately for faster search.
            let command = if let Some(parent) = file.parent() {
                match parent.file_name() {
                    Some(dirname) => {
                        if dirname == Path::new("bin") || dirname == Path::new("sbin") {
                            Some(file.file_name().expect("File name exists"))
                        } else {
                            None
                        }
                    }
                    None => None,
                }
            } else {
                None
            };
            self.inner
                .prepare_cached(
                    "INSERT INTO deb_files(path, command, package_id) VALUES (?1, ?2, ?3) \
                    ON CONFLICT DO NOTHING",
                )?
                .execute((
                    file.as_bytes(),
                    command.map(|x| x.as_encoded_bytes()),
                    package_id,
                ))
                .optional()?;
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
                  AND repo_id IN (SELECT id FROM deb_repos WHERE name=?2)
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
            .prepare_cached(
                "SELECT name, version, description, depends, url ,filename, hash, id
                FROM deb_packages
                WHERE name=?1
                  AND repo_id IN (SELECT id FROM deb_repos WHERE name=?2)
                ORDER BY name ASC, version DESC",
            )?
            .query_map((name, repo_name), |row| {
                Ok(DebDependencyMatch {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    description: row.get(2)?,
                    depends: row.get::<usize, DebDependencies>(3)?.0,
                    url: row.get(4)?,
                    filename: PathBuf::from_bytes(row.get::<usize, Vec<u8>>(5)?),
                    hash: row.get::<usize, OptionAnyHash>(6)?.into_inner(),
                    id: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn find_deb_packages_by_file(
        &self,
        repo_name: &str,
        architecture: &str,
        query: &str,
    ) -> Result<Vec<PackageFileMatch>, Error> {
        self.inner
            .prepare_cached(
                "SELECT
                  deb_files_fts.path,
                  deb_packages.name, deb_packages.version, deb_packages.description
                FROM deb_files_fts
                JOIN deb_files
                  ON deb_files.id=deb_files_fts.rowid
                JOIN deb_packages
                  ON deb_packages.id=deb_files.package_id
                WHERE architecture IN (?1, 'all')
                  AND repo_id IN (SELECT id FROM deb_repos WHERE name=?2)
                  AND deb_files_fts MATCH ?3
                ORDER BY rank",
            )?
            .query_map((architecture, repo_name, query), |row| {
                Ok(PackageFileMatch {
                    file: PathBuf::from_bytes(row.get::<usize, Vec<u8>>(0)?),
                    package: DebMatch {
                        name: row.get(1)?,
                        version: row.get(2)?,
                        description: row.get(3)?,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn find_deb_packages_by_command(
        &self,
        repo_name: &str,
        architecture: &str,
        glob: &str,
    ) -> Result<Vec<PackageFileMatch>, Error> {
        self.inner
            .prepare_cached(
                "SELECT
                  deb_files.command,
                  deb_packages.name, deb_packages.version, deb_packages.description
                FROM deb_commands_fts
                JOIN deb_files
                  ON deb_files.id=deb_commands_fts.rowid
                JOIN deb_packages
                  ON deb_packages.id=deb_files.package_id
                WHERE architecture IN (?1, 'all')
                  AND repo_id IN (SELECT id FROM deb_repos WHERE name=?2)
                  AND deb_commands_fts.command GLOB ?3
                ORDER BY deb_files.command",
            )?
            .query_map((architecture, repo_name, glob), |row| {
                Ok(PackageFileMatch {
                    file: PathBuf::from_bytes(row.get::<usize, Vec<u8>>(0)?),
                    package: DebMatch {
                        name: row.get(1)?,
                        version: row.get(2)?,
                        description: row.get(3)?,
                    },
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
            .prepare(&format!(
                "SELECT name, version, description, depends, url, filename, hash, id
                FROM deb_packages
                WHERE repo_id IN (SELECT id FROM deb_repos WHERE name=?1)
                  AND ({})
                ORDER BY name ASC, version DESC",
                condition
            ))?
            .query_map(params_from_iter(params.into_iter()), |row| {
                Ok(DebDependencyMatch {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    description: row.get(2)?,
                    depends: row.get::<usize, DebDependencies>(3)?.0,
                    url: row.get(4)?,
                    filename: PathBuf::from_bytes(row.get::<usize, Vec<u8>>(5)?),
                    hash: row.get::<usize, OptionAnyHash>(6)?.into_inner(),
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
            .prepare(&format!(
                "SELECT name, version, description, depends, url, filename, hash, id
                FROM deb_packages
                WHERE repo_id IN (SELECT id FROM deb_repos WHERE name=?1)
                  AND ({})
                ORDER BY name ASC, version DESC",
                condition
            ))?
            .query_map(params_from_iter(params.into_iter()), |row| {
                Ok(DebDependencyMatch {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    description: row.get(2)?,
                    depends: row.get::<usize, DebDependencies>(3)?.0,
                    url: row.get(4)?,
                    filename: PathBuf::from_bytes(row.get::<usize, Vec<u8>>(5)?),
                    hash: row.get::<usize, OptionAnyHash>(6)?.into_inner(),
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
                        WHERE name=?1 AND repo_id IN
                            (SELECT id FROM deb_repos WHERE name=?2)))",
            )?
            .query_map((package_name, repo_name), |row| {
                Ok(DebDependencyMatch {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    description: row.get(2)?,
                    depends: row.get::<usize, DebDependencies>(3)?.0,
                    url: row.get(4)?,
                    filename: PathBuf::from_bytes(row.get::<usize, Vec<u8>>(5)?),
                    hash: row.get::<usize, OptionAnyHash>(6)?.into_inner(),
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
                     WHERE repo_id IN (SELECT id FROM deb_repos WHERE name=?1)
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

#[derive(Debug)]
pub struct PackageFileMatch {
    pub file: PathBuf,
    pub package: DebMatch,
}

#[derive(Debug)]
pub struct DebMatch {
    pub name: String,
    pub version: String,
    pub description: String,
}

#[derive(Debug)]
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

impl PartialOrd for DebDependencyMatch {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DebDependencyMatch {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl Hash for DebDependencyMatch {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.id.hash(state);
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
