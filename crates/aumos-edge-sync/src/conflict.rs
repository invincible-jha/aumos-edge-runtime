// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! `ConflictResolver` — conflict resolution for sync merges.
//!
//! When local and remote state diverge (e.g., a budget was spent locally while
//! offline but the server also has an updated allocation), the resolver picks
//! a winner.
//!
//! The default strategy is **last-write-wins** based on the `updated_at`
//! timestamp. Custom strategies can be plugged in via the `ResolutionStrategy`
//! enum.
//!
//! FIRE LINE: There is no semantic or smart prioritisation here. Every
//! conflict is resolved mechanically.

use aumos_edge_core::storage::StorageRecord;
use chrono::DateTime;
use serde::{Deserialize, Serialize};

/// How conflicts between local and remote records are resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStrategy {
    /// The record with the later `updated_at` timestamp wins.
    LastWriteWins,
    /// The local record always wins, regardless of timestamp.
    LocalAlwaysWins,
    /// The remote record always wins, regardless of timestamp.
    RemoteAlwaysWins,
}

impl Default for ResolutionStrategy {
    fn default() -> Self {
        ResolutionStrategy::LastWriteWins
    }
}

/// Records a resolution decision for logging / audit.
#[derive(Debug, Clone)]
pub struct ConflictResolution {
    /// The namespace of the conflicting record.
    pub namespace: String,
    /// The key of the conflicting record.
    pub key: String,
    /// Which side was chosen.
    pub winner: ConflictWinner,
    /// The strategy used.
    pub strategy: ResolutionStrategy,
}

/// Which side won the conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictWinner {
    Local,
    Remote,
}

/// Resolves conflicts between local and remote `StorageRecord`s.
pub struct ConflictResolver {
    strategy: ResolutionStrategy,
}

impl ConflictResolver {
    /// Create a resolver with the given strategy.
    pub fn new(strategy: ResolutionStrategy) -> Self {
        Self { strategy }
    }

    /// Create a resolver using the default `LastWriteWins` strategy.
    pub fn last_write_wins() -> Self {
        Self::new(ResolutionStrategy::LastWriteWins)
    }

    /// Resolve a conflict between `local` and `remote`.
    ///
    /// Returns the winning record along with a resolution log entry.
    pub fn resolve(
        &self,
        local: &StorageRecord,
        remote: &StorageRecord,
    ) -> (StorageRecord, ConflictResolution) {
        let winner = match self.strategy {
            ResolutionStrategy::LastWriteWins => {
                self.pick_by_timestamp(local, remote)
            }
            ResolutionStrategy::LocalAlwaysWins => ConflictWinner::Local,
            ResolutionStrategy::RemoteAlwaysWins => ConflictWinner::Remote,
        };

        let winning_record = match winner {
            ConflictWinner::Local => local.clone(),
            ConflictWinner::Remote => remote.clone(),
        };

        let resolution = ConflictResolution {
            namespace: local.namespace.clone(),
            key: local.key.clone(),
            winner: winner.clone(),
            strategy: self.strategy,
        };

        (winning_record, resolution)
    }

    fn pick_by_timestamp(&self, local: &StorageRecord, remote: &StorageRecord) -> ConflictWinner {
        // Parse RFC-3339 timestamps; fall back to Local on parse failure.
        let local_time = DateTime::parse_from_rfc3339(&local.updated_at).ok();
        let remote_time = DateTime::parse_from_rfc3339(&remote.updated_at).ok();

        match (local_time, remote_time) {
            (Some(local_ts), Some(remote_ts)) => {
                if remote_ts > local_ts {
                    ConflictWinner::Remote
                } else {
                    ConflictWinner::Local
                }
            }
            // If we can't parse either timestamp, prefer local.
            _ => ConflictWinner::Local,
        }
    }
}
