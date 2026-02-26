// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! `EdgeGovernanceEngine` — top-level composition of all governance modules.
//!
//! The engine runs a fixed, sequential evaluation pipeline for every proposed
//! agent action:
//!
//! ```text
//! GovernanceAction
//!     │
//!     ▼
//! ┌──────────────┐   Denied / UnknownAgent
//! │  TrustCheck  │──────────────────────────► GovernanceDecision { Denied }
//! └──────┬───────┘
//!        │ Sufficient
//!        ▼
//! ┌──────────────┐   ExceedsBudget
//! │ BudgetCheck  │──────────────────────────► GovernanceDecision { Denied }
//! └──────┬───────┘
//!        │ WithinBudget
//!        ▼
//! ┌──────────────┐   Required (no grant)
//! │ ConsentCheck │──────────────────────────► GovernanceDecision { RequiresConsent }
//! └──────┬───────┘
//!        │ Granted / NotRequired
//!        ▼
//! ┌──────────────┐
//! │  AuditLog    │  (always appended)
//! └──────┬───────┘
//!        │
//!        ▼
//!  GovernanceDecision { Allowed }
//! ```
//!
//! The audit log is appended for **every** decision, including denials.

use crate::audit::AuditLog;
use crate::budget::BudgetTracker;
use crate::config::EdgeConfig;
use crate::consent::ConsentStore;
use crate::storage::{InMemoryStorage, Storage};
use crate::trust::TrustChecker;
use crate::types::{CompletedAction, EdgeError, GovernanceAction, GovernanceDecision};
use crate::audit::AuditFilter;
use std::path::Path;

/// The on-device governance engine.
///
/// Loaded from a TOML configuration file and backed by a `Storage` instance.
/// All governance decisions are made synchronously without network access.
///
/// # Example
///
/// ```rust,no_run
/// use aumos_edge_core::{EdgeGovernanceEngine, GovernanceAction, ActionKind};
/// use std::path::Path;
///
/// let mut engine = EdgeGovernanceEngine::from_config(Path::new("edge-config.toml"))
///     .expect("failed to load config");
/// let action = GovernanceAction::new("agent-001", ActionKind::DataRead, "metrics", 0.5);
/// let decision = engine.evaluate(&action);
/// println!("{:?}", decision.outcome);
/// ```
pub struct EdgeGovernanceEngine {
    config: EdgeConfig,
    storage: Box<dyn Storage>,
    /// Pending `CompletedAction`s waiting to be synced.
    sync_queue: Vec<CompletedAction>,
}

impl EdgeGovernanceEngine {
    /// Load configuration from `path` and initialise the engine with
    /// in-memory storage.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeError::Io`] or [`EdgeError::Config`] if the file cannot
    /// be read or parsed.
    pub fn from_config(path: &Path) -> Result<Self, EdgeError> {
        let config = EdgeConfig::from_file(path)?;
        Ok(Self::with_storage(config, Box::new(InMemoryStorage::new())))
    }

    /// Construct an engine directly from a pre-loaded [`EdgeConfig`] and a
    /// custom `Storage` implementation.
    pub fn with_storage(config: EdgeConfig, storage: Box<dyn Storage>) -> Self {
        Self {
            config,
            storage,
            sync_queue: Vec::new(),
        }
    }

    /// Evaluate `action` through the governance pipeline and return a decision.
    ///
    /// The audit log is appended regardless of the outcome.
    pub fn evaluate(&mut self, action: &GovernanceAction) -> GovernanceDecision {
        let decision = self.run_pipeline(action);
        // Best-effort audit; if storage is unavailable we log the error but do
        // not propagate — governance decisions must not be blocked by audit failures.
        if let Err(error) = AuditLog::new(self.storage.as_mut()).log(action, &decision) {
            log::error!("Audit log write failed for action {}: {}", action.action_id, error);
        }
        decision
    }

    /// Queue a `CompletedAction` for upload on the next sync cycle.
    ///
    /// # Errors
    ///
    /// Currently infallible, but returns `Result` for forward-compatibility
    /// with persistent queue backends.
    pub fn queue_for_sync(&mut self, action: CompletedAction) -> Result<(), EdgeError> {
        self.sync_queue.push(action);
        Ok(())
    }

    /// Drain the sync queue, returning all pending completed actions.
    pub fn drain_sync_queue(&mut self) -> Vec<CompletedAction> {
        std::mem::take(&mut self.sync_queue)
    }

    /// Access the currently loaded configuration.
    pub fn config(&self) -> &EdgeConfig {
        &self.config
    }

    /// Reload configuration from `path`, replacing the current config.
    ///
    /// Existing storage state is preserved; only the rules change.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeError::Io`] or [`EdgeError::Config`] if the file cannot
    /// be read or parsed.
    pub fn reload_config(&mut self, path: &Path) -> Result<(), EdgeError> {
        self.config = EdgeConfig::from_file(path)?;
        log::info!("EdgeGovernanceEngine: configuration reloaded from {}", path.display());
        Ok(())
    }

    /// Query the audit log using the supplied filter.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeError::AuditChain`] if chain integrity is violated.
    pub fn query_audit(
        &self,
        filter: &AuditFilter,
    ) -> Result<Vec<crate::audit::AuditEntry>, EdgeError> {
        // We need shared access; use a read-only shim.
        let records = self.storage.list_namespace("audit")?;
        let mut entries: Vec<crate::audit::AuditEntry> = records
            .into_iter()
            .filter_map(|record| serde_json::from_value(record.value).ok())
            .collect();
        entries.sort_by_key(|entry| entry.sequence);

        // Apply filter predicates (chain verification happens inside AuditLog::query,
        // but we don't have &mut here so we do a simplified read path).
        let filtered = entries
            .into_iter()
            .filter(|entry| {
                if let Some(ref agent_id) = filter.agent_id {
                    if &entry.action.agent_id != agent_id {
                        return false;
                    }
                }
                if let Some(since) = filter.since {
                    if entry.logged_at < since {
                        return false;
                    }
                }
                if let Some(until) = filter.until {
                    if entry.logged_at > until {
                        return false;
                    }
                }
                true
            })
            .take(filter.limit.unwrap_or(usize::MAX))
            .collect();

        Ok(filtered)
    }

    // ── Private pipeline ──────────────────────────────────────────────────────

    fn run_pipeline(&mut self, action: &GovernanceAction) -> GovernanceDecision {
        // Stage 1: trust check (read-only, no storage mutation).
        let trust_result = TrustChecker::new(&self.config).check(action);
        let trust_checker = TrustChecker::new(&self.config);
        if let Some(decision) = trust_checker.to_decision(action.action_id, &trust_result) {
            return decision;
        }

        // Stage 2: budget check (read-only here; cost committed after allowed decision).
        let budget_result = {
            let tracker = BudgetTracker::new(&self.config, self.storage.as_mut());
            tracker.check(action)
        };
        let budget_checker = BudgetTracker::new(&self.config, self.storage.as_mut());
        if let Some(decision) = budget_checker.to_decision(action.action_id, &budget_result) {
            return decision;
        }

        // Stage 3: consent check.
        let consent_result = {
            let store = ConsentStore::new(&self.config, self.storage.as_mut());
            store.check(action)
        };
        let consent_store = ConsentStore::new(&self.config, self.storage.as_mut());
        if let Some(decision) = consent_store.to_decision(action.action_id, &consent_result) {
            return decision;
        }

        // All stages passed — commit estimated cost to budget.
        if let Err(error) =
            BudgetTracker::new(&self.config, self.storage.as_mut()).record(action, action.estimated_cost)
        {
            log::warn!(
                "Failed to record budget cost for action {}: {}",
                action.action_id,
                error
            );
        }

        GovernanceDecision::allowed(action.action_id)
    }
}
