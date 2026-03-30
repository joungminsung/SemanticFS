use crate::config::AppConfig;
use anyhow::Result;

pub fn execute(query: String, limit: usize) -> Result<()> {
    let config = AppConfig::load();
    let data_dir = AppConfig::config_dir();

    let sqlite = std::sync::Arc::new(semfs_storage::SqliteStore::new(&data_dir.join("index.db"))?);
    let lance = std::sync::Arc::new(semfs_storage::LanceStore::new(
        &data_dir.join("vectors.lance"),
        config.embedding.dimensions,
    )?);

    let embedder: std::sync::Arc<dyn semfs_embed::Embedder> =
        std::sync::Arc::from(semfs_embed::auto_detect_embedder()?);

    let keyword_retriever = semfs_core::retriever::KeywordRetriever::new(sqlite.clone());
    let semantic_retriever = semfs_core::retriever::SemanticRetriever::new(embedder, lance);
    let hybrid = semfs_core::HybridRetriever::new(
        keyword_retriever,
        semantic_retriever,
        sqlite,
        config.search.alpha,
    );

    let parsed = semfs_core::parse_query(&query);
    let results = hybrid.search(&parsed, limit)?;

    if results.is_empty() {
        println!("No results found for: {}", query);
        return Ok(());
    }

    println!("Results for: {}\n", query);
    for (i, result) in results.iter().enumerate() {
        println!(
            "  {}. {} (score: {:.3})",
            i + 1,
            result.path.display(),
            result.score
        );
    }
    println!("\n{} files found", results.len());

    Ok(())
}
