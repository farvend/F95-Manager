use async_trait::async_trait;
use std::io;
use std::path::Path;

#[async_trait]
pub trait FileSystem: Send + Sync {
    async fn read(&self, path: &Path) -> Result<Vec<u8>, io::Error>;
    async fn write(&self, path: &Path, data: &[u8]) -> Result<(), io::Error>;
    async fn exists(&self, path: &Path) -> bool;
    async fn create_dir_all(&self, path: &Path) -> Result<(), io::Error>;
}
