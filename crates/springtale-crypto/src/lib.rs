#![deny(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod error;
pub mod identity;
pub mod message;
pub mod mlock;
pub mod secret_use;
pub mod signature;
pub mod token;
pub mod vault;
