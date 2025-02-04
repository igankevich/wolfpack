use futures_util::StreamExt;
use reqwest::header::HeaderValue;
use reqwest::header::ETAG;
use reqwest::header::IF_MODIFIED_SINCE;
use reqwest::header::IF_NONE_MATCH;
use reqwest::header::LAST_MODIFIED;
use reqwest::header::USER_AGENT;
use reqwest::StatusCode;
use std::path::Path;
use tokio::io::AsyncWriteExt;
use wolfpack::hash::AnyHash;
use wolfpack::hash::Hasher;

use crate::ConnectionArc;
use crate::Error;

pub struct DownloadedFile {
    pub etag: Option<HeaderValue>,
    pub last_modified: Option<HeaderValue>,
}

pub async fn download_file<P: AsRef<Path>>(
    url: &str,
    path: P,
    hash: Option<AnyHash>,
    conn: ConnectionArc,
) -> Result<(), Error> {
    do_download_file(url, path, hash, conn)
        .await
        .inspect_err(|e| log::error!("Failed to download {}: {}", url, e))
}

async fn do_download_file<P: AsRef<Path>>(
    url: &str,
    path: P,
    hash: Option<AnyHash>,
    conn: ConnectionArc,
) -> Result<(), Error> {
    // TODO expires, cache-control: max-age
    let path = path.as_ref();
    let downloaded_file = conn.lock().await.select_downloaded_file(url)?;
    let client = reqwest::Client::builder().build()?;
    log::info!("Downloading {} to {}", url, path.display());
    let builder = client.get(url).header(USER_AGENT, &WOLFPACK_UA);
    let builder = if let Some(downloaded_file) = downloaded_file {
        if path.exists() {
            let builder = if let Some(etag) = downloaded_file.etag {
                builder.header(IF_NONE_MATCH, etag)
            } else {
                builder
            };
            if let Some(last_modified) = downloaded_file.last_modified {
                builder.header(IF_MODIFIED_SINCE, last_modified)
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
    if response.status() == StatusCode::NOT_MODIFIED {
        log::info!("Up-to-date {}", url);
        // Up-to-date.
        return Ok(());
    }
    let response = response.error_for_status().map_err(|e| {
        if e.status() == Some(StatusCode::NOT_FOUND) {
            Error::ResourceNotFound(url.into())
        } else {
            e.into()
        }
    })?;
    conn.lock().await.insert_downloaded_file(
        url,
        response.headers().get(ETAG).map(|x| x.as_bytes()),
        response.headers().get(LAST_MODIFIED).map(|x| x.as_bytes()),
    )?;
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&path).await?;
    let mut hasher = hash.as_ref().map(|h| h.hasher());
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if let Some(ref mut hasher) = hasher {
            hasher.update(&chunk);
        }
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    if let (Some(hash), Some(hasher)) = (hash, hasher) {
        let actual_hash = hasher.finalize();
        if hash != actual_hash {
            tokio::fs::remove_file(&path).await?;
            return Err(Error::HashMismatch);
        }
    }
    Ok(())
}

const WOLFPACK_UA: HeaderValue =
    HeaderValue::from_static(concat!("Wolfpack/", env!("CARGO_PKG_VERSION")));
