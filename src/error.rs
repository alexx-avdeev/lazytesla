use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("token storage error: {0}")]
    Store(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("callback server error: {0}")]
    Callback(String),

    #[error("OAuth state mismatch")]
    StateMismatch,

    #[error("login timed out waiting for browser callback")]
    LoginTimeout,

    #[error("API error: {0}")]
    Api(String),
}

pub type Result<T> = std::result::Result<T, AppError>;