use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

pub trait Persistable: Serialize + for<'de> Deserialize<'de> {
    fn load_from_file(path: &Path) -> io::Result<Self>
    where
        Self: Sized,
    {
        let data = std::fs::read_to_string(path)?;
        let s: Self = serde_json::from_str(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(s)
    }

    fn save_to_file(&self, path: &Path) -> io::Result<()> {
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        std::fs::write(path, data)
    }
}
