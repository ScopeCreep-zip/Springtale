//! Per-locale sentence templates for the platform verbs (plan 5.4).

pub mod catalog;

pub use catalog::{LOCALES, SentenceCatalog, english, for_locale};
