use chrono::{DateTime, Datelike, NaiveTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatcherOperator {
    Always,
    Contains,
    NotContains,
    Equals,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatcher {
    #[serde(default)]
    pub field: String,
    #[serde(default = "default_operator")]
    pub operator: MatcherOperator,
    #[serde(default)]
    pub value: String,
}

fn default_operator() -> MatcherOperator {
    MatcherOperator::Always
}

pub fn evaluate_rule_matcher(field_value: &str, operator: MatcherOperator, expected: &str) -> bool {
    let expected = expected.trim();
    match operator {
        MatcherOperator::Always => true,
        MatcherOperator::Contains => {
            expected.is_empty() || field_value.to_lowercase().contains(&expected.to_lowercase())
        }
        MatcherOperator::NotContains => {
            expected.is_empty() || !field_value.to_lowercase().contains(&expected.to_lowercase())
        }
        MatcherOperator::Equals => field_value.trim() == expected,
        MatcherOperator::Regex => {
            if expected.is_empty() {
                true
            } else {
                regex_automata::meta::Regex::new(expected)
                    .map(|re| re.is_match(field_value.as_bytes()))
                    .unwrap_or(false)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuietHoursSchedule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub weekdays: Vec<u8>,
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub end: String,
}

pub fn is_in_quiet_hours(schedules: &[QuietHoursSchedule], dt: &DateTime<Utc>) -> bool {
    let beijing_offset = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let local = dt.with_timezone(&beijing_offset);
    let weekday = local.weekday().number_from_monday() as u8;
    let current = local.time();

    schedules.iter().any(|schedule| {
        if !schedule.enabled || !schedule.weekdays.contains(&weekday) {
            return false;
        }
        let (Ok(start), Ok(end)) = (
            NaiveTime::parse_from_str(&schedule.start, "%H:%M"),
            NaiveTime::parse_from_str(&schedule.end, "%H:%M"),
        ) else {
            return false;
        };

        if start <= end {
            current >= start && current <= end
        } else {
            // 跨午夜
            current >= start || current <= end
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_matcher() {
        assert!(evaluate_rule_matcher("hello world", MatcherOperator::Contains, "WORLD"));
        assert!(evaluate_rule_matcher("hello world", MatcherOperator::NotContains, "foo"));
        assert!(evaluate_rule_matcher("test", MatcherOperator::Equals, "test"));
        assert!(evaluate_rule_matcher("code is 123456", MatcherOperator::Regex, r"\d{6}"));
    }
}
