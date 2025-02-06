use futures_util::StreamExt;
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
use std::fs::remove_file;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;
use std::time::SystemTime;
use wolfpack::hash::AnyHash;
use wolfpack::hash::Hasher;

use crate::Config;
use crate::ConnectionArc;
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
) -> Result<(), Error> {
    do_download_file(url, path, hash, conn, config)
        .await
        .inspect_err(|e| log::error!("Failed to download {}: {}", url, e))
}

async fn do_download_file<P: AsRef<Path>>(
    url: &str,
    path: P,
    hash: Option<AnyHash>,
    conn: ConnectionArc,
    config: &Config,
) -> Result<(), Error> {
    let path = path.as_ref();
    let downloaded_file = conn.lock().select_downloaded_file(url)?;
    let client = reqwest::Client::builder().build()?;
    let mut externally_modified = false;
    let response = loop {
        let mut metadata = None;
        let builder = client.get(url).header(USER_AGENT, &WOLFPACK_UA);
        let builder = if !externally_modified {
            if let Some(downloaded_file) = downloaded_file.as_ref() {
                if let Some(expires) = downloaded_file.expires {
                    if expires > SystemTime::now() {
                        log::info!("Fresh {}", url);
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
                        log::info!("Force-update {}", url);
                        externally_modified = true;
                        continue;
                    }
                    _ => {}
                }
            }
            log::info!("Up-to-date {}", url);
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
        let max_age_secs = max_age.saturating_sub(age.unwrap_or(0)).min(config.max_age);
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
    let mut stream = response.bytes_stream();
    let mut file = File::create(path)?;
    let mut hasher = hash.as_ref().map(|h| h.hasher());
    log::info!("Downloading {} to {}", url, path.display());
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if let Some(ref mut hasher) = hasher {
            hasher.update(&chunk);
        }
        file.write_all(&chunk)?;
    }
    file.flush()?;
    drop(file);
    log::info!("Finished downloading {} to {}", url, path.display());
    if let (Some(hash), Some(hasher)) = (hash, hasher) {
        let actual_hash = hasher.finalize();
        if hash != actual_hash {
            remove_file(path)?;
            return Err(Error::HashMismatch);
        }
    }
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
