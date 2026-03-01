use inquire::*;
use pahe::errors::*;

use crate::args::*;

pub fn prompt_for_args(args: ResolveArgs) -> Result<RuntimeArgs> {
    let series_default = args.series.unwrap_or_default();

    let series = Text::new("series:")
        .with_help_message("anime id/url or episode id/url")
        .with_initial_value(&series_default)
        .prompt()
        .map_err(|err| PaheError::Message(format!("failed to read series URL: {err}")))?;

    let cookies = if let Some(cookies) = args.cookies {
        cookies
    } else {
        Text::new("cookies:")
            .with_help_message("you can also set this via PAHE_COOKIES environment variable")
            .prompt()
            .map_err(|err| PaheError::Message(format!("failed to read cookies: {err}")))?
    };

    let episode_input = Text::new("episodes:")
        .with_help_message(r#"a number (e.g. 12) or a range (e.g. 1-12) or a comma separated list (e.g. 1,2,3) or a id/url to the episode"#)
        .with_initial_value(&args.episodes.to_string())
        .prompt()
        .map_err(|err| PaheError::Message(format!("failed to read episode: {err}")))?;

    let episodes = episode_input.trim().parse::<EpisodeRange>().map_err(|_| {
        PaheError::Message("episode must be a valid number/list/range/url".to_string())
    })?;

    let quality_choices = vec!["highest", "1080p", "720p", "480p", "lowest", "custom"];
    let quality_choice = Select::new("preferred quality:", quality_choices)
        .with_starting_cursor(0)
        .prompt()
        .map_err(|err| PaheError::Message(format!("failed to read quality: {err}")))?;

    let quality = if quality_choice == "custom" {
        Text::new("custom quality:")
            .with_initial_value(&args.quality)
            .with_help_message("(e.g. 900p, highest)")
            .prompt()
            .map_err(|err| PaheError::Message(format!("failed to read custom quality: {err}")))?
    } else {
        quality_choice.to_string()
    };

    let lang_options = vec!["jp", "en", "zh", "any"];
    let lang = Select::new("preferred audio language:", lang_options)
        .with_help_message("it can be jp, en, zh, or any")
        .with_starting_cursor(0)
        .prompt()
        .map_err(|err| PaheError::Message(format!("failed to read language: {err}")))?
        .to_string();

    Ok(RuntimeArgs::new(series, cookies, episodes, quality, lang))
}
