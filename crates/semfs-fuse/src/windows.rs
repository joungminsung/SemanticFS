use crate::error::{FuseError, Result};
use crate::provider::{FuseProvider, MountOptions};
use std::path::Path;
use tracing::info;

pub struct WindowsFuseProvider;

impl WindowsFuseProvider {
    pub fn new() -> Self {
        Self
    }
}

impl FuseProvider for WindowsFuseProvider {
    fn mount(&self, source: &Path, mountpoint: &Path, _options: &MountOptions) -> Result<()> {
        info!(
            source = %source.display(),
            mountpoint = %mountpoint.display(),
            "Mounting SemanticFS (Windows WinFSP)"
        );

        // TODO: WinFSP/Dokan integration
        Err(FuseError::Mount(
            "Windows FUSE support is not yet implemented. Use 'semfs search' for CLI-only mode."
                .to_string(),
        ))
    }

    fn unmount(&self, _mountpoint: &Path) -> Result<()> {
        Err(FuseError::UnsupportedPlatform)
    }

    fn is_mounted(&self, _mountpoint: &Path) -> bool {
        false
    }
}
