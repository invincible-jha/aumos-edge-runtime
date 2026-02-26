// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! `ActionQueue` — offline action buffer waiting to sync.
//!
//! When the device is offline, completed actions that need to be reported to
//! the remote server are queued here. On the next successful sync, the queue
//! is drained and the actions are uploaded.
//!
//! The current implementation is in-memory. For durability across restarts,
//! swap the backing store with a file or embedded database.

use crate::sync::SyncError;
use aumos_edge_core::types::CompletedAction;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// An entry in the action queue, wrapping a `CompletedAction` with metadata
/// needed to manage the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedAction {
    /// The completed action waiting for upload.
    pub action: CompletedAction,
    /// When this entry was enqueued.
    pub queued_at: DateTime<Utc>,
    /// Number of failed upload attempts so far.
    pub attempt_count: u32,
}

/// In-memory queue of completed actions pending sync.
#[derive(Debug, Default)]
pub struct ActionQueue {
    entries: Vec<QueuedAction>,
}

impl ActionQueue {
    /// Create a new, empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a completed action for upload on the next sync.
    pub fn enqueue(&mut self, action: CompletedAction) {
        self.entries.push(QueuedAction {
            action,
            queued_at: Utc::now(),
            attempt_count: 0,
        });
    }

    /// Return `true` if the queue contains no pending actions.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return the number of pending actions.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Drain all queued actions and return them, leaving the queue empty.
    ///
    /// The caller is responsible for re-enqueuing on failure.
    pub fn drain_all(&mut self) -> Vec<QueuedAction> {
        std::mem::take(&mut self.entries)
    }

    /// Peek at the queued entries without consuming them.
    pub fn peek(&self) -> &[QueuedAction] {
        &self.entries
    }

    /// Re-add previously drained actions after a failed upload, incrementing
    /// their `attempt_count`.
    pub fn re_enqueue_failed(&mut self, mut failed: Vec<QueuedAction>) {
        for entry in &mut failed {
            entry.attempt_count += 1;
        }
        self.entries.extend(failed);
    }

    /// Remove all entries that have exceeded `max_attempts`.
    ///
    /// Returns the dropped entries so the caller can log them.
    pub fn drop_exceeded(&mut self, max_attempts: u32) -> Vec<QueuedAction> {
        let (keep, dropped): (Vec<_>, Vec<_>) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|entry| entry.attempt_count <= max_attempts);
        self.entries = keep;
        dropped
    }

    /// Persist the current queue state to a JSON file at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError`] if the file cannot be written.
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), SyncError> {
        let json = serde_json::to_string_pretty(&self.entries)
            .map_err(SyncError::Serialization)?;
        std::fs::write(path, json)
            .map_err(|error| SyncError::Io(error.to_string()))
    }

    /// Load queue state from a JSON file at `path`.
    ///
    /// If the file does not exist, returns an empty queue.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError`] if the file exists but cannot be parsed.
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, SyncError> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let raw = std::fs::read_to_string(path)
            .map_err(|error| SyncError::Io(error.to_string()))?;
        let entries: Vec<QueuedAction> = serde_json::from_str(&raw)
            .map_err(SyncError::Serialization)?;
        Ok(Self { entries })
    }
}
