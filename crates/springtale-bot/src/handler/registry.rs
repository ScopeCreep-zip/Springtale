use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use springtale_connector::registry::store::ConnectorRegistry;
use springtale_connector::tier::WasmTier;
use springtale_core::rule::engine::RuleEngine;
use springtale_store::StorageBackend;

use crate::error::BotError;

/// Context passed to every handler invocation.
pub struct HandlerContext {
    pub user_id: String,
    pub channel_id: String,
    pub store: Arc<dyn StorageBackend>,
    pub registry: Arc<RwLock<ConnectorRegistry>>,
    pub engine: Arc<RwLock<RuleEngine>>,
    /// Shared capability bridge — the single dispatch point for
    /// connector invocations from chat/command handlers (§16 /
    /// Phase 17). Routing through the bridge guarantees sentinel
    /// evaluation runs before every network call.
    pub capability_bridge: springtale_runtime::CapabilityBridge,
    /// Sentinel monitor — passed alongside the bridge so handlers that
    /// need to call `dispatch_action` directly (rather than via the
    /// bridge's bare `execute`) can uphold §6.10's "every action is
    /// sentinel-gated" guarantee.
    pub sentinel: Arc<springtale_sentinel::Sentinel>,
    /// Momentum tier for this invocation (§16). `None` for non-formation
    /// dispatch (chat commands, direct API); handlers default to the
    /// permissive `CapabilityChecker::new()` tier (Warming). Formation-
    /// scoped dispatch via `runtime::event_loop` sets it to the calling
    /// formation's `MomentumTier` mapped through `momentum_to_wasm_tier`.
    pub formation_tier: Option<WasmTier>,
}

/// Result returned by a handler.
pub struct HandlerResult {
    /// Response text to send back to the user.
    pub response: String,
}

/// Trait for command handlers.
#[async_trait]
pub trait Handler: Send + Sync + 'static {
    /// Execute the handler with the given arguments.
    async fn handle(&self, args: &str, ctx: &HandlerContext) -> Result<HandlerResult, BotError>;

    /// Short description for /help listing.
    fn description(&self) -> &str;

    /// Whether this handler is a builtin (cannot be overridden).
    fn is_builtin(&self) -> bool {
        false
    }
}

/// Registry of command name → handler.
pub struct HandlerRegistry {
    handlers: HashMap<String, Box<dyn Handler>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a handler. Returns error if trying to override a builtin.
    pub fn register(&mut self, name: String, handler: Box<dyn Handler>) -> Result<(), BotError> {
        let name_lower = name.to_lowercase();
        if let Some(existing) = self.handlers.get(&name_lower)
            && existing.is_builtin()
        {
            return Err(BotError::PermissionDenied(format!(
                "cannot override builtin command: {name_lower}"
            )));
        }
        self.handlers.insert(name_lower, handler);
        Ok(())
    }

    /// Get a handler by command name.
    pub fn get(&self, name: &str) -> Option<&dyn Handler> {
        self.handlers.get(&name.to_lowercase()).map(|h| h.as_ref())
    }

    /// List all registered commands with descriptions.
    /// Returns (name, description, is_builtin).
    pub fn list_commands(&self) -> Vec<(&str, &str, bool)> {
        let mut cmds: Vec<_> = self
            .handlers
            .iter()
            .map(|(name, handler)| (name.as_str(), handler.description(), handler.is_builtin()))
            .collect();
        cmds.sort_by_key(|(name, _, _)| *name);
        cmds
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    struct DummyHandler {
        builtin: bool,
    }

    #[async_trait]
    impl Handler for DummyHandler {
        async fn handle(
            &self,
            _args: &str,
            _ctx: &HandlerContext,
        ) -> Result<HandlerResult, BotError> {
            Ok(HandlerResult {
                response: "ok".into(),
            })
        }

        fn description(&self) -> &str {
            "dummy"
        }

        fn is_builtin(&self) -> bool {
            self.builtin
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = HandlerRegistry::new();
        registry
            .register("test".into(), Box::new(DummyHandler { builtin: false }))
            .unwrap();
        assert!(registry.get("test").is_some());
    }

    #[test]
    fn test_cannot_override_builtin() {
        let mut registry = HandlerRegistry::new();
        registry
            .register("help".into(), Box::new(DummyHandler { builtin: true }))
            .unwrap();
        let result = registry.register("help".into(), Box::new(DummyHandler { builtin: false }));
        assert!(result.is_err());
    }

    #[test]
    fn test_list_commands_sorted() {
        let mut registry = HandlerRegistry::new();
        registry
            .register("beta".into(), Box::new(DummyHandler { builtin: false }))
            .unwrap();
        registry
            .register("alpha".into(), Box::new(DummyHandler { builtin: false }))
            .unwrap();
        let cmds = registry.list_commands();
        assert_eq!(cmds[0].0, "alpha");
        assert_eq!(cmds[1].0, "beta");
    }
}
