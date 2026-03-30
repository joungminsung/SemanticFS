use crate::config::AppConfig;
use anyhow::Result;

pub fn execute_set(key: String, value: String) -> Result<()> {
    let mut config = AppConfig::load();

    match key.as_str() {
        "model" | "embedding.model" => config.embedding.model = value.clone(),
        "provider" | "embedding.provider" => config.embedding.provider = value.clone(),
        "alpha" | "search.alpha" => {
            config.search.alpha = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid alpha value: {}", value))?;
        }
        "max_results" | "search.max_results" => {
            config.search.max_results = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid max_results: {}", value))?;
        }
        "cache_size" | "search.cache_size" => {
            config.search.cache_size = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid cache_size: {}", value))?;
        }
        "ignore" | "source.ignore" => {
            config.source.ignore = value.split(',').map(|s| s.trim().to_string()).collect();
        }
        "max_file_size" | "source.max_file_size" => {
            config.source.max_file_size = value.clone();
        }
        "watch" | "index.watch" => {
            config.index.watch = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid bool: {}", value))?;
        }
        _ => {
            anyhow::bail!("Unknown config key: {}", key);
        }
    }

    config.save()?;
    println!("Set {} = {}", key, value);
    Ok(())
}

pub fn execute_get(key: String) -> Result<()> {
    let config = AppConfig::load();

    let value = match key.as_str() {
        "model" | "embedding.model" => config.embedding.model,
        "provider" | "embedding.provider" => config.embedding.provider,
        "alpha" | "search.alpha" => config.search.alpha.to_string(),
        "max_results" | "search.max_results" => config.search.max_results.to_string(),
        "cache_size" | "search.cache_size" => config.search.cache_size.to_string(),
        "ignore" | "source.ignore" => config.source.ignore.join(", "),
        "max_file_size" | "source.max_file_size" => config.source.max_file_size,
        "watch" | "index.watch" => config.index.watch.to_string(),
        _ => {
            anyhow::bail!("Unknown config key: {}", key);
        }
    };

    println!("{}", value);
    Ok(())
}
