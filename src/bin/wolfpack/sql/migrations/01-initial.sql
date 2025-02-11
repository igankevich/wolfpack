CREATE TABLE downloaded_files (
    url TEXT NOT NULL PRIMARY KEY,
    etag BLOB,
    last_modified BLOB,
    expires INTEGER,
    -- TODO hash?
    file_size INTEGER
);

-- Single component, i.e. `Packages` file.
CREATE TABLE deb_components (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL UNIQUE,
    -- TODO deb_repos table
    repo_name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    suite TEXT NOT NULL,
    component TEXT NOT NULL,
    architecture TEXT NOT NULL
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
    component_id INTEGER NOT NULL
        REFERENCES deb_components(id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
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
