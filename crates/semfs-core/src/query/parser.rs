use super::filter::{extract_date_filter, extract_extension_filter};
use super::types::{ParsedQuery, SortOrder};
use tracing::debug;

/// Parse natural language path into structured query
pub fn parse_query(input: &str) -> ParsedQuery {
    let mut text = input.trim().to_string();
    let mut filters = Vec::new();

    // Remove surrounding quotes if present
    if (text.starts_with('"') && text.ends_with('"'))
        || (text.starts_with('\'') && text.ends_with('\''))
    {
        text = text[1..text.len() - 1].to_string();
    }

    // Extract date filter
    if let Some((filter, remaining)) = extract_date_filter(&text) {
        filters.push(filter);
        text = remaining;
    }

    // Extract extension filter
    if let Some((filter, remaining)) = extract_extension_filter(&text) {
        filters.push(filter);
        text = remaining;
    }

    // Detect sort order from keywords
    let sort = if text.contains("최신") || text.contains("최근") {
        SortOrder::DateDesc
    } else if text.contains("오래된") {
        SortOrder::DateAsc
    } else if text.contains("이름순") {
        SortOrder::NameAsc
    } else {
        SortOrder::Relevance
    };

    // Clean up remaining text as semantic query
    let semantic_query = text
        .split_whitespace()
        .filter(|w| {
            ![
                "중",
                "에서",
                "관련",
                "파일",
                "있는",
                "포함된",
                "작성한",
                "만든",
                "에",
                "의",
                "을",
                "를",
                "이",
                "가",
                "은",
                "는",
                "으로",
                "동안",
                "수정한",
            ]
            .contains(w)
        })
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();

    debug!(
        raw = input,
        semantic = %semantic_query,
        filter_count = filters.len(),
        "Parsed query"
    );

    ParsedQuery {
        semantic_query,
        filters,
        sort,
        raw_input: input.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::types::QueryFilter;

    #[test]
    fn test_parse_simple_query() {
        let q = parse_query("React 프로젝트");
        assert!(q.semantic_query.contains("React"));
        assert!(q.semantic_query.contains("프로젝트"));
        assert!(q.filters.is_empty());
    }

    #[test]
    fn test_parse_with_year() {
        let q = parse_query("2024년에 작성한 React 프로젝트");
        assert!(q.semantic_query.contains("React"));
        assert_eq!(q.filters.len(), 1);
        assert!(matches!(&q.filters[0], QueryFilter::DateRange { .. }));
    }

    #[test]
    fn test_parse_with_extension() {
        let q = parse_query("2024년에 작성한 React 프로젝트 중 TypeScript 파일");
        assert_eq!(q.filters.len(), 2);
    }

    #[test]
    fn test_parse_quoted() {
        let q = parse_query("\"에러 로그가 포함된 파일\"");
        assert!(q.semantic_query.contains("에러"));
        assert!(q.semantic_query.contains("로그가"));
    }

    #[test]
    fn test_parse_sort_order() {
        let q = parse_query("최신 업데이트된 파일");
        assert!(matches!(q.sort, SortOrder::DateDesc));
    }
}
