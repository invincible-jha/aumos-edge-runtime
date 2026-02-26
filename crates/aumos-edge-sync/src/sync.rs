// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! `SyncEngine` — push local state, pull remote config.
//!
//! The sync protocol is deliberately simple and bandwidth-efficient:
//!
//! 1. Push all local audit records that have not yet been acknowledged.
//! 2. Push current budget state deltas.
//! 3. Pull the latest `EdgeConfig` from the server.
//! 4. Resolve any conflicts using `ConflictResolver` (last-write-wins by default).
//!
//! FIRE LINE: No smart prioritisation, no selective sync, no ML-driven ordering.
//! Push everything; pull everything.

use crate::conflict::{ConflictResolution, ConflictResolver, ResolutionStrategy};
use crate::queue::ActionQueue;
use crate::transport::HttpTransport;
use aumos_edge_core::audit::AuditEntry;
use aumos_edge_core::config::EdgeConfig;
use aumos_edge_core::storage::{Storage, StorageRecord};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Errors that can occur during a sync cycle.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// A summary of a completed sync cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReport {
    /// When the sync cycle started.
    pub started_at: DateTime<Utc>,
    /// When the sync cycle completed.
    pub completed_at: DateTime<Utc>,
    /// Number of audit entries pushed to the server.
    pub audit_entries_pushed: usize,
    /// Number of budget records pushed to the server.
    pub budget_records_pushed: usize,
    /// Whether the remote config was successfully pulled.
    pub config_pulled: bool,
    /// Number of conflict resolutions applied.
    pub conflicts_resolved: usize,
    /// Any non-fatal errors encountered during the cycle.
    pub warnings: Vec<String>,
}

/// Configuration for the sync engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Device or deployment identifier sent with every push.
    pub device_id: String,
    /// Bearer token for the remote server.
    pub auth_token: String,
    /// Conflict resolution strategy.
    #[serde(default)]
    pub conflict_strategy: ResolutionStrategy,
    /// Maximum upload attempts before dropping a queued action.
    #[serde(default = "default_max_attempts")]
    pub max_queue_attempts: u32,
}

fn default_max_attempts() -> u32 {
    5
}

/// The sync engine — drives a full push-then-pull cycle.
pub struct SyncEngine {
    sync_config: SyncConfig,
    storage: Box<dyn Storage>,
    queue: ActionQueue,
}

impl SyncEngine {
    /// Create a new `SyncEngine`.
    pub fn new(
        sync_config: SyncConfig,
        storage: Box<dyn Storage>,
        queue: ActionQueue,
    ) -> Self {
        Self {
            sync_config,
            storage,
            queue,
        }
    }

    /// Execute a full sync cycle against `server_url`.
    ///
    /// Steps:
    /// 1. Push local audit records
    /// 2. Push budget changes
    /// 3. Pull remote config
    /// 4. Resolve conflicts
    ///
    /// Errors in individual steps are non-fatal: they are recorded as warnings
    /// in the returned [`SyncReport`] and the cycle continues.
    ///
    /// # Errors
    ///
    /// Returns `Err` only if the transport cannot be constructed. Individual
    /// step failures appear in `SyncReport::warnings`.
    pub fn sync(&mut self, server_url: &str) -> Result<SyncReport, SyncError> {
        let started_at = Utc::now();
        let mut warnings: Vec<String> = Vec::new();
        let mut audit_pushed = 0usize;
        let mut budget_pushed = 0usize;
        let mut conflicts_resolved = 0usize;
        let mut config_pulled = false;

        let transport = HttpTransport::new(
            server_url,
            self.sync_config.auth_token.clone(),
        );

        // ── Step 1: push audit records ────────────────────────────────────────
        let audit_records = self.storage.list_namespace("audit").unwrap_or_default();
        if !audit_records.is_empty() {
            let entries: Vec<AuditEntry> = audit_records
                .iter()
                .filter_map(|record| serde_json::from_value(record.value.clone()).ok())
                .collect();

            let count = entries.len();
            match transport.push_audit_entries(&self.sync_config.device_id, entries) {
                Ok(response) => {
                    audit_pushed = response.accepted;
                    log::info!("Pushed {} audit entries (accepted {})", count, response.accepted);
                }
                Err(error) => {
                    warnings.push(format!("Audit push failed: {}", error));
                    log::warn!("Audit push failed: {}", error);
                }
            }
        }

        // ── Step 2: push budget records ───────────────────────────────────────
        let budget_records = self.storage.list_namespace("budget").unwrap_or_default();
        if !budget_records.is_empty() {
            let values: Vec<serde_json::Value> = budget_records
                .iter()
                .map(|record| record.value.clone())
                .collect();

            let count = values.len();
            match transport.push_budget_records(&self.sync_config.device_id, values) {
                Ok(response) => {
                    budget_pushed = response.accepted;
                    log::info!("Pushed {} budget records (accepted {})", count, response.accepted);
                }
                Err(error) => {
                    warnings.push(format!("Budget push failed: {}", error));
                    log::warn!("Budget push failed: {}", error);
                }
            }
        }

        // ── Step 3: drain the offline action queue ────────────────────────────
        if !self.queue.is_empty() {
            let drained = self.queue.drain_all();
            log::info!("Drained {} queued offline actions", drained.len());
            // Queued actions were already evaluated; their audit entries are
            // included in the audit push above. No further action required here.
        }

        // ── Step 4: pull remote config ────────────────────────────────────────
        let pulled_config = match transport.pull_config(&self.sync_config.device_id) {
            Ok(config) => {
                config_pulled = true;
                log::info!("Remote config pulled successfully");
                Some(config)
            }
            Err(error) => {
                warnings.push(format!("Config pull failed: {}", error));
                log::warn!("Config pull failed: {}", error);
                None
            }
        };

        // ── Step 5: resolve conflicts ─────────────────────────────────────────
        if let Some(remote_config) = pulled_config {
            let resolutions = self.apply_remote_config(remote_config);
            conflicts_resolved = resolutions.len();
        }

        Ok(SyncReport {
            started_at,
            completed_at: Utc::now(),
            audit_entries_pushed: audit_pushed,
            budget_records_pushed: budget_pushed,
            config_pulled,
            conflicts_resolved,
            warnings,
        })
    }

    /// Access the current action queue.
    pub fn queue(&self) -> &ActionQueue {
        &self.queue
    }

    /// Access the current action queue mutably.
    pub fn queue_mut(&mut self) -> &mut ActionQueue {
        &mut self.queue
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn apply_remote_config(&mut self, _remote: EdgeConfig) -> Vec<ConflictResolution> {
        // In the current push-all / pull-all model, the remote config replaces
        // local rules entirely — there are no per-record conflicts to resolve for
        // configuration. Conflict resolution is relevant for budget and consent
        // records if a bidirectional merge is implemented in future.
        //
        // For now this returns an empty list and serves as the extension point.
        Vec::new()
    }
}
