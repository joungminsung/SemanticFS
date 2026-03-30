use crate::config::AppConfig;
use anyhow::Result;
use serde_json::json;

pub fn execute(subsystem: Option<String>, as_json: bool) -> Result<()> {
    let config = AppConfig::load();
    let data_dir = AppConfig::config_dir();

    match subsystem.as_deref() {
        Some("query") => diagnose_query(&config, &data_dir, as_json)?,
        Some("index") => diagnose_index(&config, &data_dir, as_json)?,
        Some("cache") => diagnose_cache(as_json)?,
        None => {
            diagnose_index(&config, &data_dir, as_json)?;
            println!();
            diagnose_cache(as_json)?;
        }
        Some(other) => {
            anyhow::bail!("Unknown subsystem: {}. Use: query, index, cache", other);
        }
    }

    Ok(())
}

fn diagnose_index(config: &AppConfig, data_dir: &std::path::Path, as_json: bool) -> Result<()> {
    let db_path = data_dir.join("index.db");

    if as_json {
        let info = if db_path.exists() {
            let sqlite = semfs_storage::SqliteStore::new(&db_path)?;
            json!({
                "status": "ok",
                "file_count": sqlite.file_count()?,
                "db_path": db_path.display().to_string(),
                "db_size_bytes": std::fs::metadata(&db_path)?.len(),
                "embedding_model": config.embedding.model,
                "embedding_provider": config.embedding.provider,
            })
        } else {
            json!({
                "status": "not_initialized",
                "db_path": db_path.display().to_string(),
            })
        };
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("Index Diagnostics");
        println!("-----------------");
        if db_path.exists() {
            let sqlite = semfs_storage::SqliteStore::new(&db_path)?;
            println!("  Status:     OK");
            println!("  Files:      {}", sqlite.file_count()?);
            println!("  DB path:    {}", db_path.display());
            println!("  DB size:    {} bytes", std::fs::metadata(&db_path)?.len());
            println!("  Model:      {}", config.embedding.model);
            println!("  Provider:   {}", config.embedding.provider);
        } else {
            println!("  Status: NOT INITIALIZED");
            println!("  Run 'semfs index <dir>' to create the index");
        }
    }

    Ok(())
}

fn diagnose_query(config: &AppConfig, _data_dir: &std::path::Path, as_json: bool) -> Result<()> {
    let (embedder_name, embedder_dims, embedder_error) = match semfs_embed::auto_detect_embedder() {
        Ok(embedder) => (
            Some(embedder.model_name().to_string()),
            Some(embedder.dimensions()),
            None,
        ),
        Err(e) => (None, None, Some(e.to_string())),
    };

    if as_json {
        let mut info = serde_json::json!({
            "alpha": config.search.alpha,
            "max_results": config.search.max_results,
            "cache_size": config.search.cache_size,
        });
        if let Some(name) = &embedder_name {
            info["embedder_model"] = serde_json::json!(name);
            info["embedder_dimensions"] = serde_json::json!(embedder_dims.unwrap_or(0));
            if embedder_dims == Some(0) {
                info["warning"] =
                    serde_json::json!("No embedding model available. Using keyword-only search.");
            }
        }
        if let Some(err) = &embedder_error {
            info["embedder_error"] = serde_json::json!(err);
        }
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("Query Diagnostics");
        println!("-----------------");
        println!("  Alpha (semantic weight): {}", config.search.alpha);
        println!("  Max results:             {}", config.search.max_results);
        println!("  Cache size:              {}", config.search.cache_size);

        match (&embedder_name, &embedder_error) {
            (Some(name), _) => {
                let dims = embedder_dims.unwrap_or(0);
                println!("  Embedder:                {} (dims: {})", name, dims);
                if dims == 0 {
                    println!("  WARNING: No embedding model available. Using keyword-only search.");
                }
            }
            (_, Some(e)) => {
                println!("  Embedder:                ERROR - {}", e);
            }
            _ => {}
        }
    }

    Ok(())
}

fn diagnose_cache(as_json: bool) -> Result<()> {
    if as_json {
        let info = json!({
            "query_cache": "in-memory (resets on restart)",
            "embedding_cache": "in-memory",
            "parsed_query_cache": "in-memory, TTL: 300s",
        });
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("Cache Diagnostics");
        println!("-----------------");
        println!("  Query cache:        in-memory LRU (resets on restart)");
        println!("  Embedding cache:    in-memory hash-map");
        println!("  Parsed query cache: in-memory, TTL: 300s");
        println!("  Note: Live cache stats only available when semfs is running");
    }

    Ok(())
}
