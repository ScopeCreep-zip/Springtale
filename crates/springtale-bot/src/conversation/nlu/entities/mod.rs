//! Duckling-style grammar entity extractors — deterministic, no ML.
//!
//! Each submodule pulls one entity type out of free text so the
//! dialogue can pre-fill a recipe input from the same sentence that
//! named the recipe.

pub mod cron;
pub mod number;
pub mod onoff;
pub mod place;
pub mod time;
pub mod url;

pub use cron::{cron_hour, parse_schedule};
pub use number::parse_number;
pub use onoff::{is_affirmative, is_cancel, is_negative, parse_bool};
pub use place::parse_place;
pub use time::{TimeOfDay, parse_time};
pub use url::parse_url;
