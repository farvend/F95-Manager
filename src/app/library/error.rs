use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("network error: {0}")]
    Network(String),

    #[error("image decode error: {0}")]
    Decode(String),

    #[error("invalid screen index: {index}, card has {total} screens")]
    InvalidScreenIndex { index: usize, total: usize },

    #[error("cache error: {0}")]
    Cache(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
