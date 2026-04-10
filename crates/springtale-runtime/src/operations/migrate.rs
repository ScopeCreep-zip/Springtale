//! OpenClaw SKILL.md migration — parse skills into Springtale connector manifests.
//!
//! OpenClaw skills (YAML frontmatter + markdown body) are fundamentally
//! different from Springtale connectors (sandboxed WASM code). This module
//! extracts structured metadata from SKILL.md files and produces a
//! ConnectorManifest + config schema + warnings.
//!
//! The migration does NOT produce a working connector — it produces the
//! manifest and config scaffolding. The user must implement the actual
//! connector logic (as a WASM module or native Rust crate).
//!
//! References:
//! - [OpenClaw SKILL.md format](https://docs.openclaw.ai/tools/skills)
//! - [OpenClaw skills repo](https://github.com/openclaw/skills)

use serde::Deserialize;

use springtale_connector::manifest::types::{Capability, ConnectorManifest};

use crate::error::OperationError;

/// Result of migrating an OpenClaw SKILL.md.
pub struct MigratedSkill {
    /// Generated ConnectorManifest from SKILL.md metadata.
    pub manifest: ConnectorManifest,
    /// JSON Schema for config form (derived from requires.env).
    pub config_schema: serde_json::Value,
    /// Warnings about migration limitations.
    pub warnings: Vec<String>,
}

/// YAML frontmatter structure from SKILL.md.
#[derive(Debug, Deserialize, Default)]
struct SkillFrontmatter {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    metadata: Option<SkillMetadata>,
}

#[derive(Debug, Deserialize, Default)]
struct SkillMetadata {
    #[serde(default, alias = "clawdbot", alias = "clawdis")]
    openclaw: Option<OpenClawMetadata>,
}

#[derive(Debug, Deserialize, Default)]
struct OpenClawMetadata {
    #[serde(default)]
    requires: Option<SkillRequires>,
    #[serde(default, rename = "primaryEnv")]
    primary_env: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct SkillRequires {
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    bins: Vec<String>,
    #[serde(default, rename = "anyBins")]
    any_bins: Vec<String>,
    #[serde(default)]
    config: Vec<String>,
}

/// Parse an OpenClaw SKILL.md into a Springtale ConnectorManifest.
///
/// Extracts YAML frontmatter, maps requirements to capabilities,
/// infers network hosts from markdown body, and generates a config schema.
pub fn parse_openclaw_skill(skill_md: &str) -> Result<MigratedSkill, OperationError> {
    let mut warnings = Vec::new();

    // Split YAML frontmatter from markdown body
    let (frontmatter_str, markdown_body) = extract_frontmatter(skill_md)?;

    // Parse YAML frontmatter
    let frontmatter: SkillFrontmatter = serde_yaml::from_str(&frontmatter_str)
        .map_err(|e| OperationError::Validation(format!("invalid SKILL.md YAML: {e}")))?;

    if frontmatter.name.is_empty() {
        return Err(OperationError::Validation(
            "SKILL.md must have a 'name' field".into(),
        ));
    }

    // Build connector name
    let connector_name = if frontmatter.name.starts_with("connector-") {
        frontmatter.name.clone()
    } else {
        format!("connector-{}", frontmatter.name)
    };

    // Extract capabilities from requirements
    let mut capabilities = Vec::new();
    let mut config_properties = serde_json::Map::new();
    let mut required_fields = Vec::new();

    if let Some(ref metadata) = frontmatter.metadata {
        if let Some(ref oc) = metadata.openclaw {
            if let Some(ref requires) = oc.requires {
                // Environment variables → KeychainRead capabilities + config schema
                for env_var in &requires.env {
                    capabilities.push(Capability::KeychainRead {
                        key: env_var.clone(),
                    });

                    // Add to config schema as a secret field
                    config_properties.insert(
                        env_var.clone(),
                        serde_json::json!({
                            "type": "string",
                            "description": format!("Environment variable: {env_var}"),
                            "x-secret": true,
                        }),
                    );
                    required_fields.push(serde_json::Value::String(env_var.clone()));
                }

                // Binary requirements → ShellExec capability + warning
                if !requires.bins.is_empty() || !requires.any_bins.is_empty() {
                    capabilities.push(Capability::ShellExec);
                    let bins: Vec<&str> = requires
                        .bins
                        .iter()
                        .chain(requires.any_bins.iter())
                        .map(|s| s.as_str())
                        .collect();
                    warnings.push(format!(
                        "Skill requires shell binaries: {}. ShellExec capability requires blocking user approval in Springtale.",
                        bins.join(", ")
                    ));
                }

                // Config file paths → informational warning
                if !requires.config.is_empty() {
                    warnings.push(format!(
                        "Skill reads config files: {}. These must be mapped to Springtale config fields.",
                        requires.config.join(", ")
                    ));
                }
            }

            // Primary env var → mark as primary in config schema
            if let Some(ref primary) = oc.primary_env {
                if let Some(prop) = config_properties.get_mut(primary) {
                    if let Some(obj) = prop.as_object_mut() {
                        obj.insert(
                            "x-primary".into(),
                            serde_json::Value::Bool(true),
                        );
                    }
                }
            }
        }
    }

    // Scan markdown body for URLs → NetworkOutbound capabilities
    let url_regex = regex::Regex::new(r"https?://([a-zA-Z0-9._-]+)")
        .map_err(|e| OperationError::Validation(format!("regex error: {e}")))?;
    let mut seen_hosts = std::collections::HashSet::new();
    for cap in url_regex.captures_iter(&markdown_body) {
        if let Some(host) = cap.get(1) {
            let host_str = host.as_str().to_owned();
            if !seen_hosts.contains(&host_str)
                && !host_str.contains("example.com")
                && !host_str.contains("localhost")
            {
                capabilities.push(Capability::NetworkOutbound {
                    host: host_str.clone(),
                });
                seen_hosts.insert(host_str);
            }
        }
    }

    // Build config schema
    let config_schema = serde_json::json!({
        "type": "object",
        "properties": config_properties,
        "required": required_fields,
    });

    // Build manifest
    let manifest = ConnectorManifest {
        name: connector_name,
        version: frontmatter.version.unwrap_or_else(|| "0.1.0".into()),
        author: "migrated-from-openclaw".into(),
        description: frontmatter.description,
        capabilities,
        triggers: vec![], // Skills don't declare triggers
        actions: vec![],  // Actions must be implemented by the user
        data_disclosure: vec![],
        wasm_hash: None,
        signature: None,
    };

    // Standard warnings
    warnings.push(
        "Migration produces a manifest only — no WASM connector binary. \
         Implement the connector logic using the Springtale Connector SDK."
            .into(),
    );
    warnings.push(
        "OpenClaw skills are AI instructions that run arbitrary commands. \
         Springtale connectors are sandboxed code. The security model is different."
            .into(),
    );

    Ok(MigratedSkill {
        manifest,
        config_schema,
        warnings,
    })
}

/// Extract YAML frontmatter from a SKILL.md file.
///
/// Frontmatter is delimited by `---` at the start and end.
fn extract_frontmatter(content: &str) -> Result<(String, String), OperationError> {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return Err(OperationError::Validation(
            "SKILL.md must start with YAML frontmatter (---)".into(),
        ));
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    let close_pos = after_first.find("\n---").ok_or_else(|| {
        OperationError::Validation("SKILL.md missing closing --- for frontmatter".into())
    })?;

    let frontmatter = after_first[..close_pos].trim().to_owned();
    let body = after_first[close_pos + 4..].trim().to_owned();

    Ok((frontmatter, body))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_skill() {
        let skill = r#"---
name: test-skill
description: A test skill
---
# Test Skill
This does nothing."#;

        let result = parse_openclaw_skill(skill).unwrap();
        assert_eq!(result.manifest.name, "connector-test-skill");
        assert_eq!(result.manifest.description, "A test skill");
        assert_eq!(result.manifest.version, "0.1.0");
        assert!(result.manifest.capabilities.is_empty());
    }

    #[test]
    fn test_parse_skill_with_env_requirements() {
        let skill = r#"---
name: todoist
description: Manage Todoist tasks
metadata:
  openclaw:
    requires:
      env:
        - TODOIST_API_KEY
    primaryEnv: TODOIST_API_KEY
---
# Todoist Skill
Uses https://api.todoist.com for API calls."#;

        let result = parse_openclaw_skill(skill).unwrap();
        assert_eq!(result.manifest.name, "connector-todoist");

        // Should have KeychainRead + NetworkOutbound capabilities
        assert!(result.manifest.capabilities.iter().any(|c| matches!(
            c,
            Capability::KeychainRead { key } if key == "TODOIST_API_KEY"
        )));
        assert!(result.manifest.capabilities.iter().any(|c| matches!(
            c,
            Capability::NetworkOutbound { host } if host == "api.todoist.com"
        )));

        // Config schema should have the env var as a secret field
        let props = result.config_schema["properties"].as_object().unwrap();
        assert!(props.contains_key("TODOIST_API_KEY"));
        assert_eq!(props["TODOIST_API_KEY"]["x-secret"], true);
    }

    #[test]
    fn test_parse_skill_with_bin_requirements() {
        let skill = r#"---
name: ffmpeg-tool
description: Video processing
metadata:
  openclaw:
    requires:
      bins:
        - ffmpeg
        - ffprobe
---
# FFmpeg Skill"#;

        let result = parse_openclaw_skill(skill).unwrap();

        // Should have ShellExec capability
        assert!(result
            .manifest
            .capabilities
            .iter()
            .any(|c| matches!(c, Capability::ShellExec)));

        // Should have a warning about ShellExec
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("ShellExec")));
    }

    #[test]
    fn test_missing_frontmatter_fails() {
        let skill = "# No frontmatter here";
        let result = parse_openclaw_skill(skill);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_name_fails() {
        let skill = "---\ndescription: no name\n---\nbody";
        let result = parse_openclaw_skill(skill);
        assert!(result.is_err());
    }
}
