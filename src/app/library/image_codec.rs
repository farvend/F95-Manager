use super::ImageData;

#[derive(Debug)]
pub enum ImageCodecError {
    DecodeFailed(String),
    EncodeFailed(String),
}

impl std::fmt::Display for ImageCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageCodecError::DecodeFailed(msg) => write!(f, "Image decode failed: {}", msg),
            ImageCodecError::EncodeFailed(msg) => write!(f, "Image encode failed: {}", msg),
        }
    }
}

impl std::error::Error for ImageCodecError {}

pub trait ImageCodec: Send + Sync {
    fn decode(&self, bytes: &[u8]) -> Result<ImageData, ImageCodecError>;
    fn encode(&self, data: &ImageData) -> Result<Vec<u8>, ImageCodecError>;
}
