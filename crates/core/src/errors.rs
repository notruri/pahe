use thiserror::Error;

pub type Result<T> = std::result::Result<T, KwikError>;

#[derive(Debug, Error)]
pub enum KwikError {
    #[error("request client build failed while {context}: {source}")]
    BuildClient {
        context: &'static str,
        #[source]
        source: reqwest::Error,
    },

    #[error("request failed while {context}: {source}")]
    Request {
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

    #[error("{context} returned {status}\nresponse text:\n{body}")]
    HttpStatus {
        context: String,
        status: reqwest::StatusCode,
        body: String,
    },

    #[error("missing redirect location header")]
    MissingRedirectLocation,

    #[error("invalid base index {base} for alphabet key")]
    InvalidAlphabetBaseIndex { base: usize },

    #[error("failed to extract kwik post link")]
    MissingKwikPostLink,

    #[error("failed to extract _token")]
    MissingToken,

    #[error("invalid offset")]
    InvalidOffset,

    #[error("invalid base")]
    InvalidBase,

    #[error("kwik retry limit exceeded for {link}")]
    RetryLimitExceeded { link: String },

    #[error("unable to extract kwik link from pahe page")]
    MissingKwikLink,

    #[error("regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("number parse error: {0}")]
    ParseInt(#[from] std::num::ParseIntError),
}
