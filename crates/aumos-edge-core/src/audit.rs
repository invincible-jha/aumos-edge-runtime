// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! `AuditLog` — append-only, SHA-256 hash-chained audit log.
//!
//! Every governance decision is recorded as a log entry. Each entry contains
//! the SHA-256 hash of the previous entry, forming a tamper-evident chain.
//! Querying the log also verifies chain integrity, returning an error if any
//! link has been broken.
//!
//! The log is append-only within the edge runtime; it is never modified after
//! being written. Entries are pushed to the remote server during sync and then
//! optionally pruned locally to manage storage.

use crate::storage::{Storage, StorageRecord};
use crate::types::{EdgeError, GovernanceAction, GovernanceDecision};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const NAMESPACE: &str = "audit";
/// The sentinel hash used as `previous_hash` for the first entry.
const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// A single audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique identifier for this log entry.
    pub entry_id: Uuid,
    /// Sequential position within the log (1-based).
    pub sequence: u64,
    /// The action that was evaluated.
    pub action: GovernanceAction,
    /// The governance decision that was reached.
    pub decision: GovernanceDecision,
    /// SHA-256 hash of the previous entry's canonical serialization.
    pub previous_hash: String,
    /// SHA-256 hash of this entry's canonical serialization (excluding `this_hash`).
    pub this_hash: String,
    /// When the log entry was created.
    pub logged_at: DateTime<Utc>,
}

/// Filter parameters for querying the audit log.
#[derive(Debug, Default, Clone)]
pub struct AuditFilter {
    /// If set, only return entries for this agent.
    pub agent_id: Option<String>,
    /// If set, only return entries at or after this time.
    pub since: Option<DateTime<Utc>>,
    /// If set, only return entries at or before this time.
    pub until: Option<DateTime<Utc>>,
    /// Maximum number of entries to return; `None` means no limit.
    pub limit: Option<usize>,
}

/// Append-only audit log with SHA-256 hash chaining.
pub struct AuditLog<'a> {
    storage: &'a mut dyn Storage,
}

impl<'a> AuditLog<'a> {
    /// Create an `AuditLog` backed by `storage`.
    pub fn new(storage: &'a mut dyn Storage) -> Self {
        Self { storage }
    }

    /// Append a new entry for `action` + `decision` to the log.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeError::Storage`] if the entry cannot be persisted, or
    /// [`EdgeError::AuditChain`] if the existing chain fails integrity verification.
    pub fn log(
        &mut self,
        action: &GovernanceAction,
        decision: &GovernanceDecision,
    ) -> Result<AuditEntry, EdgeError> {
        let previous_entry = self.load_last_entry()?;
        let (previous_hash, next_sequence) = match &previous_entry {
            Some(entry) => (entry.this_hash.clone(), entry.sequence + 1),
            None => (GENESIS_HASH.to_string(), 1),
        };

        let entry_id = Uuid::new_v4();
        let logged_at = Utc::now();

        // Compute hash over the canonical fields (excluding `this_hash`).
        let this_hash = compute_entry_hash(
            entry_id,
            next_sequence,
            action,
            decision,
            &previous_hash,
            logged_at,
        );

        let entry = AuditEntry {
            entry_id,
            sequence: next_sequence,
            action: action.clone(),
            decision: decision.clone(),
            previous_hash,
            this_hash,
            logged_at,
        };

        self.persist_entry(&entry)?;
        Ok(entry)
    }

    /// Query the audit log applying `filter`, verifying chain integrity along
    /// the way.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeError::AuditChain`] if chain integrity is violated.
    pub fn query(&self, filter: &AuditFilter) -> Result<Vec<AuditEntry>, EdgeError> {
        let raw_records = self.storage.list_namespace(NAMESPACE)?;
        let mut entries: Vec<AuditEntry> = raw_records
            .into_iter()
            .filter_map(|record| serde_json::from_value(record.value).ok())
            .collect();

        // Sort by sequence to ensure we verify in order.
        entries.sort_by_key(|entry| entry.sequence);

        // Verify chain integrity.
        self.verify_chain(&entries)?;

        // Apply filter predicates.
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

    /// Return the total number of entries in the log.
    pub fn count(&self) -> Result<usize, EdgeError> {
        Ok(self.storage.list_namespace(NAMESPACE)?.len())
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn load_last_entry(&self) -> Result<Option<AuditEntry>, EdgeError> {
        let mut records = self.storage.list_namespace(NAMESPACE)?;
        records.sort_by_key(|record| record.sequence);
        Ok(records
            .into_iter()
            .last()
            .and_then(|record| serde_json::from_value(record.value).ok()))
    }

    fn persist_entry(&mut self, entry: &AuditEntry) -> Result<(), EdgeError> {
        let sequence = self.storage.next_sequence(NAMESPACE);
        let record = StorageRecord {
            namespace: NAMESPACE.to_string(),
            key: entry.entry_id.to_string(),
            value: serde_json::to_value(entry)?,
            sequence,
            updated_at: entry.logged_at.to_rfc3339(),
        };
        self.storage.put(record)
    }

    fn verify_chain(&self, entries: &[AuditEntry]) -> Result<(), EdgeError> {
        let mut expected_previous = GENESIS_HASH.to_string();

        for entry in entries {
            if entry.previous_hash != expected_previous {
                return Err(EdgeError::AuditChain(format!(
                    "Chain broken at sequence {}: expected previous hash '{}', found '{}'",
                    entry.sequence, expected_previous, entry.previous_hash
                )));
            }

            // Recompute and verify this entry's hash.
            let recomputed = compute_entry_hash(
                entry.entry_id,
                entry.sequence,
                &entry.action,
                &entry.decision,
                &entry.previous_hash,
                entry.logged_at,
            );
            if recomputed != entry.this_hash {
                return Err(EdgeError::AuditChain(format!(
                    "Entry hash mismatch at sequence {}: stored '{}', computed '{}'",
                    entry.sequence, entry.this_hash, recomputed
                )));
            }

            expected_previous = entry.this_hash.clone();
        }

        Ok(())
    }
}

/// Compute the SHA-256 hash of an entry's canonical fields.
///
/// The hash input is a deterministic JSON serialization of the key fields
/// (excluding `this_hash` to avoid circularity).
fn compute_entry_hash(
    entry_id: Uuid,
    sequence: u64,
    action: &GovernanceAction,
    decision: &GovernanceDecision,
    previous_hash: &str,
    logged_at: DateTime<Utc>,
) -> String {
    // Build a deterministic canonical representation.
    let canonical = serde_json::json!({
        "entry_id": entry_id.to_string(),
        "sequence": sequence,
        "action_id": action.action_id.to_string(),
        "agent_id": action.agent_id,
        "decision_outcome": format!("{:?}", decision.outcome),
        "decision_stage": format!("{:?}", decision.decided_by),
        "previous_hash": previous_hash,
        "logged_at": logged_at.to_rfc3339(),
    });

    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string().as_bytes());
    hex::encode(hasher.finalize())
}
