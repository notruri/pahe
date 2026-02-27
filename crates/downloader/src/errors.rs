use thiserror::Error;

pub type Result<T> = std::result::Result<T, DownloaderError>;

#[derive(Debug, Error)]
pub enum DownloaderError {
    #[error("http request failed while {context}: {source}")]
    Request {
        context: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("{context} returned HTTP {status}")]
    HttpStatus {
        context: String,
        status: reqwest::StatusCode,
    },

    #[error("io error while {context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}
