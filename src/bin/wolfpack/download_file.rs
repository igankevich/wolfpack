use std::ffi::OsString;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use fs_err::remove_file;
use fs_err::File;
use indicatif::ProgressBar;
use parking_lot::Mutex;
use reqwest::header::HeaderValue;
use reqwest::header::AGE;
use reqwest::header::CACHE_CONTROL;
use reqwest::header::CONTENT_LENGTH;
use reqwest::header::ETAG;
use reqwest::header::IF_MODIFIED_SINCE;
use reqwest::header::IF_NONE_MATCH;
use reqwest::header::LAST_MODIFIED;
use reqwest::header::USER_AGENT;
use reqwest::StatusCode;
use wolfpack::hash::AnyHash;
use wolfpack::hash::Hasher;

use crate::db::ConnectionArc;
use crate::Config;
use crate::Error;

pub struct DownloadedFile {
    pub etag: Option<HeaderValue>,
    pub last_modified: Option<HeaderValue>,
    pub expires: Option<SystemTime>,
    pub file_size: Option<u64>,
}

pub async fn download_file<P: AsRef<Path>>(
    url: &str,
    path: P,
    hash: Option<AnyHash>,
    conn: ConnectionArc,
    config: &Config,
    progress_bar: Option<Arc<Mutex<ProgressBar>>>,
) -> Result<(), Error> {
    do_download_file(url, path, hash, conn, config, progress_bar)
        .await
        .inspect_err(|e| log::error!("Failed to download {}: {}", url, e))
}

async fn do_download_file<P: AsRef<Path>>(
    url: &str,
    path: P,
    hash: Option<AnyHash>,
    conn: ConnectionArc,
    config: &Config,
    progress_bar: Option<Arc<Mutex<ProgressBar>>>,
) -> Result<(), Error> {
    let path = path.as_ref();
    let downloaded_file = conn.lock().select_downloaded_file(url)?;
    let client = reqwest::Client::builder().build()?;
    let mut externally_modified = false;
    let mut response = loop {
        let mut metadata = None;
        let builder = client.get(url).header(USER_AGENT, &WOLFPACK_UA);
        let builder = if !externally_modified {
            if let Some(downloaded_file) = downloaded_file.as_ref() {
                if let Some(expires) = downloaded_file.expires {
                    if expires > SystemTime::now() {
                        log::debug!("Fresh {}", url);
                        return Ok(());
                    }
                }
                metadata = path.metadata().ok();
                if metadata.is_some() {
                    let builder = if let Some(etag) = downloaded_file.etag.as_ref() {
                        builder.header(IF_NONE_MATCH, etag)
                    } else {
                        builder
                    };
                    if let Some(last_modified) = downloaded_file.last_modified.as_ref() {
                        builder.header(IF_MODIFIED_SINCE, last_modified)
                    } else {
                        builder
                    }
                } else {
                    builder
                }
            } else {
                builder
            }
        } else {
            builder
        };
        let response = builder.send().await?;
        let response = response.error_for_status().map_err(|e| {
            if e.status() == Some(StatusCode::NOT_FOUND) {
                Error::ResourceNotFound(url.into())
            } else {
                e.into()
            }
        })?;
        if response.status() == StatusCode::NOT_MODIFIED {
            if let Some(downloaded_file) = downloaded_file.as_ref() {
                let file_size = metadata.map(|m| m.size());
                match (file_size, downloaded_file.file_size) {
                    (Some(len1), Some(len2)) if len1 != len2 => {
                        // File was externally modified.
                        log::debug!("Force-update {}", url);
                        externally_modified = true;
                        continue;
                    }
                    _ => {}
                }
            }
            log::debug!("Up-to-date {}", url);
            return Ok(());
        }
        break response;
    };
    let max_age: Option<u64> = response
        .headers()
        .get(CACHE_CONTROL)
        .and_then(|header| get_header_subvalue(header, "max-age"));
    let age: Option<u64> = response
        .headers()
        .get(AGE)
        .and_then(|age| age.to_str().ok())
        .and_then(|s| s.parse().ok());
    let real_max_age = max_age.map(|max_age| {
        let max_age_secs = max_age
            .saturating_sub(age.unwrap_or(0))
            .min(config.max_age().as_secs());
        Duration::from_secs(max_age_secs)
    });
    let etag = response.headers().get(ETAG).map(|x| x.as_bytes());
    let last_modified = response.headers().get(LAST_MODIFIED).map(|x| x.as_bytes());
    let content_length: Option<u64> = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|age| age.to_str().ok())
        .and_then(|s| s.parse().ok());
    conn.lock()
        .insert_downloaded_file(url, etag, last_modified, real_max_age, content_length)?;
    let temporary_path = to_temporary_path(path);
    let mut file = File::create(&temporary_path)?;
    let mut hasher = hash.as_ref().map(|h| h.hasher());
    log::debug!("Downloading {} to {}", url, path.display());
    if let Some(progress_bar) = progress_bar.as_ref() {
        if let Some(content_length) = content_length {
            progress_bar.lock().inc_length(content_length);
        }
    }
    while let Some(chunk) = response.chunk().await? {
        if let Some(progress_bar) = progress_bar.as_ref() {
            progress_bar.lock().inc(chunk.len() as u64);
        }
        if let Some(ref mut hasher) = hasher {
            hasher.update(&chunk);
        }
        file.write_all(&chunk)?;
    }
    file.flush()?;
    drop(file);
    log::debug!("Finished downloading {} to {}", url, path.display());
    if let (Some(hash), Some(hasher)) = (hash, hasher) {
        let actual_hash = hasher.finalize();
        if hash != actual_hash {
            remove_file(&temporary_path)?;
            return Err(Error::HashMismatch(hash.into(), actual_hash.into()));
        }
    }
    fs_err::rename(&temporary_path, path)?;
    Ok(())
}

fn get_header_subvalue<T: FromStr>(header: &HeaderValue, name: &str) -> Option<T> {
    let s = header.to_str().ok()?;
    for part in s.split(',') {
        let part = part.trim();
        let mut iter = part.splitn(2, '=');
        if iter.next() != Some(name) {
            continue;
        }
        return iter.next().and_then(|value| value.parse::<T>().ok());
    }
    None
}

fn to_temporary_path(file: &Path) -> PathBuf {
    let mut new_file = file.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let file_name = file.file_name().unwrap_or_default();
    let mut new_file_name = OsString::new();
    new_file_name.push(".");
    new_file_name.push(file_name);
    new_file_name.push(".tmp");
    new_file.push(new_file_name);
    new_file
}

const WOLFPACK_UA: HeaderValue =
    HeaderValue::from_static(concat!("Wolfpack/", env!("CARGO_PKG_VERSION")));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_header_subvalue() {
        let max_age: u64 = get_header_subvalue(
            &HeaderValue::from_str("public, max-age=120").unwrap(),
            "max-age",
        )
        .unwrap();
        assert_eq!(120, max_age);
    }
}
