use crate::error::Result;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Crawl a directory and return all indexable file paths
pub fn crawl_directory(
    root: &Path,
    ignore_patterns: &[String],
    max_file_size: u64,
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    crawl_recursive(root, ignore_patterns, max_file_size, &mut files)?;
    debug!(count = files.len(), root = %root.display(), "Crawled directory");
    Ok(files)
}

fn crawl_recursive(
    dir: &Path,
    ignore_patterns: &[String],
    max_file_size: u64,
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Check ignore patterns
        if should_ignore(&name, &path, ignore_patterns) {
            continue;
        }

        if path.is_dir() {
            crawl_recursive(&path, ignore_patterns, max_file_size, files)?;
        } else if path.is_file() {
            // Check file size
            if let Ok(metadata) = entry.metadata() {
                if metadata.len() > max_file_size {
                    debug!(path = %path.display(), size = metadata.len(), "Skipping large file");
                    continue;
                }
            }
            files.push(path);
        }
    }

    Ok(())
}

fn should_ignore(name: &str, path: &Path, patterns: &[String]) -> bool {
    // Always ignore hidden files/dirs (starting with .)
    if name.starts_with('.') {
        return true;
    }

    let path_str = path.to_string_lossy();
    for pattern in patterns {
        if let Some(ext) = pattern.strip_prefix("*.") {
            // Wildcard extension match: "*.lock" matches files ending in ".lock"
            // "lock"
            if name.ends_with(&format!(".{}", ext)) {
                return true;
            }
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            if name.ends_with(suffix) {
                return true;
            }
        } else if path_str.contains(pattern) || name == *pattern {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_ignore() {
        assert!(should_ignore(".git", Path::new("/repo/.git"), &[]));
        assert!(should_ignore(
            "node_modules",
            Path::new("/repo/node_modules"),
            &["node_modules".to_string()]
        ));
        assert!(should_ignore(
            "Cargo.lock",
            Path::new("/repo/Cargo.lock"),
            &["*.lock".to_string()]
        ));
        assert!(!should_ignore(
            "package-lock.json",
            Path::new("/repo/package-lock.json"),
            &["*.lock".to_string()]
        ));
        assert!(!should_ignore(
            "clock.rs",
            Path::new("/repo/clock.rs"),
            &["*.lock".to_string()]
        ));
        assert!(!should_ignore(
            "src",
            Path::new("/repo/src"),
            &["node_modules".to_string()]
        ));
    }
}
