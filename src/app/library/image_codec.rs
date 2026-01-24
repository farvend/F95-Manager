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

pub struct RealImageCodec;

impl ImageCodec for RealImageCodec {
    fn decode(&self, bytes: &[u8]) -> Result<ImageData, ImageCodecError> {
        let img = image::load_from_memory(bytes)
            .map_err(|e| ImageCodecError::DecodeFailed(e.to_string()))?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Ok(ImageData::new(w, h, rgba.into_vec()))
    }

    fn encode(&self, data: &ImageData) -> Result<Vec<u8>, ImageCodecError> {
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        image::write_buffer_with_format(
            &mut cursor,
            &data.rgba,
            data.width,
            data.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|e| ImageCodecError::EncodeFailed(e.to_string()))?;
        Ok(buf)
    }
}
