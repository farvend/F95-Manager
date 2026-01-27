use async_trait::async_trait;
use std::path::{Path, PathBuf};
use url::Url;

use super::{
    CachedThreadMeta, FileSystem, ImageCodec, ImageCodecError, ImageData, LibraryCard,
    MetadataCodec, MetadataCodecError, ProviderError, RealFileSystem, RealImageCodec,
    RealMetadataCodec,
};

pub type ProductionCachingProvider<P> =
    CachingProvider<P, RealFileSystem, RealImageCodec, RealMetadataCodec>;

#[derive(Debug)]
pub enum CacheError {
    CreateDirFailed(std::io::Error),
    EncodeFailed(ImageCodecError),
    WriteFailed(std::io::Error),
    MetadataEncodeFailed(MetadataCodecError),
    MetadataWriteFailed(std::io::Error),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::CreateDirFailed(e) => write!(f, "Failed to create cache directory: {}", e),
            CacheError::EncodeFailed(e) => write!(f, "Failed to encode image: {}", e),
            CacheError::WriteFailed(e) => write!(f, "Failed to write to cache: {}", e),
            CacheError::MetadataEncodeFailed(e) => {
                write!(f, "Failed to encode metadata: {}", e)
            }
            CacheError::MetadataWriteFailed(e) => {
                write!(f, "Failed to write metadata to cache: {}", e)
            }
        }
    }
}

impl std::error::Error for CacheError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CacheError::CreateDirFailed(e) => Some(e),
            CacheError::EncodeFailed(e) => Some(e),
            CacheError::WriteFailed(e) => Some(e),
            CacheError::MetadataEncodeFailed(e) => Some(e),
            CacheError::MetadataWriteFailed(e) => Some(e),
        }
    }
}

#[async_trait]
pub trait CardImageProvider: Send + Sync {
    async fn fetch_cover(&self, card: &LibraryCard) -> Result<ImageData, ProviderError>;
    async fn fetch_screen(
        &self,
        card: &LibraryCard,
        idx: usize,
    ) -> Result<ImageData, ProviderError>;
}

pub struct NetworkProvider;

impl NetworkProvider {
    pub fn new() -> Self {
        Self
    }

    async fn fetch_url(&self, url: &Url) -> Result<ImageData, ProviderError> {
        let url_str = url.as_str();
        let (w, h, rgba) = crate::parser::fetch_image_f95(url_str)
            .await
            .map_err(ProviderError::Network)?;
        Ok(ImageData::new(w as u32, h as u32, rgba))
    }
}

#[async_trait]
impl CardImageProvider for NetworkProvider {
    async fn fetch_cover(&self, card: &LibraryCard) -> Result<ImageData, ProviderError> {
        let url = card
            .cover_url
            .as_ref()
            .ok_or_else(|| ProviderError::Network("no cover url".to_string()))?;
        self.fetch_url(url).await
    }

    async fn fetch_screen(
        &self,
        card: &LibraryCard,
        idx: usize,
    ) -> Result<ImageData, ProviderError> {
        let url = card
            .screen_urls
            .get(idx)
            .ok_or(ProviderError::InvalidScreenIndex {
                index: idx,
                total: card.screen_urls.len(),
            })?;
        self.fetch_url(url).await
    }
}

pub struct CachingProvider<P, FS, IC, MC> {
    inner: P,
    cache_dir: PathBuf,
    fs: FS,
    codec: IC,
    metadata_codec: MC,
}

impl<P: Clone, FS: Clone, IC: Clone, MC: Clone> Clone for CachingProvider<P, FS, IC, MC> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            cache_dir: self.cache_dir.clone(),
            fs: self.fs.clone(),
            codec: self.codec.clone(),
            metadata_codec: self.metadata_codec.clone(),
        }
    }
}

impl<P: CardImageProvider, FS: FileSystem, IC: ImageCodec, MC: MetadataCodec>
    CachingProvider<P, FS, IC, MC>
{
    pub fn new(inner: P, cache_dir: PathBuf, fs: FS, codec: IC, metadata_codec: MC) -> Self {
        Self {
            inner,
            cache_dir,
            fs,
            codec,
            metadata_codec,
        }
    }

    fn cover_path(&self, card: &LibraryCard) -> PathBuf {
        let path = self
            .cache_dir
            .join(card.thread_id.to_string())
            .join("cover.png");
        log::trace!("Generated cover path for thread {}: {:?}", card.thread_id, path);
        path
    }

    fn screen_path(&self, card: &LibraryCard, idx: usize) -> PathBuf {
        let path = self
            .cache_dir
            .join(card.thread_id.to_string())
            .join(format!("screen_{}.png", idx + 1));
        log::trace!(
            "Generated screen path for thread {} idx {}: {:?}",
            card.thread_id,
            idx,
            path
        );
        path
    }

    fn meta_path(&self, thread_id: u64) -> PathBuf {
        self.cache_dir
            .join(thread_id.to_string())
            .join("meta.json")
    }

    pub async fn load_meta(&self, thread_id: u64) -> Option<CachedThreadMeta> {
        let path = self.meta_path(thread_id);
        log::debug!(
            "Metadata cache check for thread {}: {:?}",
            thread_id,
            path
        );

        if !self.fs.exists(&path).await {
            log::debug!(
                "Metadata cache miss (not exists) for thread {}",
                thread_id
            );
            return None;
        }

        let bytes = match self.fs.read(&path).await {
            Ok(b) => b,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!(
                        "Metadata cache read error for thread {}: {}",
                        thread_id,
                        e
                    );
                }
                return None;
            }
        };

        match self.metadata_codec.decode(&bytes) {
            Ok(data) => {
                log::debug!("Metadata cache hit for thread {}", thread_id);
                Some(data)
            }
            Err(e) => {
                log::warn!(
                    "Metadata cache decode error for thread {}: {}",
                    thread_id,
                    e
                );
                None
            }
        }
    }

    pub async fn save_meta(
        &self,
        thread_id: u64,
        meta: &CachedThreadMeta,
    ) -> Result<(), CacheError> {
        let path = self.meta_path(thread_id);

        if let Some(parent) = path.parent() {
            self.fs
                .create_dir_all(parent)
                .await
                .map_err(CacheError::CreateDirFailed)?;
        }

        let bytes = self
            .metadata_codec
            .encode(meta)
            .map_err(CacheError::MetadataEncodeFailed)?;
        self.fs
            .write(&path, &bytes)
            .await
            .map_err(CacheError::MetadataWriteFailed)?;

        log::debug!("Saved metadata cache for thread {}", thread_id);
        Ok(())
    }

    async fn load_from_cache(&self, path: &Path) -> Option<ImageData> {
        log::debug!("Cache check: {:?}", path);

        if !self.fs.exists(path).await {
            log::debug!("Cache miss (not exists): {:?}", path);
            return None;
        }

        let bytes = match self.fs.read(path).await {
            Ok(b) => b,
            Err(e) => {
                log::warn!("Cache read error: {:?}: {}", path, e);
                return None;
            }
        };

        match self.codec.decode(&bytes) {
            Ok(data) => {
                log::debug!("Cache hit: {:?}", path);
                Some(data)
            }
            Err(e) => {
                log::warn!("Cache decode error: {:?}: {}", path, e);
                None
            }
        }
    }

    async fn save_to_cache(&self, path: &Path, data: &ImageData) -> Result<(), CacheError> {
        if let Some(parent) = path.parent() {
            self.fs
                .create_dir_all(parent)
                .await
                .map_err(CacheError::CreateDirFailed)?;
        }

        let bytes = self.codec.encode(data).map_err(CacheError::EncodeFailed)?;
        self.fs
            .write(path, &bytes)
            .await
            .map_err(CacheError::WriteFailed)?;
        Ok(())
    }
}

#[async_trait]
impl<P: CardImageProvider, FS: FileSystem, IC: ImageCodec, MC: MetadataCodec> CardImageProvider
    for CachingProvider<P, FS, IC, MC>
{
    async fn fetch_cover(&self, card: &LibraryCard) -> Result<ImageData, ProviderError> {
        let path = self.cover_path(card);

        if let Some(cached) = self.load_from_cache(&path).await {
            return Ok(cached);
        }

        log::warn!("Cache miss for thread {}: cover", card.thread_id);

        let data = self.inner.fetch_cover(card).await?;
        if let Err(e) = self.save_to_cache(&path, &data).await {
            log::warn!(
                "Failed to save cover to cache for thread {}: {}",
                card.thread_id,
                e
            );
        }
        Ok(data)
    }

    async fn fetch_screen(
        &self,
        card: &LibraryCard,
        idx: usize,
    ) -> Result<ImageData, ProviderError> {
        let path = self.screen_path(card, idx);

        if let Some(cached) = self.load_from_cache(&path).await {
            return Ok(cached);
        }

        log::warn!(
            "Cache miss for thread {}: screen {}",
            card.thread_id,
            idx + 1
        );

        let data = self.inner.fetch_screen(card, idx).await?;
        if let Err(e) = self.save_to_cache(&path, &data).await {
            log::warn!(
                "Failed to save screen {} to cache for thread {}: {}",
                idx + 1,
                card.thread_id,
                e
            );
        }
        Ok(data)
    }
}

impl<P, FS, IC, MC> std::fmt::Debug for CachingProvider<P, FS, IC, MC> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachingProvider")
            .field("cache_dir", &self.cache_dir)
            .finish()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    pub fn test_card(thread_id: u64) -> LibraryCard {
        LibraryCard {
            thread_id,
            title: format!("Test Game {}", thread_id),
            creator: "Test Creator".to_string(),
            version: "1.0".to_string(),
            cover_url: Some(Url::parse("https://example.com/cover.png").unwrap()),
            screen_urls: vec![
                Url::parse("https://example.com/screen1.png").unwrap(),
                Url::parse("https://example.com/screen2.png").unwrap(),
            ],
            tags: vec![1, 2, 3],
            prefixes: vec![4, 5],
        }
    }

    pub fn test_image_data(width: u32, height: u32) -> ImageData {
        let rgba = vec![255u8; (width * height * 4) as usize];
        ImageData::new(width, height, rgba)
    }

    #[derive(Clone)]
    pub struct MockCardImageProvider {
        covers: Arc<HashMap<u64, ImageData>>,
        screens: Arc<HashMap<(u64, usize), ImageData>>,
        call_count: Arc<AtomicUsize>,
        should_fail: bool,
    }

    impl MockCardImageProvider {
        pub fn new() -> Self {
            Self {
                covers: Arc::new(HashMap::new()),
                screens: Arc::new(HashMap::new()),
                call_count: Arc::new(AtomicUsize::new(0)),
                should_fail: false,
            }
        }

        pub fn with_cover(thread_id: u64, data: ImageData) -> Self {
            let mut covers = HashMap::new();
            covers.insert(thread_id, data);
            Self {
                covers: Arc::new(covers),
                screens: Arc::new(HashMap::new()),
                call_count: Arc::new(AtomicUsize::new(0)),
                should_fail: false,
            }
        }

        pub fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl CardImageProvider for MockCardImageProvider {
        async fn fetch_cover(&self, card: &LibraryCard) -> Result<ImageData, ProviderError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            
            if self.should_fail {
                return Err(ProviderError::Network("Mock failure".to_string()));
            }

            self.covers
                .get(&card.thread_id)
                .cloned()
                .ok_or_else(|| ProviderError::Network("Cover not found".to_string()))
        }

        async fn fetch_screen(
            &self,
            card: &LibraryCard,
            idx: usize,
        ) -> Result<ImageData, ProviderError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            
            if self.should_fail {
                return Err(ProviderError::Network("Mock failure".to_string()));
            }

            self.screens
                .get(&(card.thread_id, idx))
                .cloned()
                .ok_or_else(|| ProviderError::Network("Screen not found".to_string()))
        }
    }

    #[cfg(test)]
    mod cache_tests {
        use super::*;
        use crate::app::library::fs::tests::MockFileSystem;
        use crate::app::library::image_codec::tests::MockImageCodec;
        use crate::app::library::metadata_codec::tests::MockMetadataCodec;
        use crate::app::library::metadata_codec::RealMetadataCodec;
        use std::path::PathBuf;

        #[tokio::test]
        async fn cache_hit_returns_cached_data_without_calling_inner() {
            let card = test_card(12345);
            let test_data = test_image_data(100, 100);
            let codec = MockImageCodec::new();
            
            let encoded = codec.encode(&test_data).unwrap();
            let cache_path = PathBuf::from("cache/12345/cover.png");
            let mock_fs = MockFileSystem::with_file(&cache_path, &encoded);
            
            let mock_provider = MockCardImageProvider::new();
            let caching: CachingProvider<
                MockCardImageProvider,
                MockFileSystem,
                MockImageCodec,
                MockMetadataCodec,
            > = CachingProvider::new(
                mock_provider.clone(),
                PathBuf::from("cache"),
                mock_fs.clone(),
                codec,
                MockMetadataCodec::new(),
            );

            let result = caching.fetch_cover(&card).await.unwrap();
            
            assert_eq!(result.width, test_data.width);
            assert_eq!(result.height, test_data.height);
            assert_eq!(mock_provider.call_count(), 0);
        }

        #[tokio::test]
        async fn cache_miss_fetches_from_inner_and_saves() {
            let card = test_card(12345);
            let test_data = test_image_data(100, 100);
            
            let mock_fs = MockFileSystem::new();
            let codec = MockImageCodec::new();
            let mock_provider = MockCardImageProvider::with_cover(12345, test_data.clone());
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec, MockMetadataCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    codec,
                    MockMetadataCodec::new(),
                );

            let result = caching.fetch_cover(&card).await.unwrap();
            
            assert_eq!(result.width, test_data.width);
            assert_eq!(result.height, test_data.height);
            assert_eq!(mock_provider.call_count(), 1);
            
            let cache_path = PathBuf::from("cache/12345/cover.png");
            assert!(mock_fs.get_file(&cache_path).is_some());
        }

        #[tokio::test]
        async fn screen_cache_hit() {
            let card = test_card(12345);
            let test_data = test_image_data(200, 150);
            let codec = MockImageCodec::new();
            
            let encoded = codec.encode(&test_data).unwrap();
            let cache_path = PathBuf::from("cache/12345/screen_1.png");
            let mock_fs = MockFileSystem::with_file(&cache_path, &encoded);
            
            let mock_provider = MockCardImageProvider::new();
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec, MockMetadataCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs,
                    codec,
                    MockMetadataCodec::new(),
                );

            let result = caching.fetch_screen(&card, 0).await.unwrap();
            
            assert_eq!(result.width, test_data.width);
            assert_eq!(result.height, test_data.height);
            assert_eq!(mock_provider.call_count(), 0);
        }

        #[tokio::test]
        async fn screen_cache_miss_and_save() {
            let card = test_card(12345);
            let test_data = test_image_data(200, 150);
            
            let mock_fs = MockFileSystem::new();
            let codec = MockImageCodec::new();
            
            let mut screens = HashMap::new();
            screens.insert((12345u64, 0usize), test_data.clone());
            let mock_provider = MockCardImageProvider {
                covers: Arc::new(HashMap::new()),
                screens: Arc::new(screens),
                call_count: Arc::new(AtomicUsize::new(0)),
                should_fail: false,
            };
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec, MockMetadataCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    codec,
                    MockMetadataCodec::new(),
                );

            let result = caching.fetch_screen(&card, 0).await.unwrap();
            
            assert_eq!(result.width, test_data.width);
            assert_eq!(result.height, test_data.height);
            assert_eq!(mock_provider.call_count(), 1);
            
            let cache_path = PathBuf::from("cache/12345/screen_1.png");
            assert!(mock_fs.get_file(&cache_path).is_some());
        }

        #[tokio::test]
        async fn corrupted_cache_invalid_format_fetches_from_inner() {
            let card = test_card(12345);
            let test_data = test_image_data(100, 100);
            
            let cache_path = PathBuf::from("cache/12345/cover.png");
            let mock_fs = MockFileSystem::with_file(&cache_path, b"not a valid image");
            
            let mut codec = MockImageCodec::new();
            codec.should_fail_decode = true;
            
            let mock_provider = MockCardImageProvider::with_cover(12345, test_data.clone());
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec, MockMetadataCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs,
                    codec,
                    MockMetadataCodec::new(),
                );

            let result = caching.fetch_cover(&card).await.unwrap();
            
            assert_eq!(result.width, test_data.width);
            assert_eq!(mock_provider.call_count(), 1);
        }

        #[tokio::test]
        async fn corrupted_cache_truncated_file() {
            let card = test_card(12345);
            let test_data = test_image_data(100, 100);
            
            let cache_path = PathBuf::from("cache/12345/cover.png");
            let mock_fs = MockFileSystem::with_file(&cache_path, &[1, 2, 3]);
            
            let codec = MockImageCodec::new();
            let mock_provider = MockCardImageProvider::with_cover(12345, test_data.clone());
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec, MockMetadataCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs,
                    codec,
                    MockMetadataCodec::new(),
                );

            let result = caching.fetch_cover(&card).await.unwrap();
            
            assert_eq!(result.width, test_data.width);
            assert_eq!(mock_provider.call_count(), 1);
        }

        #[tokio::test]
        async fn corrupted_cache_wrong_dimensions() {
            let card = test_card(12345);
            let expected_data = test_image_data(100, 100);
            let wrong_data = test_image_data(50, 50);
            
            let codec = MockImageCodec::new();
            let encoded_wrong = codec.encode(&wrong_data).unwrap();
            
            let cache_path = PathBuf::from("cache/12345/cover.png");
            let mock_fs = MockFileSystem::with_file(&cache_path, &encoded_wrong);
            
            let mock_provider = MockCardImageProvider::with_cover(12345, expected_data.clone());
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec, MockMetadataCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs,
                    codec,
                    MockMetadataCodec::new(),
                );

            let result = caching.fetch_cover(&card).await.unwrap();
            
            assert_eq!(result.width, 50);
            assert_eq!(mock_provider.call_count(), 0);
        }

        #[tokio::test]
        async fn fs_read_error_fetches_from_inner() {
            let card = test_card(12345);
            let test_data = test_image_data(100, 100);
            
            let cache_path = PathBuf::from("cache/12345/cover.png");
            let mock_fs = MockFileSystem::new();
            mock_fs.set_error(&cache_path, std::io::ErrorKind::PermissionDenied);
            
            let codec = MockImageCodec::new();
            let mock_provider = MockCardImageProvider::with_cover(12345, test_data.clone());
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec, MockMetadataCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs,
                    codec,
                    MockMetadataCodec::new(),
                );

            let result = caching.fetch_cover(&card).await.unwrap();
            
            assert_eq!(result.width, test_data.width);
            assert_eq!(mock_provider.call_count(), 1);
        }

        #[tokio::test]
        async fn fs_write_error_still_returns_data() {
            let card = test_card(12345);
            let test_data = test_image_data(100, 100);
            
            let cache_path = PathBuf::from("cache/12345/cover.png");
            let mock_fs = MockFileSystem::new();
            mock_fs.set_error(&cache_path, std::io::ErrorKind::Other);
            
            let codec = MockImageCodec::new();
            let mock_provider = MockCardImageProvider::with_cover(12345, test_data.clone());
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec, MockMetadataCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    codec,
                    MockMetadataCodec::new(),
                );

            let result = caching.fetch_cover(&card).await.unwrap();
            
            assert_eq!(result.width, test_data.width);
            assert!(mock_fs.get_file(&cache_path).is_none());
        }

        #[tokio::test]
        async fn fs_create_dir_error() {
            let card = test_card(12345);
            let test_data = test_image_data(100, 100);
            
            let dir_path = PathBuf::from("cache/12345");
            let mock_fs = MockFileSystem::new();
            mock_fs.set_error(&dir_path, std::io::ErrorKind::PermissionDenied);
            
            let codec = MockImageCodec::new();
            let mock_provider = MockCardImageProvider::with_cover(12345, test_data.clone());
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec, MockMetadataCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs,
                    codec,
                    MockMetadataCodec::new(),
                );

            let result = caching.fetch_cover(&card).await;
            
            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn concurrent_reads_same_file() {
            let card = test_card(12345);
            let test_data = test_image_data(100, 100);
            let codec = MockImageCodec::new();
            
            let encoded = codec.encode(&test_data).unwrap();
            let cache_path = PathBuf::from("cache/12345/cover.png");
            let mock_fs = MockFileSystem::with_file(&cache_path, &encoded);
            
            let mock_provider = MockCardImageProvider::new();
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec, MockMetadataCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    MockImageCodec::new(),
                    MockMetadataCodec::new(),
                );

            let card_clone = card.clone();
            let caching_clone = caching.clone();
            
            let (result1, result2) = tokio::join!(
                caching.fetch_cover(&card),
                caching_clone.fetch_cover(&card_clone)
            );
            
            assert!(result1.is_ok());
            assert!(result2.is_ok());
            assert_eq!(result1.unwrap().width, test_data.width);
            assert_eq!(result2.unwrap().width, test_data.width);
        }

        #[tokio::test]
        async fn concurrent_read_and_write() {
            let card = test_card(12345);
            let test_data = test_image_data(100, 100);
            
            let mock_fs = MockFileSystem::new();
            let codec = MockImageCodec::new();
            let mock_provider = MockCardImageProvider::with_cover(12345, test_data.clone());
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec, MockMetadataCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    codec,
                    MockMetadataCodec::new(),
                );

            let card_clone = card.clone();
            let caching_clone = caching.clone();
            
            let (result1, result2) = tokio::join!(
                caching.fetch_cover(&card),
                caching_clone.fetch_cover(&card_clone)
            );
            
            assert!(result1.is_ok());
            assert!(result2.is_ok());
        }

        #[tokio::test]
        async fn concurrent_writes() {
            let card1 = test_card(111);
            let card2 = test_card(222);
            let test_data1 = test_image_data(100, 100);
            let test_data2 = test_image_data(200, 200);
            
            let mock_fs = MockFileSystem::new();
            let codec = MockImageCodec::new();
            
            let mut covers = HashMap::new();
            covers.insert(111, test_data1.clone());
            covers.insert(222, test_data2.clone());
            let mock_provider = MockCardImageProvider {
                covers: Arc::new(covers),
                screens: Arc::new(HashMap::new()),
                call_count: Arc::new(AtomicUsize::new(0)),
                should_fail: false,
            };
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec, MockMetadataCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    codec,
                    MockMetadataCodec::new(),
                );

            let caching_clone = caching.clone();
            
            let (result1, result2) = tokio::join!(
                caching.fetch_cover(&card1),
                caching_clone.fetch_cover(&card2)
            );
            
            assert!(result1.is_ok());
            assert!(result2.is_ok());
            
            let path1 = PathBuf::from("cache/111/cover.png");
            let path2 = PathBuf::from("cache/222/cover.png");
            assert!(mock_fs.get_file(&path1).is_some());
            assert!(mock_fs.get_file(&path2).is_some());
        }

        /// Integration test: Verify RealFileSystem and RealImageCodec work with actual cache files
        #[tokio::test]
        #[ignore] // Run manually with: cargo test real_cache_verification --ignored -- --nocapture
        async fn real_cache_verification() {
            use crate::app::library::fs::RealFileSystem;
            use crate::app::library::image_codec::RealImageCodec;
            
            // This test verifies the entire cache loading pipeline works with real files
            let cache_dir = PathBuf::from("cache");
            let test_thread_id = 100153; // Known cache directory
            
            let fs = RealFileSystem;
            let codec = RealImageCodec;
            
            // Test 1: Verify cache directory exists
            let thread_cache_dir = cache_dir.join(test_thread_id.to_string());
            println!("Checking cache directory: {:?}", thread_cache_dir);
            assert!(
                fs.exists(&thread_cache_dir).await,
                "Cache directory should exist: {:?}",
                thread_cache_dir
            );
            
            // Test 2: Verify cover.png exists
            let cover_path = thread_cache_dir.join("cover.png");
            println!("Checking cover path: {:?}", cover_path);
            let exists = fs.exists(&cover_path).await;
            println!("Cover exists: {}", exists);
            assert!(exists, "Cover should exist: {:?}", cover_path);
            
            // Test 3: Try to read the file
            println!("Reading cover file...");
            let bytes = fs.read(&cover_path).await.expect("Should read cover file");
            println!("Read {} bytes", bytes.len());
            assert!(bytes.len() > 0, "Cover file should not be empty");
            
            // Test 4: Try to decode the image
            println!("Decoding image...");
            let image_data = codec.decode(&bytes).expect("Should decode PNG");
            println!("Decoded image: {}x{}", image_data.width, image_data.height);
            assert!(image_data.width > 0 && image_data.height > 0, "Image should have valid dimensions");
            
            println!("✓ All checks passed! Cache loading works correctly.");
        }
    }
}
