// Cross-platform helpers: open URL in default browser and reveal folder in file manager.

/// Open URL in the system default browser
pub fn open_in_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        // Use explorer to open default browser without invoking a shell to avoid cmd injection
        if let Err(e) = std::process::Command::new("explorer").arg(url).spawn() {
            log::error!("Failed to open browser for {}: {}", url, e);
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = std::process::Command::new("open").arg(url).spawn() {
            log::error!("Failed to open browser for {}: {}", url, e);
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Err(e) = std::process::Command::new("xdg-open").arg(url).spawn() {
            log::error!("Failed to open browser for {}: {}", url, e);
        }
    }
}

/// Reveal a path in the system file manager (or open the folder)
pub fn reveal_in_file_manager(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = std::process::Command::new("explorer").arg(path).spawn() {
            log::error!("Failed to open folder {}: {}", path.to_string_lossy(), e);
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = std::process::Command::new("open").arg(path).spawn() {
            log::error!("Failed to open folder {}: {}", path.to_string_lossy(), e);
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Err(e) = std::process::Command::new("xdg-open").arg(path).spawn() {
            log::error!("Failed to open folder {}: {}", path.to_string_lossy(), e);
        }
    }
}
