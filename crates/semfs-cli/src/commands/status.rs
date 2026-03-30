use crate::config::AppConfig;
use anyhow::Result;

pub fn execute() -> Result<()> {
    let config = AppConfig::load();
    let data_dir = AppConfig::config_dir();

    println!("SemanticFS Status");
    println!("=================\n");

    // Config info
    println!("Config: {}", AppConfig::config_path().display());
    println!("Data:   {}\n", data_dir.display());

    // Index info
    let db_path = data_dir.join("index.db");
    if db_path.exists() {
        let sqlite = semfs_storage::SqliteStore::new(&db_path)?;
        let count = sqlite.file_count()?;
        println!("Indexed files: {}", count);

        let db_size = std::fs::metadata(&db_path)?.len();
        println!("Index size:    {}", format_size(db_size));
    } else {
        println!("Index: not created (run 'semfs index <dir>' first)");
    }

    // Vector store info
    let lance_path = data_dir.join("vectors.lance");
    if lance_path.exists() {
        let lance_size = dir_size(&lance_path).unwrap_or(0);
        println!("Vector store:  {}", format_size(lance_size));
    }

    // Embedding info
    println!("\nEmbedding:");
    println!("  Provider: {}", config.embedding.provider);
    println!("  Model:    {}", config.embedding.model);
    println!("  Dims:     {}", config.embedding.dimensions);

    // Search config
    println!("\nSearch:");
    println!("  Alpha:       {}", config.search.alpha);
    println!("  Max results: {}", config.search.max_results);
    println!("  Cache size:  {}", config.search.cache_size);

    Ok(())
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut size = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            if meta.is_dir() {
                size += dir_size(&entry.path())?;
            } else {
                size += meta.len();
            }
        }
    }
    Ok(size)
}
