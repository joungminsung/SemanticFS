use anyhow::Result;
use std::path::PathBuf;

pub fn execute(mountpoint: PathBuf) -> Result<()> {
    let provider = semfs_fuse::create_provider()?;

    if !provider.is_mounted(&mountpoint) {
        println!("Not mounted: {}", mountpoint.display());
        return Ok(());
    }

    provider.unmount(&mountpoint)?;
    println!("Unmounted: {}", mountpoint.display());
    Ok(())
}
