CREATE TABLE downloaded_files (
    url TEXT NOT NULL PRIMARY KEY,
    etag BLOB,
    last_modified BLOB,
    expires INTEGER,
    -- TODO hash?
    file_size INTEGER
);

-- CREATE INDEX downloaded_files_hash ON downloaded_files(hash);

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
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    architecture TEXT NOT NULL,
    description TEXT NOT NULL,
    installed_size INTEGER,
    component_id BLOB NOT NULL
        REFERENCES deb_components(id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
);

-- CREATE INDEX deb_packages_hash ON deb_packages(hash);
-- CREATE INDEX deb_packages_name ON deb_packages(name);
