use async_trait::async_trait;
use std::path::{Path, PathBuf};
use url::Url;

use super::{FileSystem, ImageCodec, ImageData, LibraryCard, ProviderError};

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

pub struct CachingProvider<P, FS, IC> {
    inner: P,
    cache_dir: PathBuf,
    fs: FS,
    codec: IC,
}

impl<P: Clone, FS: Clone, IC: Clone> Clone for CachingProvider<P, FS, IC> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            cache_dir: self.cache_dir.clone(),
            fs: self.fs.clone(),
            codec: self.codec.clone(),
        }
    }
}

impl<P: CardImageProvider, FS: FileSystem, IC: ImageCodec> CachingProvider<P, FS, IC> {
    pub fn new(inner: P, cache_dir: PathBuf, fs: FS, codec: IC) -> Self {
        Self {
            inner,
            cache_dir,
            fs,
            codec,
        }
    }

    fn cover_path(&self, card: &LibraryCard) -> PathBuf {
        self.cache_dir
            .join(card.thread_id.to_string())
            .join("cover.png")
    }

    fn screen_path(&self, card: &LibraryCard, idx: usize) -> PathBuf {
        self.cache_dir
            .join(card.thread_id.to_string())
            .join(format!("screen_{}.png", idx + 1))
    }

    async fn load_from_cache(&self, path: &Path) -> Option<ImageData> {
        if !self.fs.exists(path).await {
            return None;
        }

        let bytes = self.fs.read(path).await.ok()?;
        let data = self.codec.decode(&bytes).ok()?;
        Some(data)
    }

    async fn save_to_cache(&self, path: &Path, data: &ImageData) {
        if let Some(parent) = path.parent() {
            let _ = self.fs.create_dir_all(parent).await;
        }

        if let Ok(bytes) = self.codec.encode(data) {
            let _ = self.fs.write(path, &bytes).await;
        }
    }
}

#[async_trait]
impl<P: CardImageProvider, FS: FileSystem, IC: ImageCodec> CardImageProvider
    for CachingProvider<P, FS, IC>
{
    async fn fetch_cover(&self, card: &LibraryCard) -> Result<ImageData, ProviderError> {
        let path = self.cover_path(card);

        if let Some(cached) = self.load_from_cache(&path).await {
            return Ok(cached);
        }

        let data = self.inner.fetch_cover(card).await?;
        self.save_to_cache(&path, &data).await;
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

        let data = self.inner.fetch_screen(card, idx).await?;
        self.save_to_cache(&path, &data).await;
        Ok(data)
    }
}

impl<P: CardImageProvider, FS: FileSystem, IC: ImageCodec> std::fmt::Debug
    for CachingProvider<P, FS, IC>
{
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
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    codec,
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
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    codec,
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
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs,
                    codec,
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
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    codec,
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
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs,
                    codec,
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
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs,
                    codec,
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
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs,
                    codec,
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
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs,
                    codec,
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
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    codec,
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
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs,
                    codec,
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
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    MockImageCodec::new(),
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
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    codec,
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
            
            let caching: CachingProvider<MockCardImageProvider, MockFileSystem, MockImageCodec> = 
                CachingProvider::new(
                    mock_provider.clone(),
                    PathBuf::from("cache"),
                    mock_fs.clone(),
                    codec,
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
    }
}
