use inquire::*;
use pahe::errors::*;

use crate::args::*;
use crate::constants::*;

pub fn prompt_for_args(args: ResolveArgs) -> Result<RuntimeArgs> {
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

    Ok(RuntimeArgs::new(series, cookies, episodes, quality, lang))
}
