CREATE TABLE downloaded_files (
    url TEXT NOT NULL PRIMARY KEY,
    etag BLOB,
    last_modified BLOB,
    expires INTEGER,
    -- TODO hash?
    file_size INTEGER
);

CREATE TABLE deb_repos (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    url TEXT NOT NULL UNIQUE
);

-- Single component, i.e. `Packages` file.
CREATE TABLE deb_components (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL UNIQUE,
    suite TEXT NOT NULL,
    component TEXT NOT NULL,
    architecture TEXT NOT NULL,
    repo_id INTEGER NOT NULL
        REFERENCES deb_repos(id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
);

-- Single DEB package.
CREATE TABLE deb_packages (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL UNIQUE,
    filename BLOB NOT NULL UNIQUE,
    hash BLOB,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    architecture TEXT NOT NULL,
    description TEXT NOT NULL,
    homepage TEXT,
    installed_size INTEGER,
    provides TEXT,
    depends TEXT,
    repo_id INTEGER NOT NULL
        REFERENCES deb_repos(id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
);

CREATE INDEX deb_packages_name ON deb_packages(name);
CREATE INDEX deb_packages_provides ON deb_packages(provides);

-- DEB package dependencies that resolve to packages unambigously.
CREATE TABLE deb_dependencies (
    -- Dependent.
    child INTEGER NOT NULL
        REFERENCES deb_packages(id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    -- Dependency.
    parent INTEGER NOT NULL
        REFERENCES deb_packages(id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    PRIMARY KEY (child, parent)
);

-- DEB package provisions.
CREATE TABLE deb_provisions (
    package_id INTEGER NOT NULL
        REFERENCES deb_packages(id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    name TEXT NOT NULL,
    version TEXT,
    PRIMARY KEY (package_id, name)
);

CREATE INDEX deb_provisions_name ON deb_provisions(name);

-- Package contents.
CREATE TABLE deb_files (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    path BLOB NOT NULL UNIQUE,
    command BLOB
);

CREATE INDEX deb_files_command ON deb_files(command);

CREATE TABLE deb_package_files (
    package_id INTEGER NOT NULL
        REFERENCES deb_packages(id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    file_id INTEGER NOT NULL
        REFERENCES deb_files(id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    PRIMARY KEY (package_id, file_id)
);

-- Full-text search for DEB packages. {{{
CREATE VIRTUAL TABLE deb_packages_fts
USING fts5(
    name,
    description,
    homepage,
    content = 'deb_packages',
    content_rowid = 'id',
    tokenize = 'porter unicode61 remove_diacritics 2'
);

CREATE TRIGGER deb_packages_after_insert
AFTER INSERT ON deb_packages
BEGIN
    INSERT INTO deb_packages_fts(rowid, name, description)
    VALUES (new.id, new.name, new.description);
END;

CREATE TRIGGER deb_packages_after_delete
AFTER DELETE ON deb_packages
BEGIN
    INSERT INTO deb_packages_fts(deb_packages_fts, rowid, name, description)
    VALUES('delete', old.name, old.description, old.description);
END;

CREATE TRIGGER deb_packages_after_update
AFTER UPDATE ON deb_packages
BEGIN
    INSERT INTO deb_packages_fts(deb_packages_fts, rowid, name, description)
    VALUES('delete', old.name, old.name, old.description);
    INSERT INTO deb_packages_fts(rowid, name, description)
    VALUES (new.name, new.name, new.description);
END;
-- }}}
