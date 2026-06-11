//! Time-of-day grammar extractor (no ML).
//!
//! Maps the way people actually say times — "8am", "8 am", "8:30",
//! "at 7", "morning", "noon", "tonight" — onto a 24-hour
//! [`TimeOfDay`]. This is the "feel like an LLM" move for schedules:
//! the user never types cron, they say "every morning" and the
//! dialogue snaps it to the recipe's 8 AM preset.

/// A wall-clock time of day in 24h form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
}

/// Named day-parts → their canonical hour. Ordered so the matcher can
/// scan the raw text for any of these words.
const NAMED: &[(&str, u8)] = &[
    ("morning", 8),
    ("noon", 12),
    ("midday", 12),
    ("afternoon", 14),
    ("evening", 18),
    ("tonight", 20),
    ("night", 21),
    ("midnight", 0),
];

/// Extract the first time-of-day mentioned in `text`, if any.
pub fn parse_time(text: &str) -> Option<TimeOfDay> {
    let lower = text.to_lowercase();

    // 1) Clock times: scan whitespace tokens (keeping ':' intact, which
    //    word tokenization would strip) for "8am", "8:30", "8:30pm", and
    //    the "<n> am/pm" two-token form. A BARE integer is NOT a time on
    //    its own — only with an am/pm suffix or a ':' — so "keep last 7
    //    items" never becomes 7:00.
    let toks: Vec<&str> = lower
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric() && c != ':'))
        .collect();
    for (i, tok) in toks.iter().enumerate() {
        if let Some(t) = parse_clock_token(tok) {
            return Some(t);
        }
        // "<number> am" / "<number> pm" split across tokens, OR a bare
        // hour introduced by "at"/"by"/"around" ("at 7" → 07:00). A bare
        // number with neither cue is left alone ("last 7 items").
        if let Ok(h) = tok.parse::<u8>()
            && h <= 23
        {
            if let Some(next) = toks.get(i + 1) {
                if *next == "am" {
                    return Some(TimeOfDay {
                        hour: if h == 12 { 0 } else { h },
                        minute: 0,
                    });
                }
                if *next == "pm" {
                    return Some(TimeOfDay {
                        hour: if h == 12 { 12 } else { (h % 12) + 12 },
                        minute: 0,
                    });
                }
            }
            if i > 0 && matches!(toks[i - 1], "at" | "by" | "around") {
                return Some(TimeOfDay { hour: h, minute: 0 });
            }
        }
    }

    // 2) Named day-parts.
    for (word, hour) in NAMED {
        if lower.contains(word) {
            return Some(TimeOfDay {
                hour: *hour,
                minute: 0,
            });
        }
    }
    None
}

/// Parse a single fused clock token: "8", "8am", "8:30", "830pm",
/// "8:30am". Returns None if it isn't clock-shaped.
fn parse_clock_token(tok: &str) -> Option<TimeOfDay> {
    let (body, suffix) = if let Some(b) = tok.strip_suffix("am") {
        (b, Some(false))
    } else if let Some(b) = tok.strip_suffix("pm") {
        (b, Some(true))
    } else {
        (tok, None)
    };
    if body.is_empty() {
        return None;
    }

    let has_colon = body.contains(':');
    // A bare integer with neither am/pm nor ':' is NOT a clock time.
    if suffix.is_none() && !has_colon {
        return None;
    }

    let (mut hour, minute) = if let Some((h, m)) = body.split_once(':') {
        (h.parse::<u8>().ok()?, m.parse::<u8>().ok()?)
    } else {
        let h = body.parse::<u8>().ok()?;
        (h, 0)
    };
    if minute > 59 {
        return None;
    }

    match suffix {
        Some(true) => {
            // pm
            hour = if hour == 12 { 12 } else { (hour % 12) + 12 };
        }
        Some(false) => {
            // am
            hour = if hour == 12 { 0 } else { hour };
        }
        None => {}
    }
    if hour > 23 {
        return None;
    }
    Some(TimeOfDay { hour, minute })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_named_morning() {
        assert_eq!(
            parse_time("every morning").unwrap(),
            TimeOfDay { hour: 8, minute: 0 }
        );
    }

    #[test]
    fn test_fused_am_pm() {
        assert_eq!(
            parse_time("at 8am").unwrap(),
            TimeOfDay { hour: 8, minute: 0 }
        );
        assert_eq!(
            parse_time("at 9pm").unwrap(),
            TimeOfDay {
                hour: 21,
                minute: 0
            }
        );
        assert_eq!(
            parse_time("12am").unwrap(),
            TimeOfDay { hour: 0, minute: 0 }
        );
        assert_eq!(
            parse_time("12pm").unwrap(),
            TimeOfDay {
                hour: 12,
                minute: 0
            }
        );
    }

    #[test]
    fn test_split_number_meridiem() {
        assert_eq!(
            parse_time("send it 7 am").unwrap(),
            TimeOfDay { hour: 7, minute: 0 }
        );
    }

    #[test]
    fn test_colon_minutes() {
        assert_eq!(
            parse_time("at 8:30am").unwrap(),
            TimeOfDay {
                hour: 8,
                minute: 30
            }
        );
        assert_eq!(
            parse_time("at 17:45").unwrap(),
            TimeOfDay {
                hour: 17,
                minute: 45
            }
        );
    }

    #[test]
    fn test_no_time() {
        assert!(parse_time("weather in tucson").is_none());
    }
}
