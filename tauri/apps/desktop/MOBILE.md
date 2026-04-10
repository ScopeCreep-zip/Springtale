# Springtale Mobile Setup

## Prerequisites

Per [Tauri 2 mobile docs](https://v2.tauri.app/start/prerequisites/):

### Install Tauri CLI
```bash
cargo install tauri-cli
```

### Android
```bash
# Add Rust Android targets
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android

# Install Android SDK + NDK via Android Studio
# Set ANDROID_HOME and ANDROID_NDK_HOME environment variables
```

### iOS (macOS only)
```bash
# Add Rust iOS targets
rustup target add aarch64-apple-ios aarch64-apple-ios-sim

# Install Xcode from App Store
# Accept Xcode license: sudo xcodebuild -license accept
```

## Scaffold

```bash
cd tauri/apps/desktop

# Android
cargo tauri android init

# iOS (macOS only)
cargo tauri ios init
```

This generates `src-tauri/gen/android/` and `src-tauri/gen/ios/` project files.

## Build

```bash
# Android (debug)
cargo tauri android dev

# iOS (debug, macOS only)
cargo tauri ios dev

# Android (release APK)
cargo tauri android build

# iOS (release IPA, macOS only)
cargo tauri ios build
```

## WASM Sandbox on Mobile

- **Android**: Wasmtime Cranelift works natively on aarch64. JIT compilation permitted.
- **iOS**: JIT compilation blocked by Apple. Use AOT mode:
  - Precompile `.wasm` → `.cwasm` on build server
  - Load with `Module::deserialize()` on device
  - Per [Wasmtime docs](https://docs.wasmtime.dev/stability-platform-support.html)

## Notes

- The SolidJS frontend uses pointer events (touch-compatible)
- Colony canvas renders with CSS absolute positioning (works on all viewports)
- Config panels use `max-height: 80vh` + `overflow-y: auto` (scrollable on small screens)
- The TeamBuilder OOBE is single-panel (no screen switching)
- SQLite database uses `rusqlite` with bundled SQLite (no platform-specific setup)
- TLS uses `rustls` (no native OpenSSL dependency)
