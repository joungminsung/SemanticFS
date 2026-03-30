use crate::error::{FuseError, Result};
use crate::filesystem::SemanticFilesystem;
use crate::provider::{FuseProvider, MountOptions};
use fuser::MountOption;
use semfs_core::vfs::{VfsMapper, WriteHandler};
use std::path::Path;
use std::sync::Arc;
use tracing::info;

#[derive(Default)]
pub struct MacFuseProvider;

impl MacFuseProvider {
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
            "Mounting SemanticFS (macFUSE) with filesystem"
        );

        if !Path::new("/Library/Filesystems/macfuse.fs").exists()
            && !Path::new("/usr/local/lib/libfuse.dylib").exists()
        {
            return Err(FuseError::Mount(
                "macFUSE is not installed. Install it from https://osxfuse.github.io/".to_string(),
            ));
        }

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

impl FuseProvider for MacFuseProvider {
    fn mount(&self, source: &Path, mountpoint: &Path, _options: &MountOptions) -> Result<()> {
        info!(
            source = %source.display(),
            mountpoint = %mountpoint.display(),
            "Mounting SemanticFS (macFUSE)"
        );

        // Check if macFUSE is installed
        if !Path::new("/Library/Filesystems/macfuse.fs").exists()
            && !Path::new("/usr/local/lib/libfuse.dylib").exists()
        {
            return Err(FuseError::Mount(
                "macFUSE is not installed. Install it from https://osxfuse.github.io/".to_string(),
            ));
        }

        std::fs::create_dir_all(mountpoint)?;

        info!(
            source = %source.display(),
            mountpoint = %mountpoint.display(),
            "FUSE mount configured. Use MacFuseProvider::mount_filesystem() with a VfsMapper to start the filesystem."
        );

        Ok(())
    }

    fn unmount(&self, mountpoint: &Path) -> Result<()> {
        info!(mountpoint = %mountpoint.display(), "Unmounting (macOS)");

        let output = std::process::Command::new("umount")
            .arg(mountpoint)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FuseError::Unmount(stderr.to_string()));
        }

        Ok(())
    }

    fn is_mounted(&self, mountpoint: &Path) -> bool {
        if let Ok(output) = std::process::Command::new("mount").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mp_str = mountpoint.to_string_lossy();
            stdout.lines().any(|line| line.contains(&*mp_str))
        } else {
            false
        }
    }
}
