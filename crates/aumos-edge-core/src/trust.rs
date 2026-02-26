// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! `TrustChecker` — static trust level comparison.
//!
//! The trust checker answers a single question:
//! "Does this agent's configured trust level meet the minimum required for
//! the requested action?"
//!
//! Trust levels are static; they come from `EdgeConfig` and are not modified
//! at runtime based on observed behavior.

use crate::config::EdgeConfig;
use crate::types::{ActionKind, DecisionStage, GovernanceAction, GovernanceDecision, TrustLevel};
use uuid::Uuid;

/// The outcome of a trust level check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustResult {
    /// The agent's trust level satisfies the requirement for this action.
    Sufficient {
        agent_level: TrustLevel,
        required_level: TrustLevel,
    },
    /// The agent is unknown and `deny_unknown_agents` is set to `true`.
    UnknownAgent { agent_id: String },
    /// The agent's trust level is below the required threshold.
    Insufficient {
        agent_level: TrustLevel,
        required_level: TrustLevel,
    },
}

/// Performs static trust level checks against a loaded `EdgeConfig`.
///
/// # Example
///
/// ```rust
/// use aumos_edge_core::config::EdgeConfig;
/// use aumos_edge_core::trust::TrustChecker;
/// use aumos_edge_core::types::{GovernanceAction, ActionKind, TrustResult};
///
/// let config = EdgeConfig::default();
/// let checker = TrustChecker::new(&config);
/// let action = GovernanceAction::new("agent-001", ActionKind::DataRead, "metrics", 0.0);
/// let result = checker.check(&action);
/// assert!(matches!(result, TrustResult::Sufficient { .. }));
/// ```
pub struct TrustChecker<'config> {
    config: &'config EdgeConfig,
}

impl<'config> TrustChecker<'config> {
    /// Create a new `TrustChecker` backed by `config`.
    pub fn new(config: &'config EdgeConfig) -> Self {
        Self { config }
    }

    /// Check whether `action` passes the trust requirement.
    pub fn check(&self, action: &GovernanceAction) -> TrustResult {
        let agent_level = match self.config.resolve_trust(&action.agent_id) {
            Some(level) => level,
            None => {
                return TrustResult::UnknownAgent {
                    agent_id: action.agent_id.clone(),
                }
            }
        };

        let required_level = self.config.required_trust_for_kind(&action.kind);

        if agent_level >= required_level {
            TrustResult::Sufficient {
                agent_level,
                required_level,
            }
        } else {
            TrustResult::Insufficient {
                agent_level,
                required_level,
            }
        }
    }

    /// Translate a [`TrustResult`] into a [`GovernanceDecision`], or `None`
    /// if the check passed (meaning the next pipeline stage should run).
    ///
    /// Returns `Some(decision)` with a `Denied` outcome only when the check
    /// has failed.
    pub fn to_decision(
        &self,
        action_id: Uuid,
        result: &TrustResult,
    ) -> Option<GovernanceDecision> {
        match result {
            TrustResult::Sufficient { .. } => None,
            TrustResult::UnknownAgent { agent_id } => Some(GovernanceDecision::denied(
                action_id,
                DecisionStage::TrustCheck,
                format!("Agent '{}' is not recognised and unknown agents are denied", agent_id),
            )),
            TrustResult::Insufficient {
                agent_level,
                required_level,
            } => Some(GovernanceDecision::denied(
                action_id,
                DecisionStage::TrustCheck,
                format!(
                    "Agent trust level '{}' is below the required '{}' for this action",
                    agent_level, required_level
                ),
            )),
        }
    }
}
