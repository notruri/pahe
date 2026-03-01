use std::time::Duration;

use pahe::errors::*;

use crate::constants::*;

pub fn estimate_eta(downloaded: u64, total: u64, elapsed: Duration) -> Option<Duration> {
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

pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let mins = secs / 60;
    let rem = secs % 60;
    format!("{mins:02}:{rem:02}")
}

pub fn format_bytes(bytes: u64) -> String {
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

pub fn format_bytes_f64(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes;
    let mut unit = 0usize;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{:.0} {}", value, UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

pub fn normalize_series_link(raw: &str) -> Result<String> {
    let input = raw.trim();
    if let Some(caps) = ANIME_LINK_RE.captures(input)
        && let Some(anime_id) = caps.get(1).map(|m| m.as_str())
    {
        return Ok(format!("https://{ANIMEPAHE_DOMAIN}/anime/{anime_id}"));
    }

    if let Some(caps) = PLAY_LINK_RE.captures(input)
        && let Some(anime_id) = caps.get(1).map(|m| m.as_str())
    {
        return Ok(format!("https://{ANIMEPAHE_DOMAIN}/anime/{anime_id}"));
    }

    Err(PaheError::Message(
        "invalid --series URL: expected AnimePahe /anime/<uuid> or /play/<uuid>/<session> link"
            .to_string(),
    ))
}
