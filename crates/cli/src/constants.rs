use std::sync::LazyLock;

use regex::Regex;

pub const ANIMEPAHE_DOMAIN: &str = "animepahe.si";

pub static UUID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-f0-9-]{36}$").expect("uuid regex must compile"));

pub static SESSION_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-f0-9]{32,}$").expect("session id regex must compile"));

pub static ANIME_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        format!(
            r"^https?://(?:www\.)?{}/anime/([a-f0-9-]{{36}})(?:[/?#].*)?$",
            regex::escape(ANIMEPAHE_DOMAIN)
        )
        .as_str(),
    )
    .expect("anime link regex must compile")
});

pub static PLAY_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        format!(
            r"^https?://(?:www\.)?{}/play/([a-f0-9-]{{36}})/([a-f0-9]{{32,}})(?:[/?#].*)?$",
            regex::escape(ANIMEPAHE_DOMAIN)
        )
        .as_str(),
    )
    .expect("play link regex must compile")
});
