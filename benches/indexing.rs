//! Benchmarks for SemanticFS indexing pipeline
//!
//! Run with: cargo bench --bench indexing
//!
//! Performance targets from PRD:
//! - Initial indexing: 1,000 files/min
//! - Incremental indexing: < 1s per file

// TODO: Implement when criterion is added as dev-dependency
// use criterion::{criterion_group, criterion_main, Criterion};

fn main() {
    println!("Benchmarks not yet configured.");
    println!("Add criterion to workspace dev-dependencies and implement:");
    println!("  - bench_crawl_directory");
    println!("  - bench_text_chunking");
    println!("  - bench_code_chunking");
    println!("  - bench_fts_search");
    println!("  - bench_full_pipeline");
}
