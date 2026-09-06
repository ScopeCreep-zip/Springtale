//! Fixtures shared by the executor and agent-pipeline tests: a counting
//! mock connector with an optional per-call delay and an overlap probe,
//! plus the momentum/pacing warm-up the consensus round-trip needs.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::RwLock;

use springtale_connector::ConnectorError;
use springtale_connector::capability::grant::CapabilityPolicy;
use springtale_connector::connector::subscription::{Subscription, SubscriptionId};
use springtale_connector::connector::trait_::{ActionResult, Connector, EventHandler};
use springtale_connector::manifest::SignatureAlgorithm;
use springtale_connector::manifest::types::{ActionDecl, ConnectorManifest, TriggerDecl};
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_cooperation::TickId;
use springtale_cooperation::action::SubTask;
use springtale_cooperation::cadence::{AgentId, Tick, TickReport};
use springtale_cooperation::momentum::MomentumTier;
use springtale_cooperation::pacing::{PacingManager, PacingPhase};
use springtale_cooperation::tick_processor::FormationTickResult;
use springtale_runtime::CapabilityBridge;
use springtale_sentinel::{Sentinel, SentinelConfig};
use springtale_store::backend::InMemoryBackend;

use crate::cooperation::formation::Formation;

pub(crate) const CONNECTOR: &str = "consensus-target";

/// Counts executions and records how many ran at once.
#[derive(Default)]
pub(crate) struct Probe {
    pub executions: AtomicUsize,
    in_flight: AtomicUsize,
    pub max_in_flight: AtomicUsize,
}

impl Probe {
    pub fn executions(&self) -> usize {
        self.executions.load(Ordering::SeqCst)
    }

    pub fn max_in_flight(&self) -> usize {
        self.max_in_flight.load(Ordering::SeqCst)
    }
}

/// Mock connector whose single action `wipe` carries the given
/// `read_only` hint and sleeps `delay` before answering.
pub(crate) struct CountingConnector {
    manifest: ConnectorManifest,
    probe: Arc<Probe>,
    delay: Duration,
}

impl CountingConnector {
    pub fn new(probe: Arc<Probe>, read_only: bool, delay: Duration) -> Self {
        Self {
            manifest: ConnectorManifest {
                name: CONNECTOR.into(),
                version: "0.1.0".into(),
                author: "test".into(),
                description: "counts executions".into(),
                capabilities: vec![],
                triggers: vec![TriggerDecl {
                    name: "ping".into(),
                    description: "ping".into(),
                    schema: None,
                }],
                actions: vec![ActionDecl {
                    name: "wipe".into(),
                    description: "action gated by consensus unless read-only".into(),
                    input_schema: None,
                    output_schema: None,
                    read_only,
                    destructive: None,
                    poll_interval_secs: None,
                }],
                data_disclosure: vec![],
                roles: vec![],
                wasm_hash: None,
                signature_alg: SignatureAlgorithm::default(),
                signature: None,
            },
            probe,
            delay,
        }
    }
}

#[async_trait]
impl Connector for CountingConnector {
    fn triggers(&self) -> &[TriggerDecl] {
        &self.manifest.triggers
    }
    fn actions(&self) -> &[ActionDecl] {
        &self.manifest.actions
    }
    async fn execute(
        &self,
        action: &str,
        _input: serde_json::Value,
    ) -> Result<ActionResult, ConnectorError> {
        let now = self.probe.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.probe.max_in_flight.fetch_max(now, Ordering::SeqCst);
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        self.probe.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.probe.executions.fetch_add(1, Ordering::SeqCst);
        Ok(ActionResult {
            success: true,
            output: json!({ "executed": action }),
            message: "executed".into(),
        })
    }
    async fn on_event(
        &self,
        trigger: &str,
        _handler: EventHandler,
    ) -> Result<Subscription, ConnectorError> {
        Ok(Subscription {
            id: SubscriptionId(0),
            trigger: trigger.to_owned(),
        })
    }
    async fn remove_event(&self, _sub: &Subscription) -> Result<(), ConnectorError> {
        Ok(())
    }
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }
}

pub(crate) struct Runtime {
    pub probe: Arc<Probe>,
    pub registry: Arc<RwLock<ConnectorRegistry>>,
    pub bridge: CapabilityBridge,
    pub sentinel: Arc<Sentinel>,
    pub store: Arc<dyn springtale_store::StorageBackend>,
}

/// Registry + bridge + sentinel over one `CountingConnector`.
pub(crate) fn runtime(read_only: bool, delay: Duration) -> Runtime {
    let probe = Arc::new(Probe::default());
    let mut registry = ConnectorRegistry::new(CapabilityPolicy::AllowAll);
    registry
        .install_native(Box::new(CountingConnector::new(
            probe.clone(),
            read_only,
            delay,
        )))
        .unwrap();
    let registry = Arc::new(RwLock::new(registry));
    let bridge = CapabilityBridge::new(registry.clone());
    let store: Arc<dyn springtale_store::StorageBackend> = Arc::new(InMemoryBackend::new());
    let sentinel = Arc::new(Sentinel::new(SentinelConfig::default(), store.clone()));
    Runtime {
        probe,
        registry,
        bridge,
        sentinel,
        store,
    }
}

pub(crate) fn successful_tick_result(agent: AgentId) -> FormationTickResult {
    FormationTickResult {
        reports: vec![TickReport {
            agent_id: agent,
            tick_sequence: TickId(1),
            action_taken: Some(springtale_cooperation::cadence::ActionDescriptor {
                kind: "work".into(),
                target: None,
                payload_hash: 0,
            }),
            latency: Duration::from_millis(1),
            intent_alignment: 1.0,
            interference_with: vec![],
        }],
        interferences: vec![],
        all_succeeded: true,
    }
}

pub(crate) fn make_tick(sequence: u64, window: Duration) -> Tick {
    Tick {
        sequence: TickId(sequence),
        timestamp: Instant::now(),
        window,
    }
}

pub(crate) fn wipe_task() -> SubTask {
    SubTask {
        id: uuid::Uuid::new_v4(),
        target_connector: CONNECTOR.into(),
        action_name: "wipe".into(),
        params: json!({}),
        priority: 1,
        assigned_to: None,
        description: "wipe".into(),
        depends_on: vec![],
    }
}

/// Earn Fever momentum and Peak pacing (§7 / §22: capabilities are
/// earned, not granted). Returns the earned pacing manager.
pub(crate) fn earn_fever_and_peak(formation: &mut Formation) -> PacingManager {
    for _ in 0..15 {
        formation.momentum.record_success();
    }
    assert_eq!(formation.momentum.tier, MomentumTier::Fever);
    let driver = formation.members[0].agent_id;
    let mut pacing = PacingManager::default();
    for _ in 0..16 {
        pacing.evaluate_transition(
            &successful_tick_result(driver),
            &formation.momentum,
            Duration::from_millis(33),
        );
    }
    assert!(
        matches!(pacing.current_phase, PacingPhase::Peak { .. }),
        "formation earned Peak pacing (full tick rate + 30 actions/min)"
    );
    pacing
}
