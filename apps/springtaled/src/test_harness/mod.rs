//! Shared test fixture for the management API.
//!
//! Both `apps/springtaled/tests` and `apps/springtale-cli/tests` need the
//! same `AppState`: in-memory store, NoopAdapter, a derived API token.
//! Keeping one copy here is what lets the CLI suite drive the *real*
//! daemon instead of a stub, which is the whole point of plan §2.2.
//!
//! Gated behind the `test-harness` feature so a normal build never
//! compiles it.

pub mod app;
pub mod lock;
pub mod server;

pub use app::TestApp;
pub use lock::{TEST_PASSPHRASE, TestGuard};
pub use server::TestServer;
