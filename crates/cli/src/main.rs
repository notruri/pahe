use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use inquire::{Select, Text};
use owo_colors::OwoColorize;
use pahe::{EpisodeVariant, PaheBuilder, PaheError};
use pahe_downloader::{DownloadConfig, download, suggest_filename};

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
    /// AnimePahe series URL
    #[arg(short, long)]
    series: Option<String>,

    /// Cookies used to authenticate pahe requests
    #[arg(short, long, env = "PAHE_COOKIES")]
    cookies: Option<String>,

    /// Episode to fetch variants for (1-indexed)
    #[arg(short, long, default_value_t = 1)]
    episode: i32,

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
    #[arg(long)]
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
    #[arg(short = 'n', long, default_value_t = 8)]
    connections: usize,

    #[command(flatten)]
    resolve: ResolveArgs,
}

#[derive(Debug, Clone)]
struct RuntimeArgs {
    series: String,
    cookies: String,
    episode: i32,
    quality: String,
    lang: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            _ => None,
        }
    }
}

struct CliLogger {
    level: LogLevel,
}

impl CliLogger {
    fn new(level: &str) -> pahe::Result<Self> {
        let level = LogLevel::parse(level).ok_or(PaheError::Message(format!(
            "invalid log level: {level}. expected one of: error, warn, info, debug"
        )))?;

        Ok(Self { level })
    }

    fn log(&self, level: LogLevel, message: impl AsRef<str>) {
        let bullet = self.bullet();

        if level <= self.level {
            println!("\n{}{}", bullet, message.as_ref());
        }
    }

    fn info(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Info, message);
    }

    fn debug(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Debug, message);
    }

    fn bullet(&self) -> Box<dyn std::fmt::Display> {
        match self.level {
            LogLevel::Info => Box::new(" * ".green()),
            LogLevel::Error => Box::new(" * ".red()),
            LogLevel::Warn => Box::new(" * ".yellow()),
            LogLevel::Debug => Box::new(" * ".purple()),
        }
    }
}

#[tokio::main]
async fn main() -> pahe::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Resolve(args)) => run_resolve(args).await,
        Some(Commands::Download(args)) => run_download(args).await,
        None => run_resolve(cli.resolve).await,
    }
}

async fn run_resolve(args: ResolveArgs) -> pahe::Result<()> {
    let logger = CliLogger::new(&args.log_level)?;
    let resolved = resolve_episode_url(args, &logger).await?;

    logger.info("stream:");
    logger.info(resolved.yellow().to_string());
    Ok(())
}

async fn run_download(args: DownloadArgs) -> pahe::Result<()> {
    let logger = CliLogger::new(&args.resolve.log_level)?;

    let url = match args.url {
        Some(url) => {
            logger.info("using direct url provided by --url");
            url
        }
        None => {
            logger.info("resolving episode link before download");
            resolve_episode_url(args.resolve, &logger).await?
        }
    };

    let file_name: PathBuf = match args.output {
        Some(path) => path.into(),
        None => {
            let guessed = suggest_filename(&url).await.map_err(|err| {
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

    logger.info(format!(
        "downloading with {} connection(s) to {}",
        args.connections,
        output_str.yellow()
    ));

    download(DownloadConfig::new(url, output).connections(args.connections))
        .await
        .map_err(|err| PaheError::Message(format!("download failed: {err}")))?;

    logger.info("download complete");
    Ok(())
}

async fn resolve_episode_url(args: ResolveArgs, logger: &CliLogger) -> pahe::Result<String> {
    let runtime = if args.interactive || args.series.is_none() || args.cookies.is_none() {
        prompt_for_args(args)?
    } else {
        RuntimeArgs {
            series: args.series.expect("series checked as Some"),
            cookies: args.cookies.expect("cookies checked as Some"),
            episode: args.episode,
            quality: args.quality,
            lang: args.lang,
        }
    };

    logger.info("initializing");
    let pahe = PaheBuilder::new().cookies_str(&runtime.cookies).build()?;

    logger.info(format!("getting info from: {}", runtime.series.yellow()));
    let info = pahe.get_series_metadata(&runtime.series).await?;
    logger.info(format!(
        "title: {}",
        info.title
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
            .yellow()
    ));

    logger.info(format!("retrieving {} episodes", runtime.episode.yellow()));
    let links = pahe
        .fetch_series_episode_links(&info.id, runtime.episode, runtime.episode)
        .await?;

    let play_link = links
        .first()
        .ok_or(PaheError::EpisodeNotFound(runtime.episode))?;

    let variants = pahe.fetch_episode_variants(play_link).await?;
    let selected = select_quality(variants, &runtime.quality, &runtime.lang, logger)?;
    let quality = format!("{}", selected.resolution);

    logger.info(format!("language: {}", selected.lang.yellow()));
    logger.info(format!("quality: {}", quality.yellow()));
    logger.info(format!("bluray: {}", selected.bluray.yellow()));

    logger.info("resolving stream link");
    let resolved = pahe.resolve_direct_link(&selected).await?;

    Ok(resolved.direct_link)
}

fn prompt_for_args(args: ResolveArgs) -> pahe::Result<RuntimeArgs> {
    let series_default = args.series.unwrap_or_default();
    let cookies_default = args.cookies.unwrap_or_default();

    let series = Text::new("Series URL:")
        .with_placeholder("https://animepahe.ru/anime/...")
        .with_initial_value(&series_default)
        .prompt()
        .map_err(|err| PaheError::Message(format!("failed to read series URL: {err}")))?;

    let cookies = Text::new("Cookies:")
        .with_help_message("You can also set this via PAHE_COOKIES")
        .with_initial_value(&cookies_default)
        .prompt()
        .map_err(|err| PaheError::Message(format!("failed to read cookies: {err}")))?;

    let episode_input = Text::new("Episode number:")
        .with_initial_value(&args.episode.to_string())
        .prompt()
        .map_err(|err| PaheError::Message(format!("failed to read episode: {err}")))?;

    let episode = episode_input
        .trim()
        .parse::<i32>()
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
        episode,
        quality,
        lang,
    })
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
) -> pahe::Result<EpisodeVariant> {
    let filtered: Vec<EpisodeVariant> = variants
        .iter()
        .filter(|variant| match audio_lang {
            "en" => variant.lang == "eng",
            "jp" => variant.lang == "jp",
            "zh" => variant.lang == "zh",
            _ => true,
        })
        .cloned()
        .collect();

    let pool = if filtered.is_empty() {
        variants
    } else {
        filtered
    };

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
