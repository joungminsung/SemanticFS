use super::types::QueryFilter;
use chrono::{Datelike, Local, NaiveDate};
use regex::Regex;
use tracing::debug;

/// Extract date filters from natural language
pub fn extract_date_filter(text: &str) -> Option<(QueryFilter, String)> {
    // Pattern: "2024년" or "2024"
    let year_re = Regex::new(r"(\d{4})년?에?\s*(작성한|만든|생성한)?").unwrap();
    if let Some(caps) = year_re.captures(text) {
        let year: i32 = caps[1].parse().unwrap();
        let start = NaiveDate::from_ymd_opt(year, 1, 1)
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
            .unwrap_or(0);
        let end = NaiveDate::from_ymd_opt(year, 12, 31)
            .map(|d| d.and_hms_opt(23, 59, 59).unwrap().and_utc().timestamp())
            .unwrap_or(0);
        let cleaned = year_re.replace(text, "").trim().to_string();
        debug!(year, "Extracted year filter");
        return Some((QueryFilter::DateRange { start, end }, cleaned));
    }

    // Pattern: "최근 N일" / "지난 N일"
    let recent_re = Regex::new(r"(최근|지난)\s*(\d+)\s*(일|주|개월|달)").unwrap();
    if let Some(caps) = recent_re.captures(text) {
        let n: i64 = caps[2].parse().unwrap();
        let unit = &caps[3];
        let days = match unit {
            "일" => n,
            "주" => n * 7,
            "개월" | "달" => n * 30,
            _ => n,
        };
        let now = chrono::Utc::now().timestamp();
        let start = now - (days * 86400);
        let cleaned = recent_re.replace(text, "").trim().to_string();
        debug!(days, "Extracted recent date filter");
        return Some((QueryFilter::DateRange { start, end: now }, cleaned));
    }

    // Pattern: "지난달", "이번달"
    let month_re = Regex::new(r"(지난달|저번달|이번\s*달)").unwrap();
    if let Some(caps) = month_re.captures(text) {
        let now = Local::now();
        let (year, month) = match &caps[1] {
            "이번달" | "이번 달" => (now.year(), now.month()),
            _ => {
                if now.month() == 1 {
                    (now.year() - 1, 12)
                } else {
                    (now.year(), now.month() - 1)
                }
            }
        };
        let start = NaiveDate::from_ymd_opt(year, month, 1)
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
            .unwrap_or(0);
        let end_day = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)
        };
        let end = end_day
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() - 1)
            .unwrap_or(0);
        let cleaned = month_re.replace(text, "").trim().to_string();
        return Some((QueryFilter::DateRange { start, end }, cleaned));
    }

    None
}

/// Extract extension filter from natural language
pub fn extract_extension_filter(text: &str) -> Option<(QueryFilter, String)> {
    let mappings = vec![
        (r"(?i)TypeScript", vec!["ts".to_string(), "tsx".to_string()]),
        (r"(?i)JavaScript", vec!["js".to_string(), "jsx".to_string()]),
        (r"(?i)Python", vec!["py".to_string()]),
        (r"(?i)Rust", vec!["rs".to_string()]),
        (r"(?i)Go\b", vec!["go".to_string()]),
        (r"(?i)Java\b", vec!["java".to_string()]),
        (
            r"(?i)C\+\+",
            vec![
                "cpp".to_string(),
                "cc".to_string(),
                "cxx".to_string(),
                "hpp".to_string(),
            ],
        ),
        (r"(?i)C#|C\s*Sharp", vec!["cs".to_string()]),
        (r"(?i)Ruby", vec!["rb".to_string()]),
        (r"(?i)PHP", vec!["php".to_string()]),
        (r"(?i)Swift", vec!["swift".to_string()]),
        (r"(?i)Kotlin", vec!["kt".to_string()]),
        (r"(?i)마크다운|Markdown", vec!["md".to_string()]),
        (r"(?i)텍스트\s*파일|text\s*file", vec!["txt".to_string()]),
        (r"(?i)JSON", vec!["json".to_string()]),
        (r"(?i)YAML|YML", vec!["yaml".to_string(), "yml".to_string()]),
        (r"(?i)TOML", vec!["toml".to_string()]),
        (r"(?i)CSV", vec!["csv".to_string()]),
        (
            r"(?i)이미지|image|사진|photo",
            vec![
                "png".to_string(),
                "jpg".to_string(),
                "jpeg".to_string(),
                "webp".to_string(),
                "gif".to_string(),
            ],
        ),
        (r"(?i)PDF", vec!["pdf".to_string()]),
    ];

    let cleanup_re = Regex::new(r"(?i)\s*(파일|file|코드|code)\s*").unwrap();

    for (pattern, exts) in mappings {
        let re = Regex::new(pattern).unwrap();
        if re.is_match(text) {
            let cleaned = re.replace(text, "").trim().to_string();
            let cleaned = cleanup_re.replace_all(&cleaned, " ").trim().to_string();
            debug!(extensions = ?exts, "Extracted extension filter");
            return Some((QueryFilter::Extension(exts), cleaned));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_year_filter() {
        let (filter, remaining) = extract_date_filter("2024년에 작성한 React 프로젝트").unwrap();
        match filter {
            QueryFilter::DateRange { start, end } => {
                assert!(start > 0);
                assert!(end > start);
            }
            _ => panic!("Expected DateRange"),
        }
        assert!(remaining.contains("React"));
        assert!(!remaining.contains("2024"));
    }

    #[test]
    fn test_extension_filter() {
        let (filter, remaining) = extract_extension_filter("TypeScript 파일 중 API 관련").unwrap();
        match filter {
            QueryFilter::Extension(exts) => {
                assert!(exts.contains(&"ts".to_string()));
                assert!(exts.contains(&"tsx".to_string()));
            }
            _ => panic!("Expected Extension"),
        }
        assert!(remaining.contains("API"));
    }

    #[test]
    fn test_recent_days_filter() {
        let (filter, _) = extract_date_filter("최근 7일 동안 수정한 파일").unwrap();
        match filter {
            QueryFilter::DateRange { start, end } => {
                let now = chrono::Utc::now().timestamp();
                assert!(end - start <= 7 * 86400 + 1);
                assert!((end - now).abs() < 2);
            }
            _ => panic!("Expected DateRange"),
        }
    }
}
