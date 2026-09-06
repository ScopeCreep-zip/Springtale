//! Inbound webhook ingress — what a *verified* webhook payload means.
//!
//! Signature verification ([`crate::connector::trait_::Connector::verify_webhook`])
//! answers "is this payload genuine?". It does not answer "what is in
//! it?" — and the daemon used to answer that itself, with a `match` on
//! one connector name, so exactly one connector's webhooks could reach
//! the bot. Every other connector's webhook was receive-only.
//!
//! [`crate::connector::trait_::Connector::ingest_webhook`] moves that
//! answer into the crate that owns the protocol. The ingress verifies,
//! then asks the connector what the payload means, and forwards the
//! result — no connector names in the daemon.

pub mod ingest;

pub use ingest::{WebhookEvent, WebhookIngest};
