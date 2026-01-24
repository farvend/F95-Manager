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

pub struct RealFileSystem;

#[async_trait]
impl FileSystem for RealFileSystem {
    async fn read(&self, path: &Path) -> Result<Vec<u8>, io::Error> {
        tokio::fs::read(path).await
    }

    async fn write(&self, path: &Path, data: &[u8]) -> Result<(), io::Error> {
        tokio::fs::write(path, data).await
    }

    async fn exists(&self, path: &Path) -> bool {
        tokio::fs::metadata(path).await.is_ok()
    }

    async fn create_dir_all(&self, path: &Path) -> Result<(), io::Error> {
        tokio::fs::create_dir_all(path).await
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    pub struct MockFileSystem {
        files: Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,
        errors: Arc<Mutex<HashMap<PathBuf, io::ErrorKind>>>,
    }

    impl MockFileSystem {
        pub fn new() -> Self {
            Self {
                files: Arc::new(Mutex::new(HashMap::new())),
                errors: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        pub fn with_file(path: &Path, content: &[u8]) -> Self {
            let mock = Self::new();
            mock.files
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), content.to_vec());
            mock
        }

        pub fn set_error(&self, path: &Path, error: io::ErrorKind) {
            self.errors
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), error);
        }

        pub fn get_file(&self, path: &Path) -> Option<Vec<u8>> {
            self.files.lock().unwrap().get(path).cloned()
        }
    }

    #[async_trait]
    impl FileSystem for MockFileSystem {
        async fn read(&self, path: &Path) -> Result<Vec<u8>, io::Error> {
            if let Some(error_kind) = self.errors.lock().unwrap().get(path) {
                return Err(io::Error::new(*error_kind, "Mock error"));
            }
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "File not found"))
        }

        async fn write(&self, path: &Path, data: &[u8]) -> Result<(), io::Error> {
            if let Some(error_kind) = self.errors.lock().unwrap().get(path) {
                return Err(io::Error::new(*error_kind, "Mock error"));
            }
            self.files
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), data.to_vec());
            Ok(())
        }

        async fn exists(&self, path: &Path) -> bool {
            self.files.lock().unwrap().contains_key(path)
        }

        async fn create_dir_all(&self, path: &Path) -> Result<(), io::Error> {
            if let Some(error_kind) = self.errors.lock().unwrap().get(path) {
                return Err(io::Error::new(*error_kind, "Mock error"));
            }
            // Mock doesn't need to create actual directories
            Ok(())
        }
    }
}
