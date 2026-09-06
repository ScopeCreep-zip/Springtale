/// Tauri command handlers — the OS-only surface.
///
/// Plan 2.1: everything that reads or writes Springtale state goes to the
/// `springtaled` sidecar over HTTP. What is left here is what only a
/// desktop shell can do: unlock the vault and start the daemon, drive the
/// window, the tray, the global hotkey and the element picker.
pub mod quick_hide;
pub mod safety;
pub mod selector_picker;
pub mod tray;
pub mod vault;
