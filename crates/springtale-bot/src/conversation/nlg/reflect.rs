//! Small reflection + redaction helpers for natural phrasing.
//!
//! "Reflection" in the ELIZA sense — echoing the user's own words back
//! ("got it, *Tucson*") makes a deterministic bot feel like it's
//! listening. Redaction guarantees a `Secret` slot value is never
//! echoed, even in a confirmation summary.

/// How a slot value should be displayed back to the user.
///
/// For `Select` slots this is the human label ("Tucson", "8:00 AM"),
/// NOT the stored value (lat/long, cron) — the dialogue passes the
/// label. For `Secret` slots it is always the mask.
pub fn display_value(raw: &str, secret: bool) -> String {
    if secret {
        return "••••••".to_owned();
    }
    raw.to_owned()
}

/// Wrap a short fragment for gentle emphasis in a reply.
pub fn emphasize(fragment: &str) -> String {
    format!("**{fragment}**")
}

/// Join a list of options into a natural "7, 8, or 9" phrase.
pub fn or_list(items: &[String]) -> String {
    natural_list(items, "or")
}

/// Join a list into a natural "X, Y, and Z" phrase.
pub fn and_list(items: &[String]) -> String {
    natural_list(items, "and")
}

fn natural_list(items: &[String], conj: &str) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} {conj} {b}"),
        _ => {
            let (last, head) = match items.split_last() {
                Some(parts) => parts,
                None => return String::new(),
            };
            format!("{}, {conj} {last}", head.join(", "))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_is_masked() {
        assert_eq!(display_value("super-secret-token", true), "••••••");
        assert_eq!(display_value("Tucson", false), "Tucson");
    }

    #[test]
    fn test_or_list() {
        assert_eq!(
            or_list(&["7:00 AM".into(), "8:00 AM".into(), "9:00 AM".into()]),
            "7:00 AM, 8:00 AM, or 9:00 AM"
        );
        assert_eq!(or_list(&["yes".into(), "no".into()]), "yes or no");
        assert_eq!(or_list(&["only".into()]), "only");
    }
}
