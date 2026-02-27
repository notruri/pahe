use clap::Parser;
use inquire::{Select, Text};
use pahe::{EpisodeVariant, PaheBuilder, PaheError};
use tracing::{debug, info};
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
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

    /// Logging verbosity (error, warn, info, debug, trace)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Use interactive prompts to edit arguments before execution
    #[arg(long)]
    interactive: bool,
}

#[derive(Debug, Clone)]
struct RuntimeArgs {
    series: String,
    cookies: String,
    episode: i32,
    quality: String,
    lang: String,
}

#[tokio::main]
async fn main() -> pahe::Result<()> {
    let args = Args::parse();
    init_logging(&args.log_level)?;

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

    info!("building pahe client");
    let pahe = PaheBuilder::new().cookies_str(&runtime.cookies).build()?;

    info!(series = %runtime.series, "fetching series metadata");
    let info = pahe.get_series_metadata(&runtime.series).await?;

    info!(episode = runtime.episode, "fetching episode links");
    let links = pahe
        .fetch_series_episode_links(&info.id, runtime.episode, runtime.episode)
        .await?;

    let play_link = links
        .first()
        .ok_or(PaheError::EpisodeNotFound(runtime.episode))?;

    info!("fetching episode variants");
    let variants = pahe.fetch_episode_variants(play_link).await?;
    let selected = select_quality(variants, &runtime.quality, &runtime.lang)?;

    info!(
        resolution = selected.resolution,
        language = %selected.lang,
        "resolving direct link"
    );
    let resolved = pahe.resolve_direct_link(&selected).await?;

    println!("{}", resolved.direct_link);
    Ok(())
}

fn init_logging(level: &str) -> pahe::Result<()> {
    let env_filter = EnvFilter::try_new(level)
        .map_err(|_| PaheError::Message(format!("invalid log level: {level}")))?;

    fmt()
        .pretty()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    Ok(())
}

fn prompt_for_args(args: Args) -> pahe::Result<RuntimeArgs> {
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

    debug!(
        available_variants = pool.len(),
        quality, audio_lang, "selecting quality variant"
    );
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
