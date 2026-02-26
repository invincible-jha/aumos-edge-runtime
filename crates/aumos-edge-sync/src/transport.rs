// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! `HttpTransport` — HTTP transport layer for sync operations.
//!
//! Handles the raw HTTP requests needed to push local data to and pull
//! configuration from the remote governance server.
//!
//! This is intentionally minimal: no retry logic, no streaming, no
//! smart prioritization. Push everything, pull everything.

use crate::sync::SyncError;
use aumos_edge_core::audit::AuditEntry;
use aumos_edge_core::config::EdgeConfig;
use serde::{Deserialize, Serialize};

/// A batch of audit entries being pushed to the server.
#[derive(Debug, Serialize)]
pub struct PushAuditRequest {
    /// Device or deployment identifier.
    pub device_id: String,
    /// All audit entries being uploaded in this batch.
    pub entries: Vec<AuditEntry>,
}

/// Server acknowledgement for a push request.
#[derive(Debug, Deserialize)]
pub struct PushAuditResponse {
    /// Number of entries successfully accepted.
    pub accepted: usize,
    /// Server-assigned identifier for this batch, used for idempotency.
    pub batch_id: String,
}

/// A batch of budget delta records being pushed to the server.
#[derive(Debug, Serialize)]
pub struct PushBudgetRequest {
    /// Device or deployment identifier.
    pub device_id: String,
    /// Raw JSON budget state records.
    pub records: Vec<serde_json::Value>,
}

/// Server acknowledgement for a budget push.
#[derive(Debug, Deserialize)]
pub struct PushBudgetResponse {
    pub accepted: usize,
}

/// Thin wrapper over `ureq` providing typed HTTP methods for sync operations.
pub struct HttpTransport {
    /// Base URL of the remote governance server (no trailing slash).
    base_url: String,
    /// Bearer token sent with every request.
    auth_token: String,
}

impl HttpTransport {
    /// Create a new `HttpTransport` targeting `base_url` with `auth_token`.
    pub fn new(base_url: impl Into<String>, auth_token: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            auth_token: auth_token.into(),
        }
    }

    /// POST a batch of audit entries to the server.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Transport`] on HTTP or network failure.
    pub fn push_audit_entries(
        &self,
        device_id: &str,
        entries: Vec<AuditEntry>,
    ) -> Result<PushAuditResponse, SyncError> {
        let url = format!("{}/v1/sync/audit", self.base_url);
        let payload = PushAuditRequest {
            device_id: device_id.to_string(),
            entries,
        };
        let response: PushAuditResponse = ureq::post(&url)
            .set("Authorization", &format!("Bearer {}", self.auth_token))
            .set("Content-Type", "application/json")
            .send_json(serde_json::to_value(&payload).map_err(SyncError::Serialization)?)
            .map_err(|error| SyncError::Transport(error.to_string()))?
            .into_json()
            .map_err(|error| SyncError::Transport(error.to_string()))?;
        Ok(response)
    }

    /// POST budget state records to the server.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Transport`] on HTTP or network failure.
    pub fn push_budget_records(
        &self,
        device_id: &str,
        records: Vec<serde_json::Value>,
    ) -> Result<PushBudgetResponse, SyncError> {
        let url = format!("{}/v1/sync/budget", self.base_url);
        let payload = PushBudgetRequest {
            device_id: device_id.to_string(),
            records,
        };
        let response: PushBudgetResponse = ureq::post(&url)
            .set("Authorization", &format!("Bearer {}", self.auth_token))
            .set("Content-Type", "application/json")
            .send_json(serde_json::to_value(&payload).map_err(SyncError::Serialization)?)
            .map_err(|error| SyncError::Transport(error.to_string()))?
            .into_json()
            .map_err(|error| SyncError::Transport(error.to_string()))?;
        Ok(response)
    }

    /// GET the latest `EdgeConfig` from the server.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError::Transport`] on HTTP or network failure, or
    /// [`SyncError::Serialization`] if the response cannot be parsed.
    pub fn pull_config(&self, device_id: &str) -> Result<EdgeConfig, SyncError> {
        let url = format!("{}/v1/sync/config/{}", self.base_url, device_id);
        let config: EdgeConfig = ureq::get(&url)
            .set("Authorization", &format!("Bearer {}", self.auth_token))
            .call()
            .map_err(|error| SyncError::Transport(error.to_string()))?
            .into_json()
            .map_err(|error| SyncError::Transport(error.to_string()))?;
        Ok(config)
    }
}
