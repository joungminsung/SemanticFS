use crate::error::{FuseError, Result};
use crate::filesystem::SemanticFilesystem;
use crate::provider::{FuseProvider, MountOptions};
use fuser::MountOption;
use semfs_core::vfs::{VfsMapper, WriteHandler};
use std::path::Path;
use std::sync::Arc;
use tracing::info;

#[derive(Default)]
pub struct LinuxFuseProvider;

impl LinuxFuseProvider {
    pub fn new() -> Self {
        Self
    }

    /// Mount with a fully initialized SemanticFilesystem.
    ///
    /// This is the primary entry point for production use. The caller is
    /// responsible for constructing the `VfsMapper` and optional `WriteHandler`.
    pub fn mount_filesystem(
        &self,
        vfs: Arc<VfsMapper>,
        write_handler: Option<Arc<WriteHandler>>,
        source: &Path,
        mountpoint: &Path,
        options: &MountOptions,
    ) -> Result<()> {
        info!(
            source = %source.display(),
            mountpoint = %mountpoint.display(),
            "Mounting SemanticFS (Linux FUSE) with filesystem"
        );

        std::fs::create_dir_all(mountpoint)?;

        let mut mount_options = vec![
            MountOption::FSName("semanticfs".to_string()),
            MountOption::DefaultPermissions,
        ];
        if options.auto_unmount {
            mount_options.push(MountOption::AutoUnmount);
        }
        if options.allow_other {
            mount_options.push(MountOption::AllowOther);
        }
        if options.read_only {
            mount_options.push(MountOption::RO);
        }

        let fs =
            SemanticFilesystem::new(vfs, write_handler, source.to_path_buf(), options.read_only);

        fuser::mount2(fs, mountpoint, &mount_options)?;

        Ok(())
    }
}

impl FuseProvider for LinuxFuseProvider {
    fn mount(&self, source: &Path, mountpoint: &Path, _options: &MountOptions) -> Result<()> {
        info!(
            source = %source.display(),
            mountpoint = %mountpoint.display(),
            "FUSE mount configured. Use LinuxFuseProvider::mount_filesystem() with a VfsMapper to start the filesystem."
        );

        std::fs::create_dir_all(mountpoint)?;
        Ok(())
    }

    fn unmount(&self, mountpoint: &Path) -> Result<()> {
        info!(mountpoint = %mountpoint.display(), "Unmounting (Linux)");

        // Use fusermount to unmount
        let output = std::process::Command::new("fusermount")
            .arg("-u")
            .arg(mountpoint)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FuseError::Unmount(stderr.to_string()));
        }

        Ok(())
    }

    fn is_mounted(&self, mountpoint: &Path) -> bool {
        // Check /proc/mounts on Linux
        if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
            let mp_str = mountpoint.to_string_lossy();
            mounts.lines().any(|line| line.contains(&*mp_str))
        } else {
            false
        }
    }
}
