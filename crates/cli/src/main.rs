use clap::Parser;
use pahe::PaheBuilder;
use pahe::PaheError;

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    /// AnimePahe series URL
    #[arg(long)]
    series: String,

    /// Cookies used to authenticate pahe requests
    #[arg(long)]
    cookies: String,

    /// Episode to fetch variants for (1-indexed)
    #[arg(long, default_value_t = 1)]
    episode: i32,

    /// Variant index to select
    #[arg(long, default_value_t = 0)]
    variant: i32,

    /// Audio language code to select (e.g. jp, en)
    #[arg(long, default_value = "jp")]
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
    let selected = pahe.select_variant(variants, args.variant, &args.lang)?;
    let resolved = pahe.resolve_direct_link(&selected).await?;

    println!("{}", resolved.direct_link);
    Ok(())
}
