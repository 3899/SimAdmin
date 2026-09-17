use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};

/// 手机号码规范化：剥离非数字符号，清除 +86/0086/86 前缀
pub fn normalize_phone_number(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut chars = Vec::new();
    for (i, c) in trimmed.chars().enumerate() {
        if c.is_ascii_digit() {
            chars.push(c);
        } else if c == '+' && i == 0 {
            chars.push(c);
        }
    }
    let s: String = chars.into_iter().collect();
    if let Some(stripped) = s.strip_prefix("+86") {
        return stripped.to_string();
    }
    if let Some(stripped) = s.strip_prefix("0086") {
        return stripped.to_string();
    }
    if s.starts_with("86") && s.len() == 13 {
        return s[2..].to_string();
    }
    s
}

/// 跨前缀号码等价性判定
pub fn is_phone_number_match(phone_a: &str, phone_b: &str) -> bool {
    let norm_a = normalize_phone_number(phone_a);
    let norm_b = normalize_phone_number(phone_b);
    if norm_a.is_empty() || norm_b.is_empty() {
        return false;
    }
    if norm_a == norm_b {
        return true;
    }
    if norm_a.len() >= 11 && norm_b.len() >= 11 {
        let tail_a = &norm_a[norm_a.len() - 11..];
        let tail_b = &norm_b[norm_b.len() - 11..];
        if tail_a == tail_b {
            return true;
        }
    }
    false
}

/// 解析异构时间戳为 UTC
pub fn parse_and_normalize_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d %H:%M",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            let beijing_offset = FixedOffset::east_opt(8 * 3600).unwrap();
            if let Some(local) = naive.and_local_timezone(beijing_offset).single() {
                return Some(local.with_timezone(&Utc));
            }
        }
    }
    None
}

/// 统一格式化为北京时间字符串
pub fn format_beijing_time(dt: &DateTime<Utc>) -> String {
    let beijing_offset = FixedOffset::east_opt(8 * 3600).unwrap();
    dt.with_timezone(&beijing_offset)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_phone() {
        assert_eq!(normalize_phone_number("+86138 0013 8000"), "13800138000");
        assert_eq!(normalize_phone_number("0086-138-0013-8000"), "13800138000");
        assert_eq!(normalize_phone_number("8613800138000"), "13800138000");
        assert_eq!(normalize_phone_number("95588"), "95588");
        assert_eq!(normalize_phone_number("1069000000"), "1069000000");
    }

    #[test]
    fn test_is_phone_number_match() {
        assert!(is_phone_number_match("+8613800138000", "13800138000"));
        assert!(is_phone_number_match("008613800138000", "+86 138-0013-8000"));
        assert!(is_phone_number_match("95588", "95588"));
        assert!(!is_phone_number_match("95588", "95533"));
    }

    #[test]
    fn test_parse_timestamp() {
        let dt = parse_and_normalize_timestamp("2026-09-17 18:00:00").unwrap();
        let beijing = format_beijing_time(&dt);
        assert_eq!(beijing, "2026-09-17 18:00:00");
    }
}
