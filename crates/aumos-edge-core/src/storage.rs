// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! Storage abstraction for governance state.
//!
//! The `Storage` trait defines the interface that all persistence backends
//! must implement. `InMemoryStorage` provides a simple in-process
//! implementation suitable for testing and single-process deployments.
//!
//! Storage holds **only** governance state:
//! - Trust level assignments
//! - Budget consumption records
//! - Consent grants and revocations
//! - Audit log entries (hash-chained)
//!
//! No agent payloads, no semantic content, and no model state are stored here.

use crate::types::EdgeError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A raw key-value record stored by the governance engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageRecord {
    /// Logical namespace (e.g. `"trust"`, `"budget"`, `"consent"`, `"audit"`).
    pub namespace: String,
    /// Unique key within the namespace.
    pub key: String,
    /// Serialized JSON value.
    pub value: serde_json::Value,
    /// Monotonically increasing write sequence number within the namespace.
    pub sequence: u64,
    /// ISO-8601 timestamp of the last write.
    pub updated_at: String,
}

/// Trait defining the storage interface for the edge runtime.
///
/// Implementors must ensure that:
/// - Writes are atomic at the record level.
/// - `list_namespace` returns all records in insertion/sequence order.
/// - `clear_namespace` removes all records for the given namespace.
pub trait Storage: Send + Sync {
    /// Write or overwrite a record in `namespace` under `key`.
    fn put(&mut self, record: StorageRecord) -> Result<(), EdgeError>;

    /// Read the record for `namespace` + `key`, or `None` if absent.
    fn get(&self, namespace: &str, key: &str) -> Result<Option<StorageRecord>, EdgeError>;

    /// List all records in `namespace`, sorted by `sequence` ascending.
    fn list_namespace(&self, namespace: &str) -> Result<Vec<StorageRecord>, EdgeError>;

    /// Remove all records in `namespace`.
    fn clear_namespace(&mut self, namespace: &str) -> Result<(), EdgeError>;

    /// Return the next sequence number for `namespace`.
    fn next_sequence(&self, namespace: &str) -> u64;
}

/// In-process, non-persistent storage backed by a `HashMap`.
///
/// Intended for testing and environments where durability is not required
/// (e.g., processes that will re-hydrate from a sync after restart).
#[derive(Debug, Default)]
pub struct InMemoryStorage {
    /// Outer key: namespace, inner key: record key.
    records: HashMap<String, HashMap<String, StorageRecord>>,
    /// Per-namespace sequence counters.
    sequences: HashMap<String, u64>,
}

impl InMemoryStorage {
    /// Create a new, empty in-memory storage instance.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Storage for InMemoryStorage {
    fn put(&mut self, record: StorageRecord) -> Result<(), EdgeError> {
        let namespace_map = self
            .records
            .entry(record.namespace.clone())
            .or_insert_with(HashMap::new);
        namespace_map.insert(record.key.clone(), record);
        Ok(())
    }

    fn get(&self, namespace: &str, key: &str) -> Result<Option<StorageRecord>, EdgeError> {
        Ok(self
            .records
            .get(namespace)
            .and_then(|namespace_map| namespace_map.get(key))
            .cloned())
    }

    fn list_namespace(&self, namespace: &str) -> Result<Vec<StorageRecord>, EdgeError> {
        let mut records: Vec<StorageRecord> = self
            .records
            .get(namespace)
            .map(|namespace_map| namespace_map.values().cloned().collect())
            .unwrap_or_default();

        records.sort_by_key(|record| record.sequence);
        Ok(records)
    }

    fn clear_namespace(&mut self, namespace: &str) -> Result<(), EdgeError> {
        self.records.remove(namespace);
        self.sequences.remove(namespace);
        Ok(())
    }

    fn next_sequence(&self, namespace: &str) -> u64 {
        self.sequences.get(namespace).copied().unwrap_or(0) + 1
    }
}
