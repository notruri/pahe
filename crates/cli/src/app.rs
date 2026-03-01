use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use owo_colors::OwoColorize;

use pahe::prelude::*;
use pahe_downloader::*;

use crate::episode::*;
use crate::logger::*;
use crate::progress::*;

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub resolve: ResolveArgs,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Resolve and print a direct episode download URL
    Resolve(ResolveArgs),
    /// Download a file URL in parallel (wget-like)
    Download(DownloadArgs),
}

#[derive(Debug, Clone, Args)]
pub struct ResolveArgs {
    /// AnimePahe anime or play URL
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

    /// Logging verbosity (error, warn, info, debug)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Use interactive prompts to edit arguments before execution
    #[arg(short, long)]
    pub interactive: bool,
}

#[derive(Debug, Clone, Args)]
pub struct DownloadArgs {
    /// Direct URL to download. If omitted, resolves from pahe using resolve args.
    #[arg(short, long)]
    url: Option<String>,

    /// Output path for downloaded file
    #[arg(short, long)]
    output: Option<String>,

    /// Output directory for downloaded files
    #[arg(short, long)]
    dir: Option<PathBuf>,

    /// Number of parallel connections
    #[arg(short = 'n', long, default_value_t = 1)]
    connections: usize,

    #[command(flatten)]
    resolve: ResolveArgs,
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

impl Cli {
    pub fn log_level(&self) -> &str {
        match &self.command {
            Some(Commands::Resolve(args)) => &args.log_level,
            Some(Commands::Download(args)) => &args.resolve.log_level,
            None => &self.resolve.log_level,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EpisodeRange {
    pub start: i32,
    pub end: i32,
}

#[derive(Debug)]
pub struct App {
    cli: Cli,
    logger: CliLogger,
}

impl App {
    pub fn new() -> Self {
        let cli = Cli::parse();
        let logger = CliLogger::new(cli.log_level());
        Self { cli, logger }
    }

    pub async fn run(&self) {
        if let Err(err) = match &self.cli.command {
            Some(Commands::Resolve(args)) => self.resolve(args.clone()).await,
            Some(Commands::Download(args)) => self.download(args.clone()).await,
            None => self.resolve(self.cli.resolve.clone()).await,
        } {
            self.logger.failed(format!("{err}"));
        }
    }

    pub async fn resolve(&self, args: ResolveArgs) -> Result<()> {
        let logger = &self.logger;
        let resolves = resolve_episode_urls(args, logger).await?;

        logger.success("Episodes has been resolved successfully");
        for (i, episode_url) in resolves.iter().enumerate() {
            logger.success(format!("episode {}: {}", i + 1, episode_url.url.yellow()));
        }

        Ok(())
    }

    pub async fn download(&self, args: DownloadArgs) -> Result<()> {
        let logger = &self.logger;

        let urls = resolve_episode_urls(args.resolve, logger).await?;

        for episode_url in urls {
            let file_name: PathBuf = match &args.output {
                Some(path) => path.into(),
                None => {
                    let guessed = logger
                        .while_loading(
                            "inferring output filename",
                            suggest_filename(&episode_url.referer, &episode_url.url),
                        )
                        .await
                        .map_err(|err| {
                            PaheError::Message(format!("failed to infer output filename: {err}"))
                        })?;
                    guessed.into()
                }
            };

            let output = match &args.dir {
                Some(dir) => dir.join(file_name),
                None => file_name,
            };

            let output_str = output.to_string_lossy().into_owned();
            let mut progress_renderer =
                DownloadProgressRenderer::new(logger.level >= LogLevel::Info);
            let (events_tx, mut events_rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tick = tokio::time::interval(Duration::from_millis(80));
            let mut download_fut = std::pin::pin!(download(
                DownloadRequest::new(episode_url.referer, episode_url.url, output)
                    .connections(args.connections),
                move |event| {
                    let _ = events_tx.send(event);
                },
            ));

            let download_result = loop {
                tokio::select! {
                    result = &mut download_fut => break result,
                    maybe_event = events_rx.recv() => {
                        if let Some(event) = maybe_event {
                            progress_renderer.handle(event);
                        }
                    }
                    _ = tick.tick() => {
                        progress_renderer.tick();
                    }
                }
            };

            while let Ok(event) = events_rx.try_recv() {
                progress_renderer.handle(event);
            }

            download_result.map_err(|err| PaheError::Message(format!("download failed: {err}")))?;
            logger.success(format!("done {}", output_str.yellow()));
        }

        logger.success("download complete");
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use crate::constants::*;
    use crate::utils::*;

    #[test]
    fn normalize_series_link_accepts_anime_link() {
        let input =
            format!("https://{ANIMEPAHE_DOMAIN}/anime/123e4567-e89b-12d3-a456-426614174000");
        let normalized = normalize_series_link(&input).expect("anime link should be valid");
        assert_eq!(
            normalized,
            format!("https://{ANIMEPAHE_DOMAIN}/anime/123e4567-e89b-12d3-a456-426614174000")
        );
    }

    #[test]
    fn normalize_series_link_accepts_play_link() {
        let input = format!(
            "https://{ANIMEPAHE_DOMAIN}/play/123e4567-e89b-12d3-a456-426614174000/3cf1e5860ff5e9f766b36241c4dd6d48de3ef45d41183ecd079e1772aeb27c3c"
        );
        let normalized = normalize_series_link(&input).expect("play link should be valid");
        assert_eq!(
            normalized,
            format!("https://{ANIMEPAHE_DOMAIN}/anime/123e4567-e89b-12d3-a456-426614174000")
        );
    }

    #[test]
    fn normalize_series_link_rejects_non_animepahe_links() {
        let err =
            normalize_series_link("https://example.com/anime/123e4567-e89b-12d3-a456-426614174000")
                .expect_err("non animepahe links should be rejected");
        assert!(
            err.to_string()
                .contains("invalid --series URL: expected AnimePahe")
        );
    }
}
