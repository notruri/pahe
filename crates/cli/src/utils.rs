use std::time::Duration;

use pahe::errors::*;

use crate::constants::*;

#[derive(Debug, Clone)]
pub struct NormalizedSeriesInput {
    pub anime_id: String,
    pub anime_link: String,
    pub session_id: Option<String>,
}

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
    Ok(normalize_series_input(raw)?.anime_link)
}

pub fn normalize_series_input(raw: &str) -> Result<NormalizedSeriesInput> {
    let input = raw.trim();
    let normalized = input
        .strip_prefix("https://")
        .or_else(|| input.strip_prefix("http://"))
        .unwrap_or(input);
    let normalized = normalized.strip_prefix("www.").unwrap_or(normalized);
    let normalized = normalized
        .strip_prefix(ANIMEPAHE_DOMAIN)
        .unwrap_or(normalized);
    let normalized = normalized.strip_prefix('/').unwrap_or(normalized);

    if UUID_RE.is_match(input) {
        return Ok(NormalizedSeriesInput {
            anime_id: input.to_string(),
            anime_link: format!("https://{ANIMEPAHE_DOMAIN}/anime/{input}"),
            session_id: None,
        });
    }

    if let Some((anime_id, session_id)) = normalized.split_once('/')
        && UUID_RE.is_match(anime_id)
        && SESSION_ID_RE.is_match(session_id)
    {
        return Ok(NormalizedSeriesInput {
            anime_id: anime_id.to_string(),
            anime_link: format!("https://{ANIMEPAHE_DOMAIN}/anime/{anime_id}"),
            session_id: Some(session_id.to_string()),
        });
    }

    if let Some(play_path) = normalized.strip_prefix("play/")
        && let Some((anime_id, session_id)) = play_path.split_once('/')
        && UUID_RE.is_match(anime_id)
        && SESSION_ID_RE.is_match(session_id)
    {
        return Ok(NormalizedSeriesInput {
            anime_id: anime_id.to_string(),
            anime_link: format!("https://{ANIMEPAHE_DOMAIN}/anime/{anime_id}"),
            session_id: Some(session_id.to_string()),
        });
    }

    if let Some(anime_id) = normalized.strip_prefix("anime/")
        && UUID_RE.is_match(anime_id)
    {
        return Ok(NormalizedSeriesInput {
            anime_id: anime_id.to_string(),
            anime_link: format!("https://{ANIMEPAHE_DOMAIN}/anime/{anime_id}"),
            session_id: None,
        });
    }

    if let Some(caps) = ANIME_LINK_RE.captures(input)
        && let Some(anime_id) = caps.get(1).map(|m| m.as_str())
    {
        return Ok(NormalizedSeriesInput {
            anime_id: anime_id.to_string(),
            anime_link: format!("https://{ANIMEPAHE_DOMAIN}/anime/{anime_id}"),
            session_id: None,
        });
    }

    if let Some(caps) = PLAY_LINK_RE.captures(input)
        && let Some(anime_id) = caps.get(1).map(|m| m.as_str())
        && let Some(session_id) = caps.get(2).map(|m| m.as_str())
    {
        return Ok(NormalizedSeriesInput {
            anime_id: anime_id.to_string(),
            anime_link: format!("https://{ANIMEPAHE_DOMAIN}/anime/{anime_id}"),
            session_id: Some(session_id.to_string()),
        });
    }

    Err(PaheError::Message(
        "invalid --series value: expected anime id/url or anime+session id/url".to_string(),
    ))
}
