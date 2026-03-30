use crate::config::AppConfig;
use anyhow::Result;
use std::path::PathBuf;

pub fn execute(
    source: PathBuf,
    mountpoint: PathBuf,
    model: Option<String>,
    read_only: bool,
) -> Result<()> {
    let config = AppConfig::load();

    println!("SemanticFS - Mounting...");
    println!("  Source:     {}", source.display());
    println!("  Mountpoint: {}", mountpoint.display());
    println!(
        "  Model:      {}",
        model.as_deref().unwrap_or(&config.embedding.model)
    );
    println!(
        "  Mode:       {}",
        if read_only { "read-only" } else { "read-write" }
    );

    // 1. Initialize storage
    let data_dir = AppConfig::config_dir();
    std::fs::create_dir_all(&data_dir)?;

    let sqlite = std::sync::Arc::new(semfs_storage::SqliteStore::new(&data_dir.join("index.db"))?);
    let lance = std::sync::Arc::new(semfs_storage::LanceStore::new(
        &data_dir.join("vectors.lance"),
        config.embedding.dimensions,
    )?);
    let _wal = std::sync::Arc::new(semfs_storage::WalStore::new(&data_dir.join("wal.db"))?);
    let cache = std::sync::Arc::new(semfs_storage::CacheManager::new(
        config.search.cache_size,
        300, // parsed query TTL
        500, // parsed query max
    ));

    // 2. Initialize embedder
    let embedder: std::sync::Arc<dyn semfs_embed::Embedder> =
        std::sync::Arc::from(semfs_embed::auto_detect_embedder()?);

    println!("  Embedder:   {}", embedder.model_name());

    // 3. Run initial indexing
    println!("\nIndexing files...");
    let pipeline = semfs_core::IndexingPipeline::new(
        sqlite.clone(),
        lance.clone(),
        embedder.clone(),
        cache.clone(),
        config.source.ignore.clone(),
        config.max_file_size_bytes(),
        config.embedding.batch_size,
    );

    let stats = pipeline.index_directory(&source)?;
    println!(
        "  Indexed: {} files ({} skipped, {} errors)",
        stats.indexed, stats.skipped, stats.errors
    );

    // 4. Mount FUSE
    println!("\nMounting FUSE filesystem...");
    let provider = semfs_fuse::create_provider()?;
    let mount_options = semfs_fuse::MountOptions {
        read_only,
        max_results: config.search.max_results,
        ..Default::default()
    };
    provider.mount(&source, &mountpoint, &mount_options)?;

    println!("\nSemanticFS mounted at {}", mountpoint.display());
    println!("Try: ls \"{}/<your query>\"", mountpoint.display());

    Ok(())
}
