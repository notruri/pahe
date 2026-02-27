mod errors;

use std::collections::BTreeMap;
use std::path::Path;

pub use errors::{DownloaderError, Result};
use reqwest::{Client, StatusCode, header};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub url: String,
    pub output: String,
    pub connections: usize,
}

impl DownloadConfig {
    pub fn new(url: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            output: output.into(),
            connections: 8,
        }
    }

    pub fn connections(mut self, connections: usize) -> Self {
        self.connections = connections.max(1);
        self
    }
}

pub async fn download(config: DownloadConfig) -> Result<()> {
    let client = Client::new();

    let head =
        client
            .head(&config.url)
            .send()
            .await
            .map_err(|source| DownloaderError::Request {
                context: "sending HEAD request".to_string(),
                source,
            })?;

    let size = head
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let accepts_ranges = head
        .headers()
        .get(header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("bytes"));

    if size.is_none() || !accepts_ranges {
        return single_stream_download(&client, &config.url, &config.output).await;
    }

    parallel_download(
        &client,
        &config.url,
        &config.output,
        size.unwrap_or(0),
        config.connections,
    )
    .await
}

async fn single_stream_download(client: &Client, url: &str, output: &str) -> Result<()> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|source| DownloaderError::Request {
            context: "sending GET request".to_string(),
            source,
        })?;

    if !response.status().is_success() {
        return Err(DownloaderError::HttpStatus {
            context: "downloading file".to_string(),
            status: response.status(),
        });
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|source| DownloaderError::Request {
            context: "reading response body".to_string(),
            source,
        })?;

    ensure_parent_dir(output).await?;
    let mut file = File::create(output)
        .await
        .map_err(|source| DownloaderError::Io {
            context: format!("creating output file {output}"),
            source,
        })?;

    file.write_all(&bytes)
        .await
        .map_err(|source| DownloaderError::Io {
            context: format!("writing output file {output}"),
            source,
        })?;

    Ok(())
}

async fn parallel_download(
    client: &Client,
    url: &str,
    output: &str,
    total_size: u64,
    connections: usize,
) -> Result<()> {
    if total_size == 0 {
        return single_stream_download(client, url, output).await;
    }

    let workers = connections.max(1).min(total_size as usize);
    let chunk_size = total_size.div_ceil(workers as u64);
    let (tx, mut rx) = mpsc::channel::<Result<(usize, Vec<u8>)>>(workers);

    for idx in 0..workers {
        let start = idx as u64 * chunk_size;
        if start >= total_size {
            continue;
        }
        let end = ((idx as u64 + 1) * chunk_size).min(total_size) - 1;
        let client = client.clone();
        let url = url.to_string();
        let tx = tx.clone();

        tokio::spawn(async move {
            let result = fetch_chunk(client, url, idx, start, end).await;
            let _ = tx.send(result).await;
        });
    }

    drop(tx);

    ensure_parent_dir(output).await?;
    let mut file = File::create(output)
        .await
        .map_err(|source| DownloaderError::Io {
            context: format!("creating output file {output}"),
            source,
        })?;

    let mut next = 0usize;
    let mut pending = BTreeMap::new();

    while let Some(msg) = rx.recv().await {
        let (idx, bytes) = msg?;
        pending.insert(idx, bytes);

        while let Some(bytes) = pending.remove(&next) {
            file.write_all(&bytes)
                .await
                .map_err(|source| DownloaderError::Io {
                    context: format!("writing output file {output}"),
                    source,
                })?;
            next += 1;
        }
    }

    Ok(())
}

async fn fetch_chunk(
    client: Client,
    url: String,
    idx: usize,
    start: u64,
    end: u64,
) -> Result<(usize, Vec<u8>)> {
    let range = format!("bytes={start}-{end}");
    let response = client
        .get(&url)
        .header(header::RANGE, range)
        .send()
        .await
        .map_err(|source| DownloaderError::Request {
            context: format!("downloading chunk {idx}"),
            source,
        })?;

    if response.status() != StatusCode::PARTIAL_CONTENT && !response.status().is_success() {
        return Err(DownloaderError::HttpStatus {
            context: format!("downloading chunk {idx}"),
            status: response.status(),
        });
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|source| DownloaderError::Request {
            context: format!("reading chunk {idx}"),
            source,
        })?;

    Ok((idx, bytes.to_vec()))
}

async fn ensure_parent_dir(output: &str) -> Result<()> {
    let Some(parent) = Path::new(output).parent() else {
        return Ok(());
    };

    if parent.as_os_str().is_empty() {
        return Ok(());
    }

    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|source| DownloaderError::Io {
            context: format!("creating output directory {}", parent.display()),
            source,
        })
}
