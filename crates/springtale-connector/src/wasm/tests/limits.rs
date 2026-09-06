//! The sandbox limits fire.
//!
//! `wasm/runtime.rs` builds `StoreLimits` and `wasm/connector.rs` sets
//! the fuel budget and epoch deadline. These tests run real guests that
//! each breach one limit and assert the matching typed error, so a
//! regression that drops a limiter fails a test instead of silently
//! handing community connectors an unmetered sandbox.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use springtale_crypto::signature::SignatureAlgorithm;

use crate::error::ConnectorError;
use crate::manifest::types::ConnectorManifest;
use crate::wasm::connector::WasmConnectorHost;
use crate::wasm::limits::SandboxLimits;
use crate::wasm::runtime::WasmEngine;
use crate::wasm::tier::WasmTierCache;

/// 64 KiB — the WASM linear-memory page size.
const PAGE_BYTES: usize = 65_536;

/// Minimal manifest for a guest that declares no capabilities.
fn test_manifest() -> ConnectorManifest {
    ConnectorManifest {
        name: "connector-limits-test".into(),
        version: "0.1.0".into(),
        author: "test".into(),
        description: "sandbox limit guest".into(),
        capabilities: vec![],
        triggers: vec![],
        actions: vec![],
        data_disclosure: vec![],
        roles: vec![],
        wasm_hash: None,
        signature_alg: SignatureAlgorithm::default(),
        signature: None,
    }
}

/// Compile a WAT guest into a host running under `limits`.
fn host(limits: SandboxLimits, wat: &str) -> (Arc<WasmEngine>, WasmConnectorHost) {
    let engine = Arc::new(WasmEngine::new(limits.clone()).expect("engine"));
    let cache = Arc::new(WasmTierCache::new(engine.clone()).expect("tier cache"));
    let bytes = wat::parse_str(wat).expect("valid wat");
    let host = WasmConnectorHost::new(engine.clone(), &bytes, test_manifest(), limits, cache)
        .expect("host");
    (engine, host)
}

#[test]
fn fuel_exhaustion_traps() {
    let limits = SandboxLimits {
        fuel: 10_000,
        ..SandboxLimits::default()
    };
    let (_engine, host) = host(limits, r#"(module (func (export "spin") (loop br 0)))"#);

    let err = host
        .execute_raw("spin")
        .expect_err("infinite loop must trap");
    assert!(
        matches!(err, ConnectorError::FuelExhausted { limit, .. } if limit == 10_000),
        "{err:?}"
    );
}

#[test]
fn a_guest_under_the_fuel_budget_returns() {
    let limits = SandboxLimits {
        fuel: 10_000,
        ..SandboxLimits::default()
    };
    let (_engine, host) = host(limits, r#"(module (func (export "noop")))"#);

    assert!(host.execute_raw("noop").is_ok());
}

#[test]
fn memory_growth_past_limit_is_refused() {
    // Cap at 1 MiB so the guest's 1 MiB + 1 page request is one page over.
    let cap = 16 * PAGE_BYTES;
    let over = u32::try_from(cap / PAGE_BYTES + 1).expect("page count fits i32");
    let limits = SandboxLimits {
        memory_bytes: cap,
        ..SandboxLimits::default()
    };
    // `memory.grow` returns -1 when the limiter refuses; the guest traps
    // via `unreachable` if the growth was allowed, so a passing call is
    // itself proof the cap held.
    let wat = format!(
        r#"(module
             (memory (export "memory") 1)
             (func (export "grow_past_cap")
               (if (i32.ne (memory.grow (i32.const {over})) (i32.const -1))
                 (then unreachable))))"#
    );
    let (_engine, host) = host(limits, &wat);

    host.execute_raw("grow_past_cap")
        .expect("growth past the cap must be refused, not granted");
    let size = host
        .memory_size_after("grow_past_cap")
        .expect("memory size");
    assert!(size <= cap, "guest holds {size} bytes, cap is {cap}");
}

#[test]
fn memory_growth_within_limit_succeeds() {
    let cap = 16 * PAGE_BYTES;
    let limits = SandboxLimits {
        memory_bytes: cap,
        ..SandboxLimits::default()
    };
    let wat = r#"(module
                   (memory (export "memory") 1)
                   (func (export "grow_one") (drop (memory.grow (i32.const 1)))))"#;
    let (_engine, host) = host(limits, wat);

    let size = host.memory_size_after("grow_one").expect("memory size");
    assert_eq!(size, 2 * PAGE_BYTES);
}

#[test]
fn epoch_deadline_traps_without_fuel_running_out() {
    // Unbounded fuel isolates the epoch deadline: whatever stops this
    // guest, it is not the instruction meter.
    let limits = SandboxLimits {
        fuel: u64::MAX,
        timeout_secs: 1,
        ..SandboxLimits::default()
    };
    let (engine, host) = host(limits, r#"(module (func (export "spin") (loop br 0)))"#);

    // The deadline is measured in epoch ticks; something outside the
    // engine has to advance them. In production that is the daemon's
    // ticker task.
    let ticker = engine.engine().clone();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop = done.clone();
    let handle = thread::spawn(move || {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(20));
            ticker.increment_epoch();
        }
    });

    let err = host
        .execute_raw("spin")
        .expect_err("infinite loop must trap");
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    handle.join().expect("ticker thread");

    assert!(
        matches!(err, ConnectorError::Timeout { limit_secs } if limit_secs == 1),
        "{err:?}"
    );
}
