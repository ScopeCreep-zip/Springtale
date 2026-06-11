//! Frequency + time → cron grammar (no ML).
//!
//! Turns "every morning", "daily at 9am", "weekdays at 7", "every
//! monday" into a 5-field cron string for free `Cron` inputs. For
//! `Select`-backed schedules (the common case — recipes ship preset
//! cron options), the dialogue uses [`super::time::parse_time`] plus
//! [`cron_hour`] to snap onto the nearest preset instead of emitting a
//! raw expression. Default time when a frequency is named without one
//! is 9 AM (the conventional "daily digest" hour).

use crate::conversation::nlu::normalize::raw_tokens;

use super::time::{self, TimeOfDay};

/// Day-of-week tokens → cron DOW field value.
const DOW: &[(&str, &str)] = &[
    ("sunday", "0"),
    ("monday", "1"),
    ("tuesday", "2"),
    ("wednesday", "3"),
    ("thursday", "4"),
    ("friday", "5"),
    ("saturday", "6"),
];

/// Parse a schedule phrase into a cron expression. Returns None when
/// the text carries no schedule signal at all.
pub fn parse_schedule(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let toks = raw_tokens(&lower);

    let time = time::parse_time(&lower).unwrap_or(TimeOfDay { hour: 9, minute: 0 });

    // Specific weekday → weekly on that day.
    for (word, dow) in DOW {
        if toks.iter().any(|t| t == word) {
            return Some(format!("{} {} * * {}", time.minute, time.hour, dow));
        }
    }

    // Weekdays / weekends.
    if lower.contains("weekday") || lower.contains("week day") {
        return Some(format!("{} {} * * 1-5", time.minute, time.hour));
    }
    if lower.contains("weekend") {
        return Some(format!("{} {} * * 0,6", time.minute, time.hour));
    }

    // Weekly (no specific day) → Monday.
    if lower.contains("weekly") || lower.contains("every week") {
        return Some(format!("{} {} * * 1", time.minute, time.hour));
    }

    // Daily — explicit, or implied by a day-part / "every <time>".
    let daily = lower.contains("daily")
        || lower.contains("every day")
        || lower.contains("each day")
        || lower.contains("every morning")
        || lower.contains("every evening")
        || lower.contains("every night");
    if daily {
        return Some(format!("{} {} * * *", time.minute, time.hour));
    }

    // A bare time with no frequency word still reads as "every day at X"
    // in this product (the recipes are recurring by nature).
    if time::parse_time(&lower).is_some() {
        return Some(format!("{} {} * * *", time.minute, time.hour));
    }

    None
}

/// Extract the hour field from a 5- or 6-field cron expression.
/// Used to snap a parsed [`TimeOfDay`] onto a recipe's preset options.
pub fn cron_hour(expr: &str) -> Option<u8> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    // 5-field: min hour dom month dow → hour at index 1.
    // 6-field: sec min hour ... → hour at index 2.
    let idx = match fields.len() {
        5 => 1,
        6 => 2,
        _ => return None,
    };
    fields.get(idx).and_then(|h| h.parse::<u8>().ok())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_every_morning() {
        assert_eq!(parse_schedule("every morning").unwrap(), "0 8 * * *");
    }

    #[test]
    fn test_daily_at_time() {
        assert_eq!(parse_schedule("daily at 9am").unwrap(), "0 9 * * *");
    }

    #[test]
    fn test_weekday_schedule() {
        assert_eq!(parse_schedule("weekdays at 7").unwrap(), "0 7 * * 1-5");
    }

    #[test]
    fn test_specific_weekday() {
        assert_eq!(parse_schedule("every monday at 5pm").unwrap(), "0 17 * * 1");
    }

    #[test]
    fn test_bare_time_is_daily() {
        assert_eq!(parse_schedule("at 8am").unwrap(), "0 8 * * *");
    }

    #[test]
    fn test_no_schedule() {
        assert!(parse_schedule("weather in tucson").is_none());
    }

    #[test]
    fn test_cron_hour_extraction() {
        assert_eq!(cron_hour("0 8 * * *"), Some(8));
        assert_eq!(cron_hour("0 0 9 * * *"), Some(9));
        assert_eq!(cron_hour("garbage"), None);
    }
}
