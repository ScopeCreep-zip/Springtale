//! Shared environment — mutable workspace for formation members.
//!
//! Per COOPERATION.pdf §10:
//! Game sources: Siege destructible walls, Divinity surface system.
//!
//! Agents chain through the environment: A creates a water surface,
//! B electrifies it. The environment is the medium for asynchronous,
//! location-based handoffs (§20).
//!
//! Write access requires Hot+ momentum tier (§7 capability table).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::cadence::AgentId;

/// Shared mutable workspace for a formation.
///
/// From the spec:
/// ```text
/// pub struct SharedEnvironment {
///     pub workspace: HashMap<String, serde_json::Value>,
///     pub write_log: Vec<EnvironmentWrite>,
///     pub surfaces: Vec<Surface>,
/// }
/// ```
pub struct SharedEnvironment {
    pub workspace: HashMap<String, serde_json::Value>,
    pub write_log: Vec<EnvironmentWrite>,
    pub surfaces: Vec<Surface>,
}

/// A record of a write operation to the environment.
pub struct EnvironmentWrite {
    pub key: String,
    pub writer: AgentId,
    pub timestamp: Instant,
}

/// A surface in the environment — Divinity's elemental combo system.
///
/// From the spec:
/// ```text
/// pub struct Surface {
///     pub created_by: AgentId,
///     pub surface_type: SurfaceType,
///     pub data: serde_json::Value,
///     pub expires: Option<Instant>,
/// }
/// ```
pub struct Surface {
    pub created_by: AgentId,
    pub surface_type: SurfaceType,
    pub data: serde_json::Value,
    pub expires: Option<Instant>,
}

/// Surface lifecycle states — Divinity's elemental interactions.
///
/// From the spec:
/// ```text
/// pub enum SurfaceType {
///     Substrate,                           // Divinity: water on ground
///     Primed { trigger: ActionDescriptor }, // Divinity: oil ready to ignite
///     Active { remaining: Duration },      // Divinity: fire burning
/// }
/// ```
pub enum SurfaceType {
    /// Passive surface. Divinity: water on ground.
    Substrate,
    /// Ready to be triggered by another agent's action. Divinity: oil ready to ignite.
    Primed { trigger: String },
    /// Active effect with remaining duration. Divinity: fire burning.
    Active { remaining: Duration },
}

impl Default for SharedEnvironment {
    fn default() -> Self {
        Self {
            workspace: HashMap::new(),
            write_log: Vec::new(),
            surfaces: Vec::new(),
        }
    }
}

impl SharedEnvironment {
    /// Read a value from the workspace.
    pub fn read(&self, key: &str) -> Option<&serde_json::Value> {
        self.workspace.get(key)
    }

    /// Write a value to the workspace (logs the write).
    pub fn write(&mut self, key: String, value: serde_json::Value, writer: AgentId) {
        self.write_log.push(EnvironmentWrite {
            key: key.clone(),
            writer,
            timestamp: Instant::now(),
        });
        self.workspace.insert(key, value);
    }

    /// Add a surface to the environment.
    pub fn add_surface(&mut self, surface: Surface) {
        self.surfaces.push(surface);
    }

    /// Remove expired surfaces.
    pub fn expire_surfaces(&mut self) {
        let now = Instant::now();
        self.surfaces.retain(|s| {
            s.expires.map_or(true, |exp| exp > now)
        });
    }

    /// Find surfaces that can be triggered (Primed state).
    pub fn primed_surfaces(&self) -> Vec<&Surface> {
        self.surfaces
            .iter()
            .filter(|s| matches!(s.surface_type, SurfaceType::Primed { .. }))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_read_write() {
        let mut env = SharedEnvironment::default();
        let agent = AgentId::new();
        env.write("key".into(), serde_json::json!("value"), agent);
        assert_eq!(env.read("key"), Some(&serde_json::json!("value")));
        assert_eq!(env.write_log.len(), 1);
    }

    #[test]
    fn test_surface_lifecycle() {
        let mut env = SharedEnvironment::default();
        let agent = AgentId::new();

        // Agent A creates water (Substrate)
        env.add_surface(Surface {
            created_by: agent,
            surface_type: SurfaceType::Substrate,
            data: serde_json::json!({"element": "water"}),
            expires: None,
        });

        // Agent B primes with oil
        env.add_surface(Surface {
            created_by: AgentId::new(),
            surface_type: SurfaceType::Primed { trigger: "fire".into() },
            data: serde_json::json!({"element": "oil"}),
            expires: None,
        });

        assert_eq!(env.primed_surfaces().len(), 1);
    }

    #[test]
    fn test_expire_surfaces() {
        let mut env = SharedEnvironment::default();
        env.add_surface(Surface {
            created_by: AgentId::new(),
            surface_type: SurfaceType::Active { remaining: Duration::from_secs(0) },
            data: serde_json::json!({}),
            expires: Some(Instant::now() - Duration::from_secs(1)), // already expired
        });
        env.add_surface(Surface {
            created_by: AgentId::new(),
            surface_type: SurfaceType::Substrate,
            data: serde_json::json!({}),
            expires: None, // never expires
        });

        env.expire_surfaces();
        assert_eq!(env.surfaces.len(), 1);
    }
}
