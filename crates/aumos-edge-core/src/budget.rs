// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! `BudgetTracker` — local budget enforcement.
//!
//! Tracks cumulative cost consumption per agent over a rolling time window.
//! All state is persisted through the `Storage` abstraction.
//!
//! Budget decisions are purely arithmetic: if the action's estimated cost
//! would push the agent over its configured limit, the action is denied.
//! There is no adaptive reallocation or predictive logic.

use crate::config::EdgeConfig;
use crate::storage::{Storage, StorageRecord};
use crate::types::{DecisionStage, EdgeError, GovernanceAction, GovernanceDecision};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const NAMESPACE: &str = "budget";

/// Serialized budget state for one agent, stored per-agent in `Storage`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentBudgetState {
    agent_id: String,
    /// Units consumed within the current window.
    consumed: f64,
    /// Total units allowed within the window.
    total_units: f64,
    /// Start of the current rolling window.
    window_start: DateTime<Utc>,
    /// Duration of the rolling window.
    window_seconds: u64,
}

impl AgentBudgetState {
    /// Reset the window if the current time is outside the window.
    fn reset_if_expired(&mut self, now: DateTime<Utc>) {
        let window_duration = Duration::seconds(self.window_seconds as i64);
        if now >= self.window_start + window_duration {
            self.consumed = 0.0;
            self.window_start = now;
        }
    }

    /// Returns `true` if `additional_cost` would not exceed `total_units`.
    fn has_capacity(&self, additional_cost: f64) -> bool {
        self.consumed + additional_cost <= self.total_units
    }
}

/// The outcome of a budget check.
#[derive(Debug, Clone)]
pub enum BudgetResult {
    /// The action fits within the agent's remaining budget.
    WithinBudget {
        consumed: f64,
        total: f64,
        estimated_cost: f64,
    },
    /// The action would exceed the agent's budget.
    ExceedsBudget {
        consumed: f64,
        total: f64,
        estimated_cost: f64,
    },
}

/// Enforces per-agent budget limits using a rolling time window.
pub struct BudgetTracker<'a> {
    config: &'a EdgeConfig,
    storage: &'a mut dyn Storage,
}

impl<'a> BudgetTracker<'a> {
    /// Create a new `BudgetTracker` using `config` for limits and `storage`
    /// for persistence.
    pub fn new(config: &'a EdgeConfig, storage: &'a mut dyn Storage) -> Self {
        Self { config, storage }
    }

    /// Check whether `action` fits within the agent's remaining budget.
    ///
    /// This does **not** record the cost; call [`record`](Self::record) after
    /// a decision of `Allowed` to commit the cost.
    pub fn check(&self, action: &GovernanceAction) -> BudgetResult {
        let now = Utc::now();
        let mut state = self.load_state(&action.agent_id, now);

        let consumed = state.consumed;
        let total = state.total_units;
        let cost = action.estimated_cost;

        if state.has_capacity(cost) {
            BudgetResult::WithinBudget {
                consumed,
                total,
                estimated_cost: cost,
            }
        } else {
            BudgetResult::ExceedsBudget {
                consumed,
                total,
                estimated_cost: cost,
            }
        }
    }

    /// Record `actual_cost` against `action.agent_id`'s budget.
    ///
    /// Must be called only after an `Allowed` governance decision.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeError::Storage`] if the state cannot be persisted.
    pub fn record(&mut self, action: &GovernanceAction, actual_cost: f64) -> Result<(), EdgeError> {
        let now = Utc::now();
        let mut state = self.load_state(&action.agent_id, now);
        state.consumed += actual_cost;
        self.save_state(&state)
    }

    /// Convert a [`BudgetResult`] to a [`GovernanceDecision`].
    ///
    /// Returns `None` if the check passed (pipeline continues), or
    /// `Some(denied)` if the budget would be exceeded.
    pub fn to_decision(&self, action_id: Uuid, result: &BudgetResult) -> Option<GovernanceDecision> {
        match result {
            BudgetResult::WithinBudget { .. } => None,
            BudgetResult::ExceedsBudget {
                consumed,
                total,
                estimated_cost,
            } => Some(GovernanceDecision::denied(
                action_id,
                DecisionStage::BudgetCheck,
                format!(
                    "Action cost {:.4} would exceed budget: {:.4} consumed of {:.4} total",
                    estimated_cost, consumed, total
                ),
            )),
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn load_state(&self, agent_id: &str, now: DateTime<Utc>) -> AgentBudgetState {
        let raw = self.storage.get(NAMESPACE, agent_id).ok().flatten();
        match raw {
            Some(record) => {
                let mut state: AgentBudgetState =
                    serde_json::from_value(record.value).unwrap_or_else(|_| {
                        self.default_state(agent_id, now)
                    });
                state.reset_if_expired(now);
                state
            }
            None => self.default_state(agent_id, now),
        }
    }

    fn default_state(&self, agent_id: &str, now: DateTime<Utc>) -> AgentBudgetState {
        let budget_config = self.config.budget_for_agent(agent_id);
        AgentBudgetState {
            agent_id: agent_id.to_string(),
            consumed: 0.0,
            total_units: budget_config.total_units,
            window_start: now,
            window_seconds: budget_config.window_seconds,
        }
    }

    fn save_state(&mut self, state: &AgentBudgetState) -> Result<(), EdgeError> {
        let sequence = self.storage.next_sequence(NAMESPACE);
        let record = StorageRecord {
            namespace: NAMESPACE.to_string(),
            key: state.agent_id.clone(),
            value: serde_json::to_value(state)?,
            sequence,
            updated_at: Utc::now().to_rfc3339(),
        };
        self.storage.put(record)
    }
}
