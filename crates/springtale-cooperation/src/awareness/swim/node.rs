//! `SwimNode` — the driver task wrapping foca + AccumulatingRuntime
//! behind a UDP socket.
//!
//! Per COOPERATION.md §8.3 the node lives one-per-process. On creation
//! it binds a UDP socket, spawns a tokio task that:
//!
//! 1. `tokio::select!`s on incoming UDP datagrams + scheduled timers
//! 2. Drives foca via `handle_data` / `handle_timer`
//! 3. Drains `AccumulatingRuntime` each pass:
//!    - `to_send()` → UDP send
//!    - `to_schedule()` → spawn a delayed task that forwards the Timer
//!      back to the main loop
//!    - `to_notify()` → convert to `SwimEvent`, broadcast to subscribers
//!
//! The driver owns the foca instance, so all foca calls are serialized
//! through the main loop. Subscribers see `SwimEvent`s via a
//! `broadcast::Receiver` — clone via `subscribe()` as many times as
//! needed.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use bincode::config::Configuration;
use bytes::BytesMut;
use foca::{
    AccumulatingRuntime, BincodeCodec, Config, Foca, NoCustomBroadcast, OwnedNotification, Timer,
};
use rand::rngs::StdRng;
use rand::SeedableRng;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use crate::error::CooperationError;

use super::events::{SwimEvent, SwimSelfState};
use super::identity::ProcId;

const DATAGRAM_BUF: usize = 2048;
const SPAWN_CHANNEL_CAP: usize = 256;
const TIMER_CHANNEL_CAP: usize = 256;
const EVENT_CHANNEL_CAP: usize = 256;

/// Configuration for spawning a foca SWIM node.
#[derive(Debug, Clone)]
pub struct SwimNodeConfig {
    /// Local UDP listen address (host:port). This is also the node's
    /// advertised address in foca gossip.
    pub listen: std::net::SocketAddr,
    /// Peer addresses to announce to at startup. Can be empty (the
    /// node will accept inbound announcements from other seeds).
    pub seeds: Vec<std::net::SocketAddr>,
    /// Expected cluster size. `new_lan(10)` is a reasonable default.
    pub cluster_size: NonZeroU32,
}

impl Default for SwimNodeConfig {
    fn default() -> Self {
        Self {
            // 127.0.0.1:0 → kernel assigns an ephemeral port. Built
            // infallibly from IPv4 LOCALHOST + port 0 so the lib-level
            // `clippy::expect_used` deny stays clean.
            listen: std::net::SocketAddr::V4(std::net::SocketAddrV4::new(
                std::net::Ipv4Addr::LOCALHOST,
                0,
            )),
            seeds: Vec::new(),
            // 10 is non-zero; fall back to 1 (the type's `MIN`) in the
            // unreachable `None` arm rather than `.expect()`.
            cluster_size: NonZeroU32::new(10).unwrap_or(NonZeroU32::MIN),
        }
    }
}

/// Running SWIM node. Dropping this aborts the driver task — no
/// explicit shutdown call required.
pub struct SwimNode {
    events: broadcast::Sender<SwimEvent>,
    task: JoinHandle<()>,
    local_addr: std::net::SocketAddr,
}

impl SwimNode {
    /// Spawn a new SWIM node. Binds the UDP socket, announces to seeds,
    /// and starts the driver task.
    pub async fn spawn(cfg: SwimNodeConfig) -> Result<Self, CooperationError> {
        let socket = UdpSocket::bind(cfg.listen)
            .await
            .map_err(|e| CooperationError::Liveness(format!("bind {}: {}", cfg.listen, e)))?;
        let local_addr = socket.local_addr().map_err(|e| {
            CooperationError::Liveness(format!("local_addr: {e}"))
        })?;
        let socket = Arc::new(socket);

        let me = ProcId::new(local_addr);
        let config = Config::new_lan(cfg.cluster_size);
        // bincode 2.x `standard()` — matches foca 1.0's internal dep.
        let codec: BincodeCodec<Configuration> =
            BincodeCodec(bincode::config::standard());
        // rand 0.9: `from_entropy` was renamed `from_os_rng`.
        let rng = StdRng::from_os_rng();
        let mut foca: Foca<ProcId, _, _, NoCustomBroadcast> =
            Foca::new(me, config, rng, codec);

        let mut rt = AccumulatingRuntime::new();

        // Announce to seeds up front. Any seed that's live will include
        // us in its gossip within one probe cycle.
        for seed in cfg.seeds {
            let seed_id = ProcId::new(seed);
            if let Err(e) = foca.announce(seed_id, &mut rt) {
                tracing::warn!(?seed, error = %e, "foca announce to seed failed");
            }
        }

        let (events_tx, _) = broadcast::channel(EVENT_CHANNEL_CAP);

        let task = tokio::spawn(run_driver(
            foca,
            rt,
            socket,
            events_tx.clone(),
        ));

        Ok(Self {
            events: events_tx,
            task,
            local_addr,
        })
    }

    /// Subscribe to lifecycle events. Lagging receivers drop oldest
    /// events per tokio broadcast semantics.
    pub fn subscribe(&self) -> broadcast::Receiver<SwimEvent> {
        self.events.subscribe()
    }

    /// The actual bound address (after ephemeral-port resolution).
    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.local_addr
    }
}

impl Drop for SwimNode {
    fn drop(&mut self) {
        // Aborting the driver task also drops the UDP socket via
        // `Arc` refcount — no further packets accepted.
        self.task.abort();
    }
}

/// Main driver loop: reads UDP datagrams, fires foca timers,
/// drains `AccumulatingRuntime`, emits `SwimEvent`s.
async fn run_driver(
    mut foca: Foca<
        ProcId,
        BincodeCodec<Configuration>,
        StdRng,
        NoCustomBroadcast,
    >,
    mut rt: AccumulatingRuntime<ProcId>,
    socket: Arc<UdpSocket>,
    events: broadcast::Sender<SwimEvent>,
) {
    // Channel: scheduler → main loop. `to_schedule()` yields
    // `(duration, timer)`; we spawn a task that sleeps and forwards
    // the timer back here for `handle_timer`.
    let (sched_tx, mut sched_rx) =
        mpsc::channel::<(Duration, Timer<ProcId>)>(SPAWN_CHANNEL_CAP);
    let (timer_tx, mut timer_rx) = mpsc::channel::<Timer<ProcId>>(TIMER_CHANNEL_CAP);

    // Scheduler task: takes (Duration, Timer) pairs, sleeps, then
    // forwards the Timer to the main loop. Per foca docs this is the
    // intended pattern — AccumulatingRuntime hands us requests to
    // defer work, and we're responsible for actually deferring.
    let scheduler_tx = timer_tx.clone();
    let scheduler = tokio::spawn(async move {
        while let Some((dur, timer)) = sched_rx.recv().await {
            let fwd = scheduler_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(dur).await;
                let _ = fwd.send(timer).await;
            });
        }
    });

    // Drain the runtime once up front (for the initial announce(seeds)
    // work queued during spawn).
    drain_runtime(&mut rt, &socket, &sched_tx, &events).await;

    let mut buf = BytesMut::zeroed(DATAGRAM_BUF);

    loop {
        tokio::select! {
            // Inbound UDP datagram.
            res = socket.recv_from(&mut buf) => {
                match res {
                    Ok((n, _peer)) => {
                        let data = &buf[..n];
                        if let Err(e) = foca.handle_data(data, &mut rt) {
                            tracing::debug!(error = %e, "foca handle_data error");
                        }
                        drain_runtime(&mut rt, &socket, &sched_tx, &events).await;
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "SWIM UDP recv_from failed");
                        // Keep the loop — transient errors are expected on
                        // network flaps. Unrecoverable errors (socket closed)
                        // manifest as the socket's Arc being dropped, which
                        // only happens when the SwimNode itself is dropped.
                    }
                }
            }
            // Scheduled timer matured.
            Some(timer) = timer_rx.recv() => {
                if let Err(e) = foca.handle_timer(timer, &mut rt) {
                    tracing::debug!(error = %e, "foca handle_timer error");
                }
                drain_runtime(&mut rt, &socket, &sched_tx, &events).await;
            }
            // All senders dropped → driver shuts down.
            else => break,
        }
    }

    scheduler.abort();
    tracing::debug!("SWIM driver exiting");
}

/// Empty every queue that `AccumulatingRuntime` exposes:
/// - `to_send` → UDP send
/// - `to_schedule` → forward to scheduler task
/// - `to_notify` → convert OwnedNotification → SwimEvent and broadcast
async fn drain_runtime(
    rt: &mut AccumulatingRuntime<ProcId>,
    socket: &UdpSocket,
    sched: &mpsc::Sender<(Duration, Timer<ProcId>)>,
    events: &broadcast::Sender<SwimEvent>,
) {
    while let Some((dst, data)) = rt.to_send() {
        if let Err(e) = socket.send_to(&data, dst.addr).await {
            tracing::debug!(peer = ?dst, error = %e, "SWIM send_to failed");
        }
    }
    while let Some((dur, timer)) = rt.to_schedule() {
        if sched.send((dur, timer)).await.is_err() {
            tracing::debug!("scheduler dropped while forwarding timer");
        }
    }
    while let Some(notification) = rt.to_notify() {
        let Some(event) = notification_to_event(notification) else {
            continue;
        };
        // Broadcast send errors mean zero subscribers — not a problem.
        let _ = events.send(event);
    }
}

fn notification_to_event(n: OwnedNotification<ProcId>) -> Option<SwimEvent> {
    match n {
        OwnedNotification::MemberUp(id) => Some(SwimEvent::MemberUp(id.addr)),
        OwnedNotification::MemberDown(id) => Some(SwimEvent::MemberDown(id.addr)),
        OwnedNotification::Rejoin(id) => Some(SwimEvent::MemberRejoined(id.addr)),
        OwnedNotification::Active => Some(SwimEvent::SelfState(SwimSelfState::Active)),
        OwnedNotification::Idle => Some(SwimEvent::SelfState(SwimSelfState::Idle)),
        OwnedNotification::Defunct => Some(SwimEvent::SelfState(SwimSelfState::Defunct)),
        // Rename fires when foca's own identity bumps — treated as a
        // Rejoin of the new identity.
        OwnedNotification::Rename(_before, after) => Some(SwimEvent::MemberRejoined(after.addr)),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn swim_node_binds_ephemeral_port() {
        let node = SwimNode::spawn(SwimNodeConfig::default()).await.unwrap();
        // Ephemeral port ≠ 0 after bind.
        assert!(node.local_addr().port() > 0);
        assert!(node.local_addr().ip().is_loopback());
    }

    #[tokio::test]
    async fn subscribe_produces_receiver() {
        let node = SwimNode::spawn(SwimNodeConfig::default()).await.unwrap();
        let rx = node.subscribe();
        // No events yet — single-node cluster has nothing to probe.
        // But the receiver is valid for future events.
        assert_eq!(rx.len(), 0);
    }

    #[tokio::test]
    async fn two_nodes_exchange_member_up_events() {
        // Start node A, then node B announcing to A. Foca's announce
        // cycle should produce MemberUp on both sides within a few
        // hundred milliseconds.
        let a = SwimNode::spawn(SwimNodeConfig::default()).await.unwrap();
        let a_addr = a.local_addr();

        let b_cfg = SwimNodeConfig {
            listen: "127.0.0.1:0".parse().unwrap(),
            seeds: vec![a_addr],
            cluster_size: NonZeroU32::new(4).unwrap(),
        };
        let b = SwimNode::spawn(b_cfg).await.unwrap();
        let mut a_rx = a.subscribe();
        let mut b_rx = b.subscribe();

        // Wait up to 3s for MemberUp events on both sides.
        let a_saw_b = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match a_rx.recv().await {
                    Ok(SwimEvent::MemberUp(peer)) if peer == b.local_addr() => return true,
                    Ok(_) => continue,
                    Err(_) => return false,
                }
            }
        })
        .await
        .unwrap_or(false);

        let b_saw_a = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match b_rx.recv().await {
                    Ok(SwimEvent::MemberUp(peer)) if peer == a_addr => return true,
                    Ok(_) => continue,
                    Err(_) => return false,
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(a_saw_b, "node A should observe B up");
        assert!(b_saw_a, "node B should observe A up");
    }
}
