use async_trait::async_trait;
use std::path::PathBuf;
use url::Url;

use super::{ImageData, LibraryCard, ProviderError};

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

pub struct CachingProvider<P: CardImageProvider> {
    inner: P,
    cache_dir: PathBuf,
}

impl<P: CardImageProvider> CachingProvider<P> {
    pub fn new(inner: P, cache_dir: PathBuf) -> Self {
        Self { inner, cache_dir }
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

    async fn load_from_cache(&self, path: &PathBuf) -> Option<ImageData> {
        if tokio::fs::metadata(path).await.is_err() {
            return None;
        }

        let path_clone = path.clone();
        let result = tokio::task::spawn_blocking(move || {
            let bytes = std::fs::read(&path_clone).ok()?;
            let img = image::load_from_memory(&bytes).ok()?;
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            Some(ImageData::new(w, h, rgba.into_vec()))
        })
        .await
        .ok()
        .flatten();

        result
    }

    async fn save_to_cache(&self, path: &PathBuf, data: &ImageData) {
        if let Some(parent) = path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        let path_clone = path.clone();
        let width = data.width;
        let height = data.height;
        let rgba = data.rgba.clone();

        let _ = tokio::task::spawn_blocking(move || {
            image::save_buffer(&path_clone, &rgba, width, height, image::ColorType::Rgba8)
        })
        .await;
    }
}

#[async_trait]
impl<P: CardImageProvider> CardImageProvider for CachingProvider<P> {
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

impl<P: CardImageProvider> std::fmt::Debug for CachingProvider<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachingProvider")
            .field("cache_dir", &self.cache_dir)
            .finish()
    }
}
