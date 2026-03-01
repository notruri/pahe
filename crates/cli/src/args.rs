use std::path::PathBuf;
use std::str::FromStr;

use clap::Args;

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

    /// Episodes to fetch variants for (1-indexed)
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
pub struct EpisodeRange {
    pub start: i32,
    pub end: i32,
}

impl FromStr for EpisodeRange {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if let Some((start, end)) = s.split_once('-') {
            let start: i32 = start.parse().map_err(|_| "invalid start")?;
            let end: i32 = end.parse().map_err(|_| "invalid end")?;

            if start > end {
                return Err("start cannot be greater than end".into());
            }

            Ok(EpisodeRange { start, end })
        } else {
            let value: i32 = s.parse().map_err(|_| "invalid number")?;
            Ok(EpisodeRange {
                start: value,
                end: value,
            })
        }
    }
}

impl std::fmt::Display for EpisodeRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.start == self.end {
            write!(f, "{}", self.start)
        } else {
            write!(f, "{}-{}", self.start, self.end)
        }
    }
}
