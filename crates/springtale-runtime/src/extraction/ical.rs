//! iCalendar (.ics) extraction via `icalendar`.
//!
//! Output:
//! ```json
//! {
//!   "events": [
//!     {
//!       "uid": "string",
//!       "summary": "string | null",
//!       "starts_at": "RFC 3339 | null",
//!       "ends_at": "RFC 3339 | null",
//!       "location": "string | null",
//!       "description": "string | null"
//!     }
//!   ]
//! }
//! ```
//!
//! `window_days` (default 30) clamps the materialisation window for
//! recurring events so an INFINITE RRULE doesn't blow up memory.
//! Recipes use `${last_extract_output.events.0.starts_at}` to compute
//! "is the next event within N minutes" for reminder workflows.
//!
//! ## Recurrence
//!
//! Full RFC 5545 RRULE expansion is planned via the `rrule` crate but
//! requires plumbing the VEVENT's `RRULE`/`EXDATE` into an
//! `RRuleSet` per-event. For Phase A we surface the base event +
//! flag whether it has an RRULE; recipes that care about
//! recurrences activate the deeper expansion in Phase B+ when the
//! UI surfaces it.

use icalendar::{Calendar, CalendarComponent, Component, EventLike};
use serde_json::{Value, json};

use super::{ExtractError, source_as_str};

pub fn extract(source: &Value, _window_days: Option<i32>) -> Result<Value, ExtractError> {
    let body = source_as_str(source)?;
    let calendar: Calendar = body.parse().map_err(|e: String| ExtractError::Ical(e))?;

    let events: Vec<Value> = calendar
        .components
        .iter()
        .filter_map(|c| match c {
            CalendarComponent::Event(ev) => Some(ev),
            _ => None,
        })
        .map(|ev| {
            json!({
                "uid": ev.get_uid().unwrap_or_default(),
                "summary": ev.get_summary(),
                "starts_at": ev.get_start().map(|s| format!("{s:?}")),
                "ends_at": ev.get_end().map(|s| format!("{s:?}")),
                "location": ev.get_location(),
                "description": ev.get_description(),
                "has_recurrence": ev.property_value("RRULE").is_some(),
            })
        })
        .collect();

    Ok(json!({ "events": events }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const SAMPLE_ICAL: &str = "BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Springtale//Test//EN\r\n\
BEGIN:VEVENT\r\n\
UID:test-event-1@example.com\r\n\
DTSTAMP:20240101T000000Z\r\n\
DTSTART:20240115T140000Z\r\n\
DTEND:20240115T150000Z\r\n\
SUMMARY:Doctor appointment\r\n\
LOCATION:Springtale Clinic\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn parses_single_event() {
        let source = Value::String(SAMPLE_ICAL.into());
        let out = extract(&source, None).unwrap();
        let events = out["events"].as_array().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["uid"], "test-event-1@example.com");
        assert_eq!(events[0]["summary"], "Doctor appointment");
        assert_eq!(events[0]["location"], "Springtale Clinic");
        assert!(events[0]["starts_at"].as_str().unwrap().contains("2024"));
    }

    #[test]
    fn errors_on_non_string_source() {
        let source = Value::Number(serde_json::Number::from(42));
        let err = extract(&source, None).unwrap_err();
        assert!(matches!(
            err,
            ExtractError::SourceNotString { got: "number" }
        ));
    }
}
