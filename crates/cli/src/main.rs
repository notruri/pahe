use std::{
    future::Future,
    io::Write,
    path::PathBuf,
    str::FromStr,
    sync::{
        LazyLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use clap::{Args, Parser, Subcommand};
use crossterm::{
    cursor, execute,
    terminal::{Clear, ClearType},
};
use inquire::{Select, Text};
use owo_colors::OwoColorize;
use pahe::prelude::*;
use pahe_downloader::{DownloadEvent, DownloadRequest, download, suggest_filename};
use regex::Regex;

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
    #[arg(short = 'n', long, default_value_t = 8)]
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
    spinner_step: AtomicUsize,
    loading_active: AtomicBool,
    loading_padded: AtomicBool,
}

struct DownloadProgressRenderer {
    enabled: bool,
    initialized: bool,
    spinner_step: usize,
    total: Option<u64>,
}

impl DownloadProgressRenderer {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            initialized: false,
            spinner_step: 0,
            total: None,
        }
    }

    fn handle(&mut self, event: DownloadEvent) {
        if !self.enabled {
            return;
        }

        match event {
            DownloadEvent::Started { total_bytes, .. } => {
                self.total = total_bytes;
                self.draw_frame(0, total_bytes, Duration::ZERO, false);
            }
            DownloadEvent::Progress {
                downloaded_bytes,
                total_bytes,
                elapsed,
            } => {
                self.total = total_bytes;
                self.draw_frame(downloaded_bytes, total_bytes, elapsed, false);
            }
            DownloadEvent::Finished {
                downloaded_bytes,
                elapsed,
            } => {
                self.draw_frame(downloaded_bytes, self.total, elapsed, true);
            }
        }
    }

    fn draw_frame(&mut self, downloaded: u64, total: Option<u64>, elapsed: Duration, done: bool) {
        let mut stdout = std::io::stdout();

        if !self.initialized {
            let _ = writeln!(stdout);
            let _ = writeln!(stdout);
            self.initialized = true;
        }

        let spinner = if done {
            "✓"
        } else {
            const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let frame = FRAMES[self.spinner_step % FRAMES.len()];
            self.spinner_step = self.spinner_step.wrapping_add(1);
            frame
        };

        let ratio = total
            .map(|total_bytes| {
                if total_bytes == 0 {
                    1.0
                } else {
                    downloaded as f64 / total_bytes as f64
                }
            })
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);

        let bar_width = 45.0;
        let filled = (ratio * bar_width).round();
        let empty = bar_width - filled;
        let bar = format!(
            "[{}{}]",
            "█".repeat(filled as usize),
            " ".repeat(empty as usize)
        );

        let speed_bps = if elapsed.as_secs_f64() > 0.0 {
            downloaded as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };
        let speed_text = format!("{}/s", format_bytes_f64(speed_bps));

        let eta = total.and_then(|total_bytes| estimate_eta(downloaded, total_bytes, elapsed));
        let downloaded_text = format_bytes(downloaded);
        let total_text = total
            .map(format_bytes)
            .unwrap_or_else(|| "unknown".to_string());
        let eta_text = eta
            .map(format_duration)
            .unwrap_or_else(|| "--:--".to_string());

        let spinner = spinner.cyan();
        let bar = bar.green();
        let downloaded_text = downloaded_text.yellow();
        let total_text = total_text.dimmed();
        let eta_text = eta_text.magenta();

        let _ = execute!(stdout, cursor::MoveUp(2), Clear(ClearType::CurrentLine));
        let _ = writeln!(stdout, "[{spinner}] {bar}  eta {eta_text}");
        let _ = writeln!(
            stdout,
            "{downloaded_text:>14} / {total_text:<14}  {speed_text:>30}"
        );
        let _ = stdout.flush();
    }
}

#[derive(Debug, Clone, Copy)]
enum LogState {
    Success,
    Failed,
    Debug,
}

impl CliLogger {
    fn new(level: &str) -> Result<Self> {
        let level = LogLevel::parse(level).ok_or(PaheError::Message(format!(
            "invalid log level: {level}. expected one of: error, warn, info, debug"
        )))?;

        Ok(Self {
            level,
            spinner_step: AtomicUsize::new(0),
            loading_active: AtomicBool::new(false),
            loading_padded: AtomicBool::new(false),
        })
    }

    fn log(&self, level: LogLevel, state: LogState, message: impl AsRef<str>) {
        self.clear_loading_line_if_needed();
        let icon = self.icon(state);

        if level <= self.level {
            println!("{} {}", icon, message.as_ref());
        }
    }

    fn loading(&self, message: impl AsRef<str>) {
        if LogLevel::Info > self.level {
            return;
        }

        self.draw_loading_frame(message.as_ref());
    }

    fn success(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Info, LogState::Success, message);
    }

    fn failed(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Error, LogState::Failed, message);
    }

    fn debug(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Debug, LogState::Debug, message);
    }

    fn icon(&self, state: LogState) -> Box<dyn std::fmt::Display> {
        match state {
            LogState::Success => Box::new("[✓]".green()),
            LogState::Failed => Box::new("[✗]".red()),
            LogState::Debug => Box::new("[λ]".purple()),
        }
    }

    async fn while_loading<F, T>(&self, message: impl Into<String>, future: F) -> T
    where
        F: Future<Output = T>,
    {
        if LogLevel::Info > self.level {
            return future.await;
        }

        let message = message.into();
        let mut ticker = tokio::time::interval(Duration::from_millis(120));
        let mut future = Box::pin(future);
        self.loading_active.store(true, Ordering::Relaxed);

        loop {
            tokio::select! {
                result = &mut future => {
                    self.clear_loading_line_if_needed();
                    return result;
                }
                _ = ticker.tick() => {
                    self.draw_loading_frame(&message);
                }
            }
        }
    }

    fn draw_loading_frame(&self, message: &str) {
        const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let idx = self.spinner_step.fetch_add(1, Ordering::Relaxed);
        let frame = FRAMES[idx % FRAMES.len()].yellow();
        let mut stdout = std::io::stdout();

        if !self.loading_padded.swap(true, Ordering::Relaxed) {
            let _ = writeln!(stdout);
        }

        self.loading_active.store(true, Ordering::Relaxed);
        let _ = execute!(
            stdout,
            cursor::MoveToColumn(0),
            Clear(ClearType::CurrentLine)
        );
        let _ = write!(stdout, "{} {}", frame, message);
        let _ = stdout.flush();
    }

    fn clear_loading_line_if_needed(&self) {
        if self.loading_active.swap(false, Ordering::Relaxed) {
            let mut stdout = std::io::stdout();
            let _ = execute!(
                stdout,
                cursor::MoveToColumn(0),
                Clear(ClearType::CurrentLine)
            );
            if self.loading_padded.load(Ordering::Relaxed) {
                let _ = execute!(
                    stdout,
                    cursor::MoveUp(1),
                    cursor::MoveToColumn(0),
                    Clear(ClearType::CurrentLine)
                );
            }
            let _ = stdout.flush();
            self.loading_padded.store(false, Ordering::Relaxed);
        }
    }
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
        logger.success(format!(
            "episode {}: {}",
            i + 1,
            episode_url.url.yellow().to_string()
        ));
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

        download(
            DownloadRequest::new(episode_url.referer, episode_url.url, output)
                .connections(args.connections),
            |event| progress_renderer.handle(event),
        )
        .await
        .map_err(|err| PaheError::Message(format!("download failed: {err}")))?;
        logger.success(format!("done {}", output_str.yellow()));
    }

    logger.success("download complete");
    Ok(())
}

async fn resolve_episode_urls(args: ResolveArgs, logger: &CliLogger) -> Result<Vec<EpisodeURL>> {
    let mut runtime = if args.interactive || args.series.is_none() || args.cookies.is_none() {
        prompt_for_args(args)?
    } else {
        RuntimeArgs {
            series: args.series.expect("series checked as Some"),
            cookies: args.cookies.expect("cookies checked as Some"),
            episodes: args.episodes,
            quality: args.quality,
            lang: args.lang,
        }
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
                pahe.fetch_episode_variants(&link),
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

fn format_bytes_f64(bytes: f64) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_series_link_accepts_anime_link() {
        let input = format!("https://{ANIMEPAHE_DOMAIN}/anime/123e4567-e89b-12d3-a456-426614174000");
        let normalized = normalize_series_link(&input).expect("anime link should be valid");
        assert_eq!(
            normalized,
            format!("https://{ANIMEPAHE_DOMAIN}/anime/123e4567-e89b-12d3-a456-426614174000")
        );
    }

    #[test]
    fn normalize_series_link_accepts_play_link() {
        let input = format!("https://{ANIMEPAHE_DOMAIN}/play/123e4567-e89b-12d3-a456-426614174000/3cf1e5860ff5e9f766b36241c4dd6d48de3ef45d41183ecd079e1772aeb27c3c");
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
