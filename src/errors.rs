use thiserror::Error;

use pahe_core::KwikError;

pub type Result<T> = std::result::Result<T, PaheError>;

#[derive(Debug, Error)]
pub enum PaheError {
    #[error("failed to parse animepahe base URL")]
    AnimepaheBaseUrl,

    #[error("failed building reqwest client: {0}")]
    BuildClient(#[source] reqwest::Error),

    #[error("kwik error: {0}")]
    Kwik(#[from] KwikError),

    #[error("invalid anime link; unable to parse anime id from {link}")]
    InvalidAnimeLink { link: String },

    #[error("HTTP request failed while {context}: {source}")]
    Request {
        context: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed to decode JSON while {context}: {source}")]
    Json {
        context: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed to read response body while {context}: {source}")]
    ResponseBody {
        context: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("{context} returned 403 Forbidden (DDoS-Guard). {hint}")]
    DdosGuard { context: String, hint: String },

    #[error("{context} returned {status}\nresponse text:\n{body}")]
    HttpStatus {
        context: String,
        status: reqwest::StatusCode,
        body: String,
    },

    #[error("regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("no pahe.win mirrors found in play page")]
    NoMirrors,

    #[error("no selectable variant found")]
    NoSelectableVariant,

    #[error("failed resolving direct link through kwik: {0}")]
    ResolveDirectLink(#[source] anyhow::Error),
    
    #[error("episode not found: {0}")]
    EpisodeNotFound(i32)
}
