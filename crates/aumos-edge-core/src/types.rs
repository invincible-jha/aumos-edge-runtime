// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! Core domain types for the edge governance runtime.
//!
//! These types are shared across all modules within `aumos-edge-core`.
//! They are deliberately minimal — the edge runtime stores only what is
//! needed to enforce governance rules and produce an auditable record.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── Trust ────────────────────────────────────────────────────────────────────

/// Discrete trust level assigned to an agent by local configuration.
///
/// Levels are ordered: `Restricted` < `Standard` < `Elevated` < `System`.
/// Comparison operators reflect this ordering.
///
/// # Example
///
/// ```rust
/// use aumos_edge_core::types::TrustLevel;
/// assert!(TrustLevel::Elevated > TrustLevel::Standard);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    /// Agent is restricted to a minimal safe action set.
    Restricted = 0,
    /// Default level for authenticated agents.
    Standard = 1,
    /// Agent has passed additional verification steps.
    Elevated = 2,
    /// Internal system agent with full permissions.
    System = 3,
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustLevel::Restricted => write!(formatter, "restricted"),
            TrustLevel::Standard => write!(formatter, "standard"),
            TrustLevel::Elevated => write!(formatter, "elevated"),
            TrustLevel::System => write!(formatter, "system"),
        }
    }
}

// ── Actions ──────────────────────────────────────────────────────────────────

/// The category of operation an agent is attempting.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Read data from a resource.
    DataRead,
    /// Write or modify data in a resource.
    DataWrite,
    /// Delete data from a resource.
    DataDelete,
    /// Call an external API or service.
    ExternalCall,
    /// Execute a tool or sub-agent.
    ToolExecution,
    /// Modify system configuration.
    ConfigChange,
    /// Any action not covered by the above categories.
    Custom(String),
}

/// A proposed agent action submitted to the governance engine for evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceAction {
    /// Unique identifier for this proposed action.
    pub action_id: Uuid,
    /// Identifier of the agent requesting the action.
    pub agent_id: String,
    /// The category of action being requested.
    pub kind: ActionKind,
    /// The specific resource or target the action applies to.
    pub resource: String,
    /// The estimated cost of the action in abstract budget units.
    pub estimated_cost: f64,
    /// Arbitrary key-value metadata attached by the calling agent.
    pub metadata: HashMap<String, String>,
    /// When the action was proposed.
    pub timestamp: DateTime<Utc>,
}

impl GovernanceAction {
    /// Create a new `GovernanceAction` with a generated ID and current timestamp.
    pub fn new(
        agent_id: impl Into<String>,
        kind: ActionKind,
        resource: impl Into<String>,
        estimated_cost: f64,
    ) -> Self {
        Self {
            action_id: Uuid::new_v4(),
            agent_id: agent_id.into(),
            kind,
            resource: resource.into(),
            estimated_cost,
            metadata: HashMap::new(),
            timestamp: Utc::now(),
        }
    }
}

/// An action that has already been executed, ready for sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedAction {
    /// The original proposed action.
    pub action: GovernanceAction,
    /// The governance outcome that was applied.
    pub outcome: GovernanceOutcome,
    /// Actual cost incurred (may differ from estimated).
    pub actual_cost: f64,
    /// When execution completed.
    pub completed_at: DateTime<Utc>,
}

// ── Decisions ─────────────────────────────────────────────────────────────────

/// The outcome of a governance evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceOutcome {
    /// The action is permitted to proceed.
    Allowed,
    /// The action is denied; the agent must not execute it.
    Denied,
    /// The action requires explicit consent before proceeding.
    RequiresConsent,
}

/// Which check stage produced the final outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStage {
    TrustCheck,
    BudgetCheck,
    ConsentCheck,
}

/// The full governance decision returned to the caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceDecision {
    /// The action that was evaluated.
    pub action_id: Uuid,
    /// Final outcome.
    pub outcome: GovernanceOutcome,
    /// The pipeline stage that determined the outcome.
    pub decided_by: DecisionStage,
    /// Human-readable explanation of the decision.
    pub reason: String,
    /// When the decision was made.
    pub decided_at: DateTime<Utc>,
}

impl GovernanceDecision {
    /// Construct an `Allowed` decision from the consent stage (last stage to pass).
    pub fn allowed(action_id: Uuid) -> Self {
        Self {
            action_id,
            outcome: GovernanceOutcome::Allowed,
            decided_by: DecisionStage::ConsentCheck,
            reason: "All governance checks passed".to_string(),
            decided_at: Utc::now(),
        }
    }

    /// Construct a `Denied` decision from any pipeline stage.
    pub fn denied(action_id: Uuid, stage: DecisionStage, reason: impl Into<String>) -> Self {
        Self {
            action_id,
            outcome: GovernanceOutcome::Denied,
            decided_by: stage,
            reason: reason.into(),
            decided_at: Utc::now(),
        }
    }

    /// Construct a `RequiresConsent` decision.
    pub fn requires_consent(action_id: Uuid, reason: impl Into<String>) -> Self {
        Self {
            action_id,
            outcome: GovernanceOutcome::RequiresConsent,
            decided_by: DecisionStage::ConsentCheck,
            reason: reason.into(),
            decided_at: Utc::now(),
        }
    }
}

// ── Errors ────────────────────────────────────────────────────────────────────

/// Errors produced by the edge governance engine.
#[derive(Debug, thiserror::Error)]
pub enum EdgeError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Audit chain integrity violation: {0}")]
    AuditChain(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}
