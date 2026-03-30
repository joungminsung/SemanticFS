use crate::error::Result;
use std::path::Path;

/// Mount options
#[derive(Debug, Clone)]
pub struct MountOptions {
    pub read_only: bool,
    pub allow_other: bool,
    pub auto_unmount: bool,
    pub max_results: usize,
}

impl Default for MountOptions {
    fn default() -> Self {
        Self {
            read_only: false,
            allow_other: false,
            auto_unmount: true,
            max_results: 100,
        }
    }
}

/// Trait for platform-specific FUSE implementations
pub trait FuseProvider: Send + Sync {
    fn mount(&self, source: &Path, mountpoint: &Path, options: &MountOptions) -> Result<()>;
    fn unmount(&self, mountpoint: &Path) -> Result<()>;
    fn is_mounted(&self, mountpoint: &Path) -> bool;
}

/// Create the appropriate FUSE provider for the current platform.
/// Requires the `fuse-mount` feature (and macFUSE/libfuse3 installed).
pub fn create_provider() -> Result<Box<dyn FuseProvider>> {
    #[cfg(all(feature = "fuse-mount", target_os = "linux"))]
    {
        Ok(Box::new(crate::linux::LinuxFuseProvider::new()))
    }

    #[cfg(all(feature = "fuse-mount", target_os = "macos"))]
    {
        Ok(Box::new(crate::macos::MacFuseProvider::new()))
    }

    #[cfg(all(feature = "fuse-mount", target_os = "windows"))]
    {
        Ok(Box::new(crate::windows::WindowsFuseProvider::new()))
    }

    #[cfg(not(any(
        all(feature = "fuse-mount", target_os = "linux"),
        all(feature = "fuse-mount", target_os = "macos"),
        all(feature = "fuse-mount", target_os = "windows"),
    )))]
    {
        Err(crate::error::FuseError::UnsupportedPlatform)
    }
}
