mod logger;
mod progress;
mod utils;

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use inquire::{Select, Text};
use owo_colors::OwoColorize;
use pahe::prelude::*;
use pahe_downloader::{DownloadRequest, download, suggest_filename};
use regex::Regex;

use crate::logger::{CliLogger, LogLevel};
use crate::progress::DownloadProgressRenderer;

const ANIMEPAHE_DOMAIN: &str = "animepahe.si";

static ANIME_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        format!(
            r"^https?://(?:www\.)?{}/anime/([a-f0-9-]{{36}})(?:[/?#].*)?$",
            regex::escape(ANIMEPAHE_DOMAIN)
        )
        .as_str(),
    )
    .expect("anime link regex must compile")
});
static PLAY_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        format!(
            r"^https?://(?:www\.)?{}/play/([a-f0-9-]{{36}})/[a-f0-9]{{32,}}(?:[/?#].*)?$",
            regex::escape(ANIMEPAHE_DOMAIN)
        )
        .as_str(),
    )
    .expect("play link regex must compile")
});

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    resolve: ResolveArgs,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Resolve and print a direct episode download URL
    Resolve(ResolveArgs),
    /// Download a file URL in parallel (wget-like)
    Download(DownloadArgs),
}

#[derive(Debug, Clone, Args)]
struct ResolveArgs {
    /// AnimePahe anime or play URL
    #[arg(short, long)]
    series: Option<String>,

    /// Cookies used to authenticate pahe requests
    #[arg(short, long, env = "PAHE_COOKIES")]
    cookies: Option<String>,

    /// Episodes to fetch variants for (1-indexed)
    #[arg(short, long, default_value = "1")]
    episodes: EpisodeRange,

    /// Quality to select (e.g. 1080p, 720p, highest, lowest)
    #[arg(short, long, default_value = "highest")]
    quality: String,

    /// Audio language code to select (e.g. jp, en)
    #[arg(short, long, default_value = "jp")]
    lang: String,

    /// Logging verbosity (error, warn, info, debug)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Use interactive prompts to edit arguments before execution
    #[arg(short, long)]
    interactive: bool,
}

#[derive(Debug, Clone, Args)]
struct DownloadArgs {
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
struct RuntimeArgs {
    series: String,
    cookies: String,
    episodes: EpisodeRange,
    quality: String,
    lang: String,
}

impl Cli {
    fn log_level(&self) -> &str {
        match &self.command {
            Some(Commands::Resolve(args)) => &args.log_level,
            Some(Commands::Download(args)) => &args.resolve.log_level,
            None => &self.resolve.log_level,
        }
    }
}

#[derive(Debug, Clone)]
struct EpisodeRange {
    start: i32,
    end: i32,
}

#[derive(Debug, Clone)]
struct EpisodeURL {
    referer: String,
    url: String,
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let logger = CliLogger::new(cli.log_level()).unwrap_or(CliLogger {
        level: LogLevel::Error,
        spinner_step: AtomicUsize::new(0),
        loading_active: AtomicBool::new(false),
        loading_padded: AtomicBool::new(false),
    });

    let result = match cli.command {
        Some(Commands::Resolve(args)) => run_resolve(args).await,
        Some(Commands::Download(args)) => run_download(args).await,
        None => run_resolve(cli.resolve).await,
    };

    if let Err(err) = result {
        logger.failed(format!("{err}"));
        std::process::exit(1);
    }
}

async fn run_resolve(args: ResolveArgs) -> Result<()> {
    let logger = CliLogger::new(&args.log_level)?;
    let resolves = resolve_episode_urls(args, &logger).await?;

    logger.success("Episodes has been resolved successfully");
    for (i, episode_url) in resolves.iter().enumerate() {
        logger.success(format!("episode {}: {}", i + 1, episode_url.url.yellow()));
    }

    Ok(())
}

async fn run_download(args: DownloadArgs) -> Result<()> {
    let logger = CliLogger::new(&args.resolve.log_level)?;

    let urls = resolve_episode_urls(args.resolve, &logger).await?;

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
        let mut progress_renderer = DownloadProgressRenderer::new(logger.level >= LogLevel::Info);
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

async fn resolve_episode_urls(args: ResolveArgs, logger: &CliLogger) -> Result<Vec<EpisodeURL>> {
    let mut runtime = match args {
        args if args.interactive => prompt_for_args(args)?,
        ResolveArgs {
            series: Some(series),
            cookies: Some(cookies),
            episodes,
            quality,
            lang,
            ..
        } => RuntimeArgs {
            series,
            cookies,
            episodes,
            quality,
            lang,
        },
        args => prompt_for_args(args)?,
    };
    runtime.series = normalize_series_link(&runtime.series)?;

    logger.loading("initializing");
    let pahe = PaheBuilder::new().cookies_str(&runtime.cookies).build()?;
    logger.success("initialized");

    let info = logger
        .while_loading(
            format!("getting info from: {}", runtime.series.yellow()),
            pahe.get_series_metadata(&runtime.series),
        )
        .await?;
    logger.success(format!(
        "title: {}",
        info.title
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
            .trim()
            .yellow()
    ));

    let links = logger
        .while_loading(
            format!(
                "retrieving {} episodes",
                (runtime.episodes.end - runtime.episodes.start).yellow()
            ),
            pahe.fetch_series_episode_links(&info.id, runtime.episodes.start, runtime.episodes.end),
        )
        .await?;

    if links.is_empty() {
        return Err(PaheError::EpisodeNotFound(runtime.episodes.start));
    }

    let mut results = Vec::new();

    for (i, link) in links.iter().enumerate() {
        logger.loading(format!("processing episode {}", (i + 1).yellow()));
        logger.debug(format!("link: {}", link.yellow()));

        let variants = logger
            .while_loading(
                format!("fetching variants for episode {}", (i + 1).yellow()),
                pahe.fetch_episode_variants(link),
            )
            .await?;
        let selected = select_quality(variants, &runtime.quality, &runtime.lang, logger)?;
        let quality = format!("{}p", selected.resolution);
        let resolved = logger
            .while_loading(
                format!("resolving direct link for episode {}", (i + 1).yellow()),
                pahe.resolve_direct_link(&selected),
            )
            .await?;

        results.push(EpisodeURL {
            referer: resolved.referer,
            url: resolved.direct_link,
        });

        logger.success(format!("episode: {}", (i + 1).yellow()));
        logger.success(format!("language: {}", selected.lang.yellow()));
        logger.success(format!("quality: {}", quality.yellow()));
        logger.success(format!("bluray: {}", selected.bluray.yellow()));
    }

    Ok(results)
}

fn prompt_for_args(args: ResolveArgs) -> Result<RuntimeArgs> {
    let series_default = args.series.unwrap_or_default();
    let cookies_default = args.cookies.unwrap_or_default();

    let series = Text::new("Series URL:")
        .with_placeholder(format!("https://{ANIMEPAHE_DOMAIN}/anime/... or /play/...").as_ref())
        .with_initial_value(&series_default)
        .prompt()
        .map_err(|err| PaheError::Message(format!("failed to read series URL: {err}")))?;

    let cookies = Text::new("Cookies:")
        .with_help_message("You can also set this via PAHE_COOKIES")
        .with_initial_value(&cookies_default)
        .prompt()
        .map_err(|err| PaheError::Message(format!("failed to read cookies: {err}")))?;

    let episode_input = Text::new("Episodes:")
        .with_initial_value(&args.episodes.to_string())
        .prompt()
        .map_err(|err| PaheError::Message(format!("failed to read episode: {err}")))?;

    let episodes = episode_input
        .trim()
        .parse::<EpisodeRange>()
        .map_err(|_| PaheError::Message("episode must be a valid number".to_string()))?;

    let quality_choices = vec!["highest", "1080p", "720p", "480p", "lowest", "custom"];
    let quality_choice = Select::new("Preferred quality:", quality_choices)
        .with_starting_cursor(0)
        .prompt()
        .map_err(|err| PaheError::Message(format!("failed to read quality: {err}")))?;

    let quality = if quality_choice == "custom" {
        Text::new("Custom quality (e.g. 900p, highest):")
            .with_initial_value(&args.quality)
            .prompt()
            .map_err(|err| PaheError::Message(format!("failed to read custom quality: {err}")))?
    } else {
        quality_choice.to_string()
    };

    let lang_options = vec!["jp", "en", "zh", "any"];
    let lang = Select::new("Preferred audio language:", lang_options)
        .with_starting_cursor(0)
        .prompt()
        .map_err(|err| PaheError::Message(format!("failed to read language: {err}")))?
        .to_string();

    Ok(RuntimeArgs {
        series,
        cookies,
        episodes,
        quality,
        lang,
    })
}

fn normalize_series_link(raw: &str) -> Result<String> {
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

enum QualityPreference {
    Highest,
    Lowest,
    Exact(i32),
}

fn select_quality(
    variants: Vec<EpisodeVariant>,
    quality: &str,
    audio_lang: &str,
    logger: &CliLogger,
) -> Result<EpisodeVariant> {
    let pool: Vec<EpisodeVariant> = variants
        .iter()
        .filter(|variant| match audio_lang {
            "en" => variant.lang == "en",
            "jp" => variant.lang == "jp",
            "zh" => variant.lang == "zh",
            "any" => true,
            _ => false,
        })
        .cloned()
        .collect();

    if pool.is_empty() {
        return Err(PaheError::NoSelectableVariant);
    }

    logger.debug(format!(
        "Selecting quality from {} variant(s) with quality={} and lang={}",
        pool.len(),
        quality,
        audio_lang
    ));

    let preference = parse_quality(quality).ok_or(PaheError::NoSelectableVariant)?;

    let selected = match preference {
        QualityPreference::Highest => pool.into_iter().max_by_key(|variant| variant.resolution),
        QualityPreference::Lowest => pool.into_iter().min_by_key(|variant| variant.resolution),
        QualityPreference::Exact(target) => pool
            .iter()
            .find(|variant| variant.resolution == target)
            .cloned()
            .or_else(|| pool.into_iter().max_by_key(|variant| variant.resolution)),
    };

    selected.ok_or(PaheError::NoSelectableVariant)
}

fn parse_quality(raw_quality: &str) -> Option<QualityPreference> {
    let normalized = raw_quality.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "highest" => Some(QualityPreference::Highest),
        "lowest" => Some(QualityPreference::Lowest),
        _ => {
            let digits = normalized.trim_end_matches('p');
            digits.parse::<i32>().ok().map(QualityPreference::Exact)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
