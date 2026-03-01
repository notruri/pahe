use owo_colors::OwoColorize;

use pahe::client::EpisodeVariant;
use pahe::errors::*;
use pahe::prelude::PaheBuilder;

use crate::args::*;
use crate::constants::*;
use crate::logger::*;
use crate::prompt::*;
use crate::utils::*;

#[derive(Debug, Clone)]
pub struct EpisodeURL {
    pub referer: String,
    pub url: String,
}

enum QualityPreference {
    Highest,
    Lowest,
    Exact(i32),
}

pub async fn resolve_episode_urls(
    args: ResolveArgs,
    logger: &CliLogger,
) -> Result<Vec<EpisodeURL>> {
    let mut runtime = match args {
        args if args.app_args.interactive => prompt_for_args(args)?,
        ResolveArgs {
            series: Some(series),
            cookies: Some(cookies),
            episodes,
            quality,
            lang,
            ..
        } => RuntimeArgs::new(series, cookies, episodes, quality, lang),
        args => prompt_for_args(args)?,
    };
    let normalized_series = normalize_series_input(&runtime.series)?;
    runtime.series = normalized_series.anime_link.clone();
    if let Some(session_id) = normalized_series.session_id {
        runtime.episodes = EpisodeRange::Session {
            anime_id: Some(normalized_series.anime_id),
            session_id,
        };
    }

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

    let links = match &runtime.episodes {
        EpisodeRange::Range { start, end } => {
            logger
                .while_loading(
                    format!("retrieving {} episodes", (end - start).yellow()),
                    pahe.fetch_series_episode_links(&info.id, *start, *end),
                )
                .await?
        }
        EpisodeRange::Session {
            anime_id,
            session_id,
        } => {
            let anime_id = anime_id.as_deref().unwrap_or(&info.id);
            let link = format!("https://{ANIMEPAHE_DOMAIN}/play/{anime_id}/{session_id}");
            let episode = pahe.fetch_episode_index(&link).await?;
            vec![(episode, link)]
        }
    };

    if links.is_empty() {
        return match runtime.episodes {
            EpisodeRange::Range { start, .. } => Err(PaheError::EpisodeNotFound(start)),
            EpisodeRange::Session { .. } => Err(PaheError::Message(
                "episode not found for given session input".to_string(),
            )),
        };
    }

    let mut results = Vec::new();

    for (n, link) in links.iter() {
        logger.loading(format!("processing episode {}", n.yellow()));
        logger.debug("episode", format!("link: {}", link.yellow()));

        let variants = logger
            .while_loading(
                format!("fetching variants for episode {}", n.yellow()),
                pahe.fetch_episode_variants(link),
            )
            .await?;
        let selected = select_quality(variants, &runtime.quality, &runtime.lang, logger)?;
        let quality = format!("{}p", selected.resolution);
        let resolved = logger
            .while_loading(
                format!("resolving direct link for episode {}", n.yellow()),
                pahe.resolve_direct_link(&selected),
            )
            .await?;

        results.push(EpisodeURL {
            referer: resolved.referer,
            url: resolved.direct_link,
        });

        logger.success(format!("episode: {}", n.yellow()));
        logger.success(format!("language: {}", selected.lang.yellow()));
        logger.success(format!("quality: {}", quality.yellow()));
        logger.success(format!("bluray: {}", selected.bluray.yellow()));
    }

    Ok(results)
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

    logger.debug(
        "episode",
        format!(
            "selecting quality from {} variant(s) with quality={} and lang={}",
            pool.len(),
            quality,
            audio_lang
        ),
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
