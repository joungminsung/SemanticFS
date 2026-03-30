use semfs_storage::MetadataFilter;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedQuery {
    pub semantic_query: String,
    pub filters: Vec<QueryFilter>,
    pub sort: SortOrder,
    pub raw_input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryFilter {
    DateRange { start: i64, end: i64 },
    Extension(Vec<String>),
    Size { min: Option<u64>, max: Option<u64> },
    MimeType(Vec<String>),
}

impl QueryFilter {
    pub fn to_metadata_filter(&self) -> MetadataFilter {
        match self {
            QueryFilter::DateRange { start, end } => MetadataFilter::DateRange {
                start: *start,
                end: *end,
            },
            QueryFilter::Extension(exts) => MetadataFilter::Extension(exts.clone()),
            QueryFilter::Size { min, max } => MetadataFilter::Size {
                min: *min,
                max: *max,
            },
            QueryFilter::MimeType(types) => MetadataFilter::MimeType(types.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum SortOrder {
    #[default]
    Relevance,
    DateDesc,
    DateAsc,
    NameAsc,
    NameDesc,
}
