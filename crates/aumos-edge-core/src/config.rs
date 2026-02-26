// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! `EdgeConfig` — local configuration file loader.
//!
//! The edge runtime is configuration-driven. All governance rules come from a
//! TOML file loaded at startup. The runtime does not modify its own rules at
//! runtime; updated rules arrive via the sync engine.

use crate::types::{ActionKind, EdgeError, TrustLevel};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Trust assignment for a specific agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTrustConfig {
    /// The agent identifier this entry applies to.
    pub agent_id: String,
    /// The static trust level assigned to this agent.
    pub level: TrustLevel,
}

/// Minimum trust level required to perform a given action kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionTrustRequirement {
    /// The action kind this requirement applies to.
    pub kind: String,
    /// The minimum trust level required.
    pub minimum_level: TrustLevel,
}

/// Budget allowance for a specific agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBudgetConfig {
    /// The agent identifier this entry applies to.
    pub agent_id: String,
    /// Total budget units available within the `window_seconds` period.
    pub total_units: f64,
    /// Rolling window duration in seconds over which the budget applies.
    pub window_seconds: u64,
}

/// Consent requirement for a resource pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRequirement {
    /// Resource prefix or exact name that requires explicit consent.
    pub resource_pattern: String,
    /// Action kinds that require consent for this resource pattern.
    pub required_for_kinds: Vec<String>,
}

/// Top-level edge configuration.
///
/// Loaded from a TOML file via [`EdgeConfig::from_file`].
///
/// # Example config (`edge-config.toml`)
///
/// ```toml
/// [governance]
/// default_trust_level = "standard"
/// deny_unknown_agents = true
///
/// [[agents]]
/// agent_id = "agent-001"
/// level = "elevated"
///
/// [[budgets]]
/// agent_id = "agent-001"
/// total_units = 100.0
/// window_seconds = 3600
///
/// [[action_requirements]]
/// kind = "data_delete"
/// minimum_level = "elevated"
///
/// [[consent_requirements]]
/// resource_pattern = "user-"
/// required_for_kinds = ["data_write", "data_delete"]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfig {
    /// Governance policy settings.
    pub governance: GovernancePolicyConfig,
    /// Per-agent trust assignments.
    #[serde(default)]
    pub agents: Vec<AgentTrustConfig>,
    /// Per-agent budget allocations.
    #[serde(default)]
    pub budgets: Vec<AgentBudgetConfig>,
    /// Minimum trust level required per action kind.
    #[serde(default)]
    pub action_requirements: Vec<ActionTrustRequirement>,
    /// Resources requiring explicit consent.
    #[serde(default)]
    pub consent_requirements: Vec<ConsentRequirement>,
}

/// Top-level governance policy knobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernancePolicyConfig {
    /// Trust level applied to agents not listed in `agents`.
    #[serde(default = "default_trust_level")]
    pub default_trust_level: TrustLevel,
    /// When `true`, agents not listed in `agents` are denied outright.
    #[serde(default)]
    pub deny_unknown_agents: bool,
    /// Default budget units for agents not listed in `budgets`.
    #[serde(default = "default_budget_units")]
    pub default_budget_units: f64,
    /// Default budget window in seconds.
    #[serde(default = "default_budget_window")]
    pub default_budget_window_seconds: u64,
}

fn default_trust_level() -> TrustLevel {
    TrustLevel::Standard
}

fn default_budget_units() -> f64 {
    50.0
}

fn default_budget_window() -> u64 {
    3600
}

impl EdgeConfig {
    /// Load and parse configuration from a TOML file at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeError::Io`] if the file cannot be read, or
    /// [`EdgeError::Config`] if the TOML is malformed.
    pub fn from_file(path: &Path) -> Result<Self, EdgeError> {
        let raw = std::fs::read_to_string(path)?;
        let config: EdgeConfig = toml::from_str(&raw)
            .map_err(|error| EdgeError::Config(error.to_string()))?;
        Ok(config)
    }

    /// Build the agent trust map for O(1) lookups at runtime.
    pub fn agent_trust_map(&self) -> HashMap<String, TrustLevel> {
        self.agents
            .iter()
            .map(|entry| (entry.agent_id.clone(), entry.level))
            .collect()
    }

    /// Build the action kind → minimum trust map for O(1) lookups.
    pub fn action_trust_map(&self) -> HashMap<String, TrustLevel> {
        self.action_requirements
            .iter()
            .map(|requirement| (requirement.kind.clone(), requirement.minimum_level))
            .collect()
    }

    /// Return the trust level for a given `agent_id`, applying defaults and
    /// the `deny_unknown_agents` policy.
    pub fn resolve_trust(&self, agent_id: &str) -> Option<TrustLevel> {
        let agent_map = self.agent_trust_map();
        if let Some(level) = agent_map.get(agent_id) {
            return Some(*level);
        }
        if self.governance.deny_unknown_agents {
            return None;
        }
        Some(self.governance.default_trust_level)
    }

    /// Return the minimum trust level required for an [`ActionKind`].
    pub fn required_trust_for_kind(&self, kind: &ActionKind) -> TrustLevel {
        let kind_str = serde_json::to_value(kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        let action_map = self.action_trust_map();
        action_map
            .get(&kind_str)
            .copied()
            .unwrap_or(TrustLevel::Standard)
    }

    /// Return the budget config for `agent_id`, or a default derived from
    /// the global policy settings.
    pub fn budget_for_agent(&self, agent_id: &str) -> AgentBudgetConfig {
        self.budgets
            .iter()
            .find(|budget| budget.agent_id == agent_id)
            .cloned()
            .unwrap_or_else(|| AgentBudgetConfig {
                agent_id: agent_id.to_string(),
                total_units: self.governance.default_budget_units,
                window_seconds: self.governance.default_budget_window_seconds,
            })
    }
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            governance: GovernancePolicyConfig {
                default_trust_level: TrustLevel::Standard,
                deny_unknown_agents: false,
                default_budget_units: 50.0,
                default_budget_window_seconds: 3600,
            },
            agents: Vec::new(),
            budgets: Vec::new(),
            action_requirements: Vec::new(),
            consent_requirements: Vec::new(),
        }
    }
}
