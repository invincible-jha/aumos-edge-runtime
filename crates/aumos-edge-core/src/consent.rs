// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! `ConsentStore` — local consent checking.
//!
//! The consent store answers the question:
//! "Has the agent previously been granted consent to perform this action
//! on this resource?"
//!
//! Consent grants are recorded by the caller after the user (or a higher-trust
//! system component) approves. Revocations are honoured immediately.

use crate::config::EdgeConfig;
use crate::storage::{Storage, StorageRecord};
use crate::types::{ActionKind, DecisionStage, EdgeError, GovernanceAction, GovernanceDecision};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const NAMESPACE: &str = "consent";

/// A consent grant stored for a specific (agent, resource, action kind) tuple.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    /// The agent that has been granted consent.
    pub agent_id: String,
    /// The resource the consent covers.
    pub resource: String,
    /// The action kind the consent covers.
    pub action_kind: String,
    /// When the consent was granted.
    pub granted_at: DateTime<Utc>,
    /// Optional expiry; `None` means the consent does not expire.
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether the consent has been explicitly revoked.
    pub revoked: bool,
}

impl ConsentRecord {
    /// Returns `true` if the consent is currently valid (not revoked and not expired).
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        if self.revoked {
            return false;
        }
        if let Some(expiry) = self.expires_at {
            return now < expiry;
        }
        true
    }

    /// Storage key for this record.
    fn storage_key(agent_id: &str, resource: &str, action_kind: &str) -> String {
        format!("{agent_id}::{resource}::{action_kind}")
    }
}

/// The outcome of a consent check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsentResult {
    /// Valid consent exists for this (agent, resource, action kind) tuple.
    Granted,
    /// No consent required for this action on this resource.
    NotRequired,
    /// Consent is required but has not been granted (or has been revoked/expired).
    Required { reason: String },
}

/// Checks and records consent for (agent, resource, action kind) tuples.
pub struct ConsentStore<'a> {
    config: &'a EdgeConfig,
    storage: &'a mut dyn Storage,
}

impl<'a> ConsentStore<'a> {
    /// Create a new `ConsentStore`.
    pub fn new(config: &'a EdgeConfig, storage: &'a mut dyn Storage) -> Self {
        Self { config, storage }
    }

    /// Check whether `action` either (a) does not require consent or
    /// (b) has valid consent already recorded.
    pub fn check(&self, action: &GovernanceAction) -> ConsentResult {
        let action_kind_str = self.action_kind_to_str(&action.kind);

        if !self.consent_required_for(action, &action_kind_str) {
            return ConsentResult::NotRequired;
        }

        // Consent is required — look up existing record.
        let key = ConsentRecord::storage_key(
            &action.agent_id,
            &action.resource,
            &action_kind_str,
        );
        let record = self
            .storage
            .get(NAMESPACE, &key)
            .ok()
            .flatten()
            .and_then(|storage_record| {
                serde_json::from_value::<ConsentRecord>(storage_record.value).ok()
            });

        match record {
            Some(consent) if consent.is_valid(Utc::now()) => ConsentResult::Granted,
            Some(consent) if consent.revoked => ConsentResult::Required {
                reason: "Consent has been explicitly revoked".to_string(),
            },
            Some(_) => ConsentResult::Required {
                reason: "Consent has expired".to_string(),
            },
            None => ConsentResult::Required {
                reason: format!(
                    "No consent found for agent '{}' on resource '{}' for action '{}'",
                    action.agent_id, action.resource, action_kind_str
                ),
            },
        }
    }

    /// Record a new consent grant.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeError::Storage`] if the record cannot be persisted.
    pub fn record_consent(
        &mut self,
        agent_id: impl Into<String>,
        resource: impl Into<String>,
        action_kind: impl Into<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(), EdgeError> {
        let agent_id = agent_id.into();
        let resource = resource.into();
        let action_kind = action_kind.into();
        let key = ConsentRecord::storage_key(&agent_id, &resource, &action_kind);

        let consent = ConsentRecord {
            agent_id,
            resource,
            action_kind,
            granted_at: Utc::now(),
            expires_at,
            revoked: false,
        };

        let sequence = self.storage.next_sequence(NAMESPACE);
        let storage_record = StorageRecord {
            namespace: NAMESPACE.to_string(),
            key,
            value: serde_json::to_value(&consent)?,
            sequence,
            updated_at: Utc::now().to_rfc3339(),
        };
        self.storage.put(storage_record)
    }

    /// Mark an existing consent grant as revoked.
    ///
    /// If no consent record exists for the tuple, this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeError::Storage`] if the updated record cannot be persisted.
    pub fn revoke_consent(
        &mut self,
        agent_id: &str,
        resource: &str,
        action_kind: &str,
    ) -> Result<(), EdgeError> {
        let key = ConsentRecord::storage_key(agent_id, resource, action_kind);
        let existing = self.storage.get(NAMESPACE, &key).ok().flatten();

        if let Some(storage_record) = existing {
            if let Ok(mut consent) =
                serde_json::from_value::<ConsentRecord>(storage_record.value)
            {
                consent.revoked = true;
                let sequence = self.storage.next_sequence(NAMESPACE);
                let updated = StorageRecord {
                    namespace: NAMESPACE.to_string(),
                    key,
                    value: serde_json::to_value(&consent)?,
                    sequence,
                    updated_at: Utc::now().to_rfc3339(),
                };
                self.storage.put(updated)?;
            }
        }
        Ok(())
    }

    /// Convert a [`ConsentResult`] to a [`GovernanceDecision`].
    ///
    /// Returns `None` if the check passed, or `Some(decision)` with a
    /// `RequiresConsent` outcome if consent is needed.
    pub fn to_decision(&self, action_id: Uuid, result: &ConsentResult) -> Option<GovernanceDecision> {
        match result {
            ConsentResult::Granted | ConsentResult::NotRequired => None,
            ConsentResult::Required { reason } => {
                Some(GovernanceDecision::requires_consent(action_id, reason.clone()))
            }
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn consent_required_for(&self, action: &GovernanceAction, action_kind_str: &str) -> bool {
        self.config
            .consent_requirements
            .iter()
            .any(|requirement| {
                action.resource.starts_with(&requirement.resource_pattern)
                    && requirement
                        .required_for_kinds
                        .iter()
                        .any(|k| k == action_kind_str)
            })
    }

    fn action_kind_to_str(&self, kind: &ActionKind) -> String {
        serde_json::to_value(kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "custom".to_string())
    }
}
