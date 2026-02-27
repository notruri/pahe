mod errors;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use errors::{DownloaderError, Result};
use owo_colors::OwoColorize;
use reqwest::{Client, StatusCode, header};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::time;

#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub url: String,
    pub output: PathBuf,
    pub connections: usize,
}

impl DownloadConfig {
    pub fn new(url: impl Into<String>, output: PathBuf) -> Self {
        Self {
            url: url.into(),
            output: output,
            connections: 8,
        }
    }

    pub fn connections(mut self, connections: usize) -> Self {
        self.connections = connections.max(1);
        self
    }
}

pub async fn suggest_filename(url: &str) -> Result<String> {
    let client = Client::new();
    suggest_filename_with_client(&client, url).await
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
        return single_stream_download(&client, &config.url, &config.output, size).await;
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

async fn suggest_filename_with_client(client: &Client, url: &str) -> Result<String> {
    let response = client
        .head(url)
        .send()
        .await
        .map_err(|source| DownloaderError::Request {
            context: "requesting filename metadata".to_string(),
            source,
        })?;

    if !response.status().is_success() {
        return Err(DownloaderError::HttpStatus {
            context: "requesting filename metadata".to_string(),
            status: response.status(),
        });
    }

    if let Some(content_disposition) = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        && let Some(filename) = parse_content_disposition_filename(content_disposition)
    {
        return Ok(filename);
    }

    Ok(filename_from_url(url))
}

fn parse_content_disposition_filename(content_disposition: &str) -> Option<String> {
    for segment in content_disposition.split(';').map(str::trim) {
        if let Some(value) = segment.strip_prefix("filename*=UTF-8''") {
            let decoded = percent_decode_filename(value);
            if !decoded.is_empty() {
                return Some(decoded);
            }
        }

        if let Some(value) = segment.strip_prefix("filename=") {
            let clean = value.trim_matches('"').trim();
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }

    None
}

fn percent_decode_filename(value: &str) -> String {
    let mut bytes = Vec::with_capacity(value.len());
    let mut iter = value.as_bytes().iter().copied();

    while let Some(b) = iter.next() {
        if b == b'%' {
            let hi = iter.next();
            let lo = iter.next();
            if let (Some(hi), Some(lo)) = (hi, lo)
                && let (Some(hi), Some(lo)) = (hex_value(hi), hex_value(lo))
            {
                bytes.push((hi << 4) | lo);
                continue;
            }
            bytes.push(b'%');
            if let Some(hi) = hi {
                bytes.push(hi);
            }
            if let Some(lo) = lo {
                bytes.push(lo);
            }
            continue;
        }

        bytes.push(b);
    }

    String::from_utf8_lossy(&bytes).to_string()
}

fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn filename_from_url(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| {
            url.path_segments()
                .and_then(|mut segments| segments.next_back().map(str::to_string))
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "download.bin".to_string())
}

async fn single_stream_download(
    client: &Client,
    url: &str,
    output: &Path,
    total_size: Option<u64>,
) -> Result<()> {
    let output_str = output.to_string_lossy();
    let mut response = client
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

    ensure_parent_dir(output).await?;
    let mut file = File::create(output)
        .await
        .map_err(|source| DownloaderError::Io {
            context: format!("creating output file {output_str}"),
            source,
        })?;

    let mut progress = ProgressRenderer::new(total_size);

    loop {
        let maybe_chunk = response
            .chunk()
            .await
            .map_err(|source| DownloaderError::Request {
                context: "reading response body".to_string(),
                source,
            })?;

        let Some(chunk) = maybe_chunk else {
            break;
        };

        file.write_all(&chunk)
            .await
            .map_err(|source| DownloaderError::Io {
                context: format!("writing output file {output_str}"),
                source,
            })?;

        progress.advance(chunk.len() as u64);
        progress.draw(false);
    }

    progress.draw(true);

    Ok(())
}

async fn parallel_download(
    client: &Client,
    url: &str,
    output: &Path,
    total_size: u64,
    connections: usize,
) -> Result<()> {
    let output_str = output.to_string_lossy();
    if total_size == 0 {
        return single_stream_download(client, url, output, Some(total_size)).await;
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
            context: format!("creating output file {output_str}"),
            source,
        })?;

    let mut next = 0usize;
    let mut pending = BTreeMap::new();
    let mut downloaded = 0u64;
    let mut progress = ProgressRenderer::new(Some(total_size));
    let mut ticker = time::interval(Duration::from_millis(120));

    loop {
        tokio::select! {
            biased;
            maybe_msg = rx.recv() => {
                let Some(msg) = maybe_msg else {
                    break;
                };
                let (idx, bytes) = msg?;
                pending.insert(idx, bytes);

                while let Some(bytes) = pending.remove(&next) {
                    file.write_all(&bytes)
                        .await
                        .map_err(|source| DownloaderError::Io {
                            context: format!("writing output file {output_str}"),
                            source,
                        })?;
                    downloaded += bytes.len() as u64;
                    progress.set(downloaded);
                    progress.draw(false);
                    next += 1;
                }
            }
            _ = ticker.tick() => {
                progress.draw(false);
            }
        }
    }

    progress.draw(true);

    Ok(())
}

struct ProgressRenderer {
    total: Option<u64>,
    downloaded: u64,
    started_at: Instant,
    spinner_step: usize,
}

impl ProgressRenderer {
    fn new(total: Option<u64>) -> Self {
        Self {
            total,
            downloaded: 0,
            started_at: Instant::now(),
            spinner_step: 0,
        }
    }

    fn advance(&mut self, bytes: u64) {
        self.downloaded = self.downloaded.saturating_add(bytes);
    }

    fn set(&mut self, bytes: u64) {
        self.downloaded = bytes;
    }

    fn draw(&mut self, done: bool) {
        let spinner = if done {
            "✓"
        } else {
            const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let frame = FRAMES[self.spinner_step % FRAMES.len()];
            self.spinner_step = self.spinner_step.wrapping_add(1);
            frame
        };

        let ratio = self
            .total
            .map(|total| {
                if total == 0 {
                    1.0
                } else {
                    self.downloaded as f64 / total as f64
                }
            })
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);

        let filled = (ratio * 30.0).round() as usize;
        let empty = 30 - filled;
        let bar = format!("[{}{}]", "█".repeat(filled), " ".repeat(empty));

        let eta = self
            .total
            .and_then(|total| estimate_eta(self.downloaded, total, self.started_at.elapsed()));

        let downloaded = format_bytes(self.downloaded);
        let total = self
            .total
            .map(format_bytes)
            .unwrap_or_else(|| "unknown".to_string());
        let eta_text = eta
            .map(format_duration)
            .unwrap_or_else(|| "--:--".to_string());

        let spinner = spinner.cyan();
        let bar = bar.green();
        let downloaded = downloaded.yellow();
        let total = total.dimmed();
        let eta_text = eta_text.magenta();

        eprint!(
            "\x1b[1A\x1b[2K\n\r{spinner:>4} {bar}  {downloaded:>10} / {total:<10}  eta {eta_text}"
        );

        if done {
            eprintln!();
        }
    }
}

fn estimate_eta(downloaded: u64, total: u64, elapsed: Duration) -> Option<Duration> {
    if downloaded == 0 || total <= downloaded || elapsed.is_zero() {
        return None;
    }

    let speed = downloaded as f64 / elapsed.as_secs_f64();
    if speed <= 0.0 {
        return None;
    }

    let remaining = (total - downloaded) as f64 / speed;
    Some(Duration::from_secs_f64(remaining.max(0.0)))
}

fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let mins = secs / 60;
    let rem = secs % 60;
    format!("{mins:02}:{rem:02}")
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
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

async fn ensure_parent_dir(output: &Path) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::{filename_from_url, parse_content_disposition_filename};

    #[test]
    fn parses_quoted_filename() {
        let value = "attachment; filename=\"episode01.mkv\"";
        assert_eq!(
            parse_content_disposition_filename(value).as_deref(),
            Some("episode01.mkv")
        );
    }

    #[test]
    fn parses_utf8_encoded_filename() {
        let value = "attachment; filename*=UTF-8''Spy%20x%20Family%20S01E01.mp4";
        assert_eq!(
            parse_content_disposition_filename(value).as_deref(),
            Some("Spy x Family S01E01.mp4")
        );
    }

    #[test]
    fn gets_filename_from_url_path() {
        assert_eq!(
            filename_from_url("https://cdn.example.com/videos/file-01.mp4?token=123"),
            "file-01.mp4"
        );
    }
}
