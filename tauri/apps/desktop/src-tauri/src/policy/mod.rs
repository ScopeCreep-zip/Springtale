//! Pure safety-surface decisions — no Tauri, no tokio, no OS calls.
//!
//! The desktop shell's job is to *apply* safety state to the operating
//! system: retitle a window, swap a tray icon, arm a hotkey, start a
//! countdown. Deciding *what* to apply is ordinary logic, and keeping that
//! logic here — away from the `AppHandle`s and the `tokio::spawn`s — is what
//! makes it testable. Every module in `commands/` and `autolock.rs` stays a
//! thin wrapper: read the decision from here, hand it to the OS.
//!
//! Nothing in this tree may take a `tauri::` type as an argument.

pub mod autolock;
pub mod disguise;
pub mod shortcut;
