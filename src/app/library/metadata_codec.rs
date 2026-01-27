use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum MetadataCodecError {
    DeserializationFailed(String),
    SerializationFailed(String),
}

impl std::fmt::Display for MetadataCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetadataCodecError::DeserializationFailed(msg) => {
                write!(f, "Metadata deserialization failed: {}", msg)
            }
            MetadataCodecError::SerializationFailed(msg) => {
                write!(f, "Metadata serialization failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for MetadataCodecError {}

/// Cached metadata structure matching the JSON format in cache/<id>/meta.json
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CachedThreadMeta {
    pub thread_id: u64,
    pub title: String,
    pub creator: String,
    pub version: String,
    pub cover_url: String,
    pub screens: Vec<String>,
    pub tag_ids: Vec<u32>,
}

pub trait MetadataCodec: Send + Sync {
    fn decode(&self, bytes: &[u8]) -> Result<CachedThreadMeta, MetadataCodecError>;
    fn encode(&self, meta: &CachedThreadMeta) -> Result<Vec<u8>, MetadataCodecError>;
}

pub struct RealMetadataCodec;

impl MetadataCodec for RealMetadataCodec {
    fn decode(&self, bytes: &[u8]) -> Result<CachedThreadMeta, MetadataCodecError> {
        let data = std::str::from_utf8(bytes)
            .map_err(|e| MetadataCodecError::DeserializationFailed(e.to_string()))?;

        let mut cached: CachedThreadMeta = serde_json::from_str(data)
            .map_err(|e| MetadataCodecError::DeserializationFailed(e.to_string()))?;

        // Filter screens to only include image files (handle legacy cache with .zip etc)
        let image_extensions = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
        cached.screens = cached
            .screens
            .into_iter()
            .filter(|url| {
                if let Some(ext_start) = url.rfind('.') {
                    let ext = &url[ext_start + 1..];
                    image_extensions.contains(&ext.to_lowercase().as_str())
                } else {
                    false
                }
            })
            .collect();

        Ok(cached)
    }

    fn encode(&self, meta: &CachedThreadMeta) -> Result<Vec<u8>, MetadataCodecError> {
        let json = serde_json::to_string_pretty(meta)
            .map_err(|e| MetadataCodecError::SerializationFailed(e.to_string()))?;
        Ok(json.into_bytes())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[derive(Clone)]
    pub struct MockMetadataCodec {
        pub should_fail_decode: bool,
        pub should_fail_encode: bool,
    }

    impl MockMetadataCodec {
        pub fn new() -> Self {
            Self {
                should_fail_decode: false,
                should_fail_encode: false,
            }
        }
    }

    impl MetadataCodec for MockMetadataCodec {
        fn decode(&self, bytes: &[u8]) -> Result<CachedThreadMeta, MetadataCodecError> {
            if self.should_fail_decode {
                return Err(MetadataCodecError::DeserializationFailed(
                    "Mock decode failure".to_string(),
                ));
            }

            let data = std::str::from_utf8(bytes)
                .map_err(|e| MetadataCodecError::DeserializationFailed(e.to_string()))?;
            serde_json::from_str(data)
                .map_err(|e| MetadataCodecError::DeserializationFailed(e.to_string()))
        }

        fn encode(&self, meta: &CachedThreadMeta) -> Result<Vec<u8>, MetadataCodecError> {
            if self.should_fail_encode {
                return Err(MetadataCodecError::SerializationFailed(
                    "Mock encode failure".to_string(),
                ));
            }

            let json = serde_json::to_string(meta)
                .map_err(|e| MetadataCodecError::SerializationFailed(e.to_string()))?;
            Ok(json.into_bytes())
        }
    }

    #[test]
    fn test_metadata_codec_trait_exists() {
        let codec = RealMetadataCodec;
        let meta = CachedThreadMeta {
            thread_id: 123,
            title: "Test".to_string(),
            creator: "Creator".to_string(),
            version: "1.0".to_string(),
            cover_url: "http://example.com/cover.png".to_string(),
            screens: vec!["http://example.com/screen1.png".to_string()],
            tag_ids: vec![1, 2, 3],
        };

        let encoded = codec.encode(&meta).expect("encode should succeed");
        let decoded = codec.decode(&encoded).expect("decode should succeed");
        assert_eq!(meta, decoded);
    }

    #[test]
    fn test_real_metadata_codec_roundtrip() {
        let codec = RealMetadataCodec;
        let original = CachedThreadMeta {
            thread_id: 456,
            title: "Test Game".to_string(),
            creator: "Dev Name".to_string(),
            version: "v2.0".to_string(),
            cover_url: "https://example.com/cover.jpg".to_string(),
            screens: vec![
                "https://example.com/screen1.png".to_string(),
                "https://example.com/screen2.jpg".to_string(),
            ],
            tag_ids: vec![10, 20, 30],
        };

        let encoded = codec.encode(&original).expect("encode failed");
        let decoded = codec.decode(&encoded).expect("decode failed");

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_real_metadata_codec_filters_non_image_screens() {
        let codec = RealMetadataCodec;
        let meta = CachedThreadMeta {
            thread_id: 789,
            title: "Test".to_string(),
            creator: "Creator".to_string(),
            version: "1.0".to_string(),
            cover_url: "http://example.com/cover.png".to_string(),
            screens: vec![
                "http://example.com/screen1.png".to_string(),
                "http://example.com/file.zip".to_string(),
                "http://example.com/screen2.jpg".to_string(),
                "http://example.com/doc.txt".to_string(),
            ],
            tag_ids: vec![1],
        };

        let encoded = codec.encode(&meta).expect("encode failed");
        let decoded = codec.decode(&encoded).expect("decode failed");

        // Should only have .png and .jpg files
        assert_eq!(decoded.screens.len(), 2);
        assert!(decoded.screens[0].ends_with(".png"));
        assert!(decoded.screens[1].ends_with(".jpg"));
    }

    #[test]
    fn test_mock_metadata_codec_can_inject_error() {
        let mut codec = MockMetadataCodec::new();
        let meta = CachedThreadMeta {
            thread_id: 1,
            title: "Test".to_string(),
            creator: "Creator".to_string(),
            version: "1.0".to_string(),
            cover_url: "http://example.com/cover.png".to_string(),
            screens: vec![],
            tag_ids: vec![],
        };

        // Test encode error
        codec.should_fail_encode = true;
        assert!(codec.encode(&meta).is_err());

        // Test decode error
        codec.should_fail_encode = false;
        codec.should_fail_decode = true;
        let bytes = b"{}";
        assert!(codec.decode(bytes).is_err());
    }
}
