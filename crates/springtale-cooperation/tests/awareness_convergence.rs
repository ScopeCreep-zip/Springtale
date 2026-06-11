//! SWIM cluster convergence integration test (Phase-7 audit Finding C).
//!
//! Stands up an N-node foca/SWIM cluster on localhost-only ephemeral
//! UDP sockets, announces each non-seed node at the seed, and asserts
//! that every node eventually observes every other node as `MemberUp`
//! within a generous wall-clock budget. Convergence is the property
//! that gives the cooperation framework its "awareness" — the
//! pre-condition for capability advertisement, mental-model sync,
//! cadence handoffs, and rally formation.
//!
//! Per COOPERATION.pdf §8 awareness is bounded by foca's probe period
//! (Config::new_lan's default suspect timeout) which is on the order
//! of seconds; we give the test 15 seconds wall-clock to converge,
//! generous enough for CI but tight enough to catch a real regression
//! (a regressed gossip path stays empty indefinitely, not for 14s).

#![allow(clippy::unwrap_used)]

use std::collections::HashSet;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::time::Duration;

use springtale_cooperation::awareness::swim::events::SwimEvent;
use springtale_cooperation::awareness::swim::node::{SwimNode, SwimNodeConfig};
use tokio::sync::broadcast;
use tokio::time::{Instant, timeout};

const NODE_COUNT: usize = 3;
const CONVERGENCE_DEADLINE: Duration = Duration::from_secs(15);

async fn observe_member_ups(
    rx: &mut broadcast::Receiver<SwimEvent>,
    expected: usize,
    deadline: Instant,
) -> HashSet<SocketAddr> {
    let mut seen = HashSet::new();
    while seen.len() < expected {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match timeout(remaining, rx.recv()).await {
            Ok(Ok(SwimEvent::MemberUp(addr))) => {
                seen.insert(addr);
            }
            Ok(Ok(_)) => continue,
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(broadcast::error::RecvError::Closed)) => break,
            Err(_) => break,
        }
    }
    seen
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cluster_converges_on_membership() {
    // Spawn each node and IMMEDIATELY subscribe to its event bus
    // before the next node spawns + connects. Tokio's broadcast
    // channel drops sends when no receivers exist, so a late
    // subscribe after the first MemberUp has already fired would
    // miss it forever — foca only emits MemberUp on state
    // transitions, not on every probe. Subscribing in-line with
    // spawn shrinks the window to whatever lies between `events_tx`
    // being created in `SwimNode::spawn` and `subscribe()`
    // returning — under a microsecond on a healthy host.
    let seed = SwimNode::spawn(SwimNodeConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        seeds: Vec::new(),
        cluster_size: NonZeroU32::new(NODE_COUNT as u32).unwrap(),
    })
    .await
    .expect("seed node spawn");
    let seed_sub = seed.subscribe();
    let seed_addr = seed.local_addr();

    // Spawn the rest. Each one points back at the seed; foca's gossip
    // path will propagate the full membership list to every node.
    // Subscribe immediately after each spawn for the same reason.
    let mut others = Vec::new();
    let mut other_subs = Vec::new();
    for _ in 1..NODE_COUNT {
        let node = SwimNode::spawn(SwimNodeConfig {
            listen: "127.0.0.1:0".parse().unwrap(),
            seeds: vec![seed_addr],
            cluster_size: NonZeroU32::new(NODE_COUNT as u32).unwrap(),
        })
        .await
        .expect("non-seed spawn");
        other_subs.push(node.subscribe());
        others.push(node);
    }

    let mut subs: Vec<_> = std::iter::once(seed_sub).chain(other_subs).collect();
    let addrs: Vec<SocketAddr> = std::iter::once(seed_addr)
        .chain(others.iter().map(|n| n.local_addr()))
        .collect();

    let deadline = Instant::now() + CONVERGENCE_DEADLINE;
    let expected = NODE_COUNT - 1;

    for (i, sub) in subs.iter_mut().enumerate() {
        let observed = observe_member_ups(sub, expected, deadline).await;
        assert!(
            observed.len() >= expected,
            "node {} (addr {}) only observed {} peers, expected {}: {:?}",
            i,
            addrs[i],
            observed.len(),
            expected,
            observed
        );
        // Sanity: every observed peer must be one of the other nodes.
        for peer in &observed {
            assert!(
                addrs.contains(peer),
                "node {} reported unknown peer {peer}",
                i
            );
            assert_ne!(*peer, addrs[i], "node {} reported itself as a peer", i);
        }
    }
}
