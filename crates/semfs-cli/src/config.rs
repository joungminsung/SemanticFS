use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub source: SourceConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub index: IndexConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub paths: Vec<PathBuf>,
    pub ignore: Vec<String>,
    pub max_file_size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub model: String,
    pub batch_size: usize,
    pub dimensions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    pub alpha: f32,
    pub max_results: usize,
    pub cache_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    pub watch: bool,
    pub interval: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            source: SourceConfig::default(),
            embedding: EmbeddingConfig::default(),
            search: SearchConfig::default(),
            index: IndexConfig::default(),
        }
    }
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            ignore: vec![
                "node_modules".to_string(),
                ".git".to_string(),
                "dist".to_string(),
                "__pycache__".to_string(),
                "*.lock".to_string(),
                "target".to_string(),
            ],
            max_file_size: "50MB".to_string(),
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "auto".to_string(),
            model: "multilingual-e5-base".to_string(),
            batch_size: 100,
            dimensions: 768,
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            alpha: 0.7,
            max_results: 100,
            cache_size: 1000,
        }
    }
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            watch: true,
            interval: "5s".to_string(),
        }
    }
}

impl AppConfig {
    pub fn config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".semanticfs")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(config) => return config,
                    Err(e) => {
                        eprintln!("Warning: Failed to parse config: {}", e);
                    }
                },
                Err(e) => {
                    eprintln!("Warning: Failed to read config: {}", e);
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(Self::config_path(), content)?;
        Ok(())
    }

    pub fn max_file_size_bytes(&self) -> u64 {
        parse_size(&self.source.max_file_size).unwrap_or(50 * 1024 * 1024)
    }
}

fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_uppercase();
    if let Some(num) = s.strip_suffix("GB") {
        num.trim()
            .parse::<u64>()
            .ok()
            .map(|n| n * 1024 * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix("MB") {
        num.trim().parse::<u64>().ok().map(|n| n * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix("KB") {
        num.trim().parse::<u64>().ok().map(|n| n * 1024)
    } else {
        s.parse().ok()
    }
}
