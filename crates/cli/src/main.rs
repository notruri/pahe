use clap::Parser;
use pahe::EpisodeVariant;
use pahe::PaheBuilder;
use pahe::PaheError;

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    /// AnimePahe series URL
    #[arg(short, long)]
    series: String,

    /// Cookies used to authenticate pahe requests
    #[arg(short, long)]
    cookies: String,

    /// Episode to fetch variants for (1-indexed)
    #[arg(short, long, default_value_t = 1)]
    episode: i32,

    /// Quality to select (e.g. 1080p, 720p, highest, lowest)
    #[arg(short, long, default_value = "highest")]
    quality: String,

    /// Audio language code to select (e.g. jp, en)
    #[arg(short, long, default_value = "jp")]
    lang: String,
}

#[tokio::main]
async fn main() -> pahe::Result<()> {
    let args = Args::parse();

    let pahe = PaheBuilder::new().cookies_str(&args.cookies).build()?;

    let links = pahe
        .fetch_series_episode_links(&args.series, args.episode, args.episode)
        .await?;

    let play_link = links
        .first()
        .ok_or(PaheError::EpisodeNotFound(args.episode))?;

    let variants = pahe.fetch_episode_variants(play_link).await?;
    let selected = select_quality(variants, &args.quality, &args.lang)?;
    let resolved = pahe.resolve_direct_link(&selected).await?;

    println!("{}", resolved.direct_link);
    Ok(())
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
