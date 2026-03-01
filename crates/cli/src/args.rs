use std::path::PathBuf;
use std::str::FromStr;

use clap::Args;

use crate::constants::*;

#[derive(Debug, Clone, Args)]
pub struct AppArgs {
    /// Logging verbosity (error, warn, info, debug)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Use interactive prompts to edit arguments before execution
    #[arg(short, long)]
    pub interactive: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ResolveArgs {
    /// AnimePahe anime/play url or uuid
    #[arg(short, long)]
    pub series: Option<String>,

    /// Cookies used to authenticate pahe requests
    #[arg(short, long, env = "PAHE_COOKIES")]
    pub cookies: Option<String>,

    /// Episode range (1-indexed) or a session id/play URL
    #[arg(short, long, default_value = "1")]
    pub episodes: EpisodeRange,

    /// Quality to select (e.g. 1080p, 720p, highest, lowest)
    #[arg(short, long, default_value = "highest")]
    pub quality: String,

    /// Audio language code to select (e.g. jp, en)
    #[arg(short, long, default_value = "jp")]
    pub lang: String,

    #[command(flatten)]
    pub app_args: AppArgs,
}

#[derive(Debug, Clone, Args)]
pub struct DownloadArgs {
    /// Output path for downloaded file
    #[arg(short, long)]
    pub output: Option<String>,

    /// Output directory for downloaded files
    #[arg(short, long)]
    pub dir: Option<PathBuf>,

    /// Number of parallel connections
    #[arg(short = 'n', long, default_value_t = 1)]
    pub connections: usize,

    #[command(flatten)]
    pub resolve: ResolveArgs,
}

#[derive(Debug, Clone)]
pub struct RuntimeArgs {
    pub series: String,
    pub cookies: String,
    pub episodes: EpisodeRange,
    pub quality: String,
    pub lang: String,
}

impl RuntimeArgs {
    pub fn new(
        series: String,
        cookies: String,
        episodes: EpisodeRange,
        quality: String,
        lang: String,
    ) -> Self {
        Self {
            series,
            cookies,
            episodes,
            quality,
            lang,
        }
    }
}

#[derive(Debug, Clone)]
pub enum EpisodeRange {
    Range {
        start: i32,
        end: i32,
    },
    Session {
        anime_id: Option<String>,
        session_id: String,
    },
}

impl FromStr for EpisodeRange {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let input = s.trim();

        if let Some(caps) = PLAY_LINK_RE.captures(input) {
            let anime_id = caps.get(1).map(|m| m.as_str().to_string());
            let session_id = caps
                .get(2)
                .map(|m| m.as_str().to_string())
                .ok_or("invalid play url")?;
            return Ok(EpisodeRange::Session {
                anime_id,
                session_id,
            });
        }

        if SESSION_ID_RE.is_match(input) {
            return Ok(EpisodeRange::Session {
                anime_id: None,
                session_id: input.to_string(),
            });
        }

        if let Some((start, end)) = input.split_once('-') {
            let start: i32 = start.parse().map_err(|_| "invalid start")?;
            let end: i32 = end.parse().map_err(|_| "invalid end")?;

            if start > end {
                return Err("start cannot be greater than end".into());
            }

            Ok(EpisodeRange::Range { start, end })
        } else {
            let value: i32 = input.parse().map_err(|_| "invalid number/session id/url")?;
            Ok(EpisodeRange::Range {
                start: value,
                end: value,
            })
        }
    }
}

impl std::fmt::Display for EpisodeRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EpisodeRange::Range { start, end } => {
                if start == end {
                    write!(f, "{start}")
                } else {
                    write!(f, "{start}-{end}")
                }
            }
            EpisodeRange::Session {
                anime_id: Some(anime_id),
                session_id,
            } => write!(f, "https://{ANIMEPAHE_DOMAIN}/play/{anime_id}/{session_id}"),
            EpisodeRange::Session {
                anime_id: None,
                session_id,
            } => write!(f, "{session_id}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_episode_range_number() {
        let parsed = "12".parse::<EpisodeRange>().expect("must parse number");
        assert!(matches!(parsed, EpisodeRange::Range { start: 12, end: 12 }));
    }

    #[test]
    fn parse_episode_range_span() {
        let parsed = "2-5".parse::<EpisodeRange>().expect("must parse range");
        assert!(matches!(parsed, EpisodeRange::Range { start: 2, end: 5 }));
    }

    #[test]
    fn parse_episode_session_id() {
        let parsed = "3cf1e5860ff5e9f766b36241c4dd6d48de3ef45d41183ecd079e1772aeb27c3c"
            .parse::<EpisodeRange>()
            .expect("must parse session id");
        assert!(matches!(
            parsed,
            EpisodeRange::Session { anime_id: None, .. }
        ));
    }

    #[test]
    fn parse_episode_play_url() {
        let parsed = format!(
            "https://{ANIMEPAHE_DOMAIN}/play/123e4567-e89b-12d3-a456-426614174000/3cf1e5860ff5e9f766b36241c4dd6d48de3ef45d41183ecd079e1772aeb27c3c"
        )
        .parse::<EpisodeRange>()
        .expect("must parse play url");
        assert!(matches!(
            parsed,
            EpisodeRange::Session {
                anime_id: Some(_),
                ..
            }
        ));
    }
}
