pub mod error;
pub mod provider;

#[cfg(feature = "fuse-mount")]
pub mod filesystem;

#[cfg(all(feature = "fuse-mount", target_os = "linux"))]
pub mod linux;

#[cfg(all(feature = "fuse-mount", target_os = "macos"))]
pub mod macos;

#[cfg(all(feature = "fuse-mount", target_os = "windows"))]
pub mod windows;

pub use error::{FuseError, Result};
#[cfg(feature = "fuse-mount")]
pub use filesystem::SemanticFilesystem;
pub use provider::{create_provider, FuseProvider, MountOptions};
