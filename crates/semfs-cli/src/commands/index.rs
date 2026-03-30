use crate::config::AppConfig;
use anyhow::Result;
use std::path::PathBuf;

pub fn execute(source: PathBuf, full: bool) -> Result<()> {
    let config = AppConfig::load();

    println!("Indexing: {}", source.display());
    if full {
        println!("  Mode: full reindex");
    } else {
        println!("  Mode: incremental");
    }

    let data_dir = AppConfig::config_dir();
    std::fs::create_dir_all(&data_dir)?;

    let sqlite = std::sync::Arc::new(semfs_storage::SqliteStore::new(&data_dir.join("index.db"))?);
    let lance = std::sync::Arc::new(semfs_storage::LanceStore::new(
        &data_dir.join("vectors.lance"),
        config.embedding.dimensions,
    )?);
    let cache = std::sync::Arc::new(semfs_storage::CacheManager::default());

    let embedder: std::sync::Arc<dyn semfs_embed::Embedder> =
        std::sync::Arc::from(semfs_embed::auto_detect_embedder()?);

    println!("  Embedder: {}", embedder.model_name());

    let pipeline = semfs_core::IndexingPipeline::new(
        sqlite.clone(),
        lance.clone(),
        embedder,
        cache,
        config.source.ignore.clone(),
        config.max_file_size_bytes(),
        config.embedding.batch_size,
    );

    let start = std::time::Instant::now();
    let stats = pipeline.index_directory(&source)?;
    let elapsed = start.elapsed();

    println!("\nIndexing complete in {:.1}s", elapsed.as_secs_f64());
    println!("  Total files:  {}", stats.total_files);
    println!("  Indexed:      {}", stats.indexed);
    println!("  Skipped:      {}", stats.skipped);
    println!("  Errors:       {}", stats.errors);

    if stats.total_files > 0 {
        let rate = stats.total_files as f64 / elapsed.as_secs_f64() * 60.0;
        println!("  Speed:        {:.0} files/min", rate);
    }

    Ok(())
}
