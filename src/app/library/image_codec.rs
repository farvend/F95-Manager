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

#[cfg(test)]
pub mod tests {
    use super::*;

    #[derive(Clone)]
    pub struct MockImageCodec {
        pub should_fail_decode: bool,
        pub should_fail_encode: bool,
    }

    impl MockImageCodec {
        pub fn new() -> Self {
            Self {
                should_fail_decode: false,
                should_fail_encode: false,
            }
        }
    }

    impl ImageCodec for MockImageCodec {
        fn decode(&self, bytes: &[u8]) -> Result<ImageData, ImageCodecError> {
            if self.should_fail_decode {
                return Err(ImageCodecError::DecodeFailed(
                    "Mock decode failure".to_string(),
                ));
            }

            if bytes.len() < 8 {
                return Err(ImageCodecError::DecodeFailed("Invalid format".to_string()));
            }

            let width = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            let height = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            let rgba = bytes[8..].to_vec();

            if rgba.len() != (width * height * 4) as usize {
                return Err(ImageCodecError::DecodeFailed("Size mismatch".to_string()));
            }

            Ok(ImageData::new(width, height, rgba))
        }

        fn encode(&self, data: &ImageData) -> Result<Vec<u8>, ImageCodecError> {
            if self.should_fail_encode {
                return Err(ImageCodecError::EncodeFailed(
                    "Mock encode failure".to_string(),
                ));
            }

            let mut bytes = Vec::new();
            bytes.extend_from_slice(&data.width.to_le_bytes());
            bytes.extend_from_slice(&data.height.to_le_bytes());
            bytes.extend_from_slice(&data.rgba);
            Ok(bytes)
        }
    }
}
