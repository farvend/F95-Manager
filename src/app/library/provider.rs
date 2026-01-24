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
