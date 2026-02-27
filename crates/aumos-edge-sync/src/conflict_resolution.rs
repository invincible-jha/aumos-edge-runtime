// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! Domain-specific conflict resolution for governance state sync.
//!
//! This module provides deterministic, rule-based conflict resolution for
//! the three categories of governance state that can diverge between local
//! and remote:
//!
//! - **Trust levels** -- assigned by operators, can differ if updated on
//!   the server while the device was offline.
//! - **Budget state** -- consumed locally while the server may have reset
//!   or adjusted the allocation.
//! - **Policy versions** -- the server may publish a new policy while the
//!   device is enforcing a cached version.
//!
//! ## Design Principles
//!
//! All resolution is deterministic and based on static rules. There is no
//! ML-driven merging, no adaptive weighting, and no behavioural analysis.
//! The [`ConflictStrategy`] enum selects which rule set to apply; the
//! [`ConflictResolver`] executes the selected rule.
//!
//! ## Fire Line
//!
//! - No adaptive merging or smart prioritisation.
//! - No behavioural scoring to influence conflict outcomes.
//! - Resolution reasons are recorded for audit; they are not analysed.

use aumos_edge_core::types::TrustLevel;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Strategy
// ---------------------------------------------------------------------------

/// Deterministic strategy for resolving conflicts between local and remote
/// governance state.
///
/// The strategy is configured once at sync engine initialisation and applied
/// uniformly to all conflicts within a sync cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// Pick whichever side grants fewer permissions. For trust levels,
    /// this means the lower level. For budgets, the smaller remaining
    /// allocation. For policies, the newer version (assumed to be more
    /// restrictive by convention).
    MostRestrictiveWins,

    /// Accept both sides and emit an audit record documenting the merge.
    /// For trust levels, the lower level is chosen (same as
    /// `MostRestrictiveWins`). For budgets, the spent amounts are summed.
    /// The key difference is that a conflict record is always emitted,
    /// even when there is no effective divergence.
    MergeAndAudit,

    /// The remote (server) value always overrides the local value.
    RemoteOverridesLocal,

    /// The local (device) value always overrides the remote value.
    /// Useful for air-gapped or sovereign deployments where the device
    /// is the source of truth.
    LocalOverridesRemote,
}

impl Default for ConflictStrategy {
    fn default() -> Self {
        ConflictStrategy::MostRestrictiveWins
    }
}

// ---------------------------------------------------------------------------
// Input types
// ---------------------------------------------------------------------------

/// Budget state from one side of a conflict (local or remote).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetState {
    /// Budget category identifier.
    pub category: String,
    /// Total units allocated for the current period.
    pub total_units: f64,
    /// Units consumed so far in the current period.
    pub consumed: f64,
    /// When this budget state was last updated.
    pub updated_at: DateTime<Utc>,
}

impl BudgetState {
    /// Remaining headroom.
    pub fn remaining(&self) -> f64 {
        (self.total_units - self.consumed).max(0.0)
    }
}

/// Policy version from one side of a conflict (local or remote).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyVersion {
    /// Monotonically increasing version number assigned by the server.
    pub version: u64,
    /// SHA-256 hash of the policy document.
    pub hash: String,
    /// When this version was published.
    pub published_at: DateTime<Utc>,
    /// Raw policy document (serialised EdgeConfig).
    pub payload: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Result of resolving a trust level conflict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedTrust {
    /// The trust level that should be applied after resolution.
    pub value: TrustLevel,
    /// Human-readable explanation of why this value was chosen.
    pub resolution_reason: String,
    /// Whether a genuine conflict existed between local and remote.
    pub conflict_detected: bool,
}

/// Result of resolving a budget state conflict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedBudget {
    /// The budget state that should be applied after resolution.
    pub total_units: f64,
    /// The consumed amount that should be applied after resolution.
    pub consumed: f64,
    /// Human-readable explanation of why these values were chosen.
    pub resolution_reason: String,
    /// Whether a genuine conflict existed between local and remote.
    pub conflict_detected: bool,
}

/// Result of resolving a policy version conflict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPolicy {
    /// The policy version that should be applied after resolution.
    pub version: u64,
    /// The hash of the chosen policy document.
    pub hash: String,
    /// The raw policy document to apply.
    pub payload: serde_json::Value,
    /// Human-readable explanation of why this version was chosen.
    pub resolution_reason: String,
    /// Whether a genuine conflict existed between local and remote.
    pub conflict_detected: bool,
}

// ---------------------------------------------------------------------------
// Resolver
// ---------------------------------------------------------------------------

/// Applies a [`ConflictStrategy`] to resolve divergent governance state.
///
/// # Example
///
/// ```rust
/// use aumos_edge_sync::conflict_resolution::{
///     ConflictStrategy, ConflictResolver,
/// };
/// use aumos_edge_core::types::TrustLevel;
///
/// let resolver = ConflictResolver::new(ConflictStrategy::MostRestrictiveWins);
///
/// let resolved = resolver.resolve_trust_conflict(
///     TrustLevel::Elevated,
///     TrustLevel::Standard,
/// );
///
/// assert_eq!(resolved.value, TrustLevel::Standard);
/// assert!(resolved.conflict_detected);
/// ```
pub struct ConflictResolver {
    strategy: ConflictStrategy,
}

impl ConflictResolver {
    /// Create a new resolver with the given strategy.
    pub fn new(strategy: ConflictStrategy) -> Self {
        Self { strategy }
    }

    /// The strategy this resolver is configured with.
    pub fn strategy(&self) -> ConflictStrategy {
        self.strategy
    }

    /// Resolve a trust level conflict between local and remote.
    ///
    /// Trust levels are compared by their ordinal value. Lower values
    /// represent more restrictive trust.
    pub fn resolve_trust_conflict(
        &self,
        local: TrustLevel,
        remote: TrustLevel,
    ) -> ResolvedTrust {
        let conflict_detected = local != remote;

        if !conflict_detected {
            return ResolvedTrust {
                value: local,
                resolution_reason: "No conflict: local and remote trust levels are identical".to_string(),
                conflict_detected: false,
            };
        }

        match self.strategy {
            ConflictStrategy::MostRestrictiveWins | ConflictStrategy::MergeAndAudit => {
                let value = if local <= remote { local } else { remote };
                ResolvedTrust {
                    value,
                    resolution_reason: format!(
                        "Most restrictive wins: chose '{}' over '{}' (local={}, remote={})",
                        value, if value == local { remote } else { local },
                        local, remote,
                    ),
                    conflict_detected: true,
                }
            }
            ConflictStrategy::RemoteOverridesLocal => {
                ResolvedTrust {
                    value: remote,
                    resolution_reason: format!(
                        "Remote overrides local: chose remote '{}' over local '{}'",
                        remote, local,
                    ),
                    conflict_detected: true,
                }
            }
            ConflictStrategy::LocalOverridesRemote => {
                ResolvedTrust {
                    value: local,
                    resolution_reason: format!(
                        "Local overrides remote: chose local '{}' over remote '{}'",
                        local, remote,
                    ),
                    conflict_detected: true,
                }
            }
        }
    }

    /// Resolve a budget state conflict between local and remote.
    ///
    /// For `MostRestrictiveWins`, the smaller remaining allocation wins.
    /// For `MergeAndAudit`, consumed amounts are summed.
    pub fn resolve_budget_conflict(
        &self,
        local: &BudgetState,
        remote: &BudgetState,
    ) -> ResolvedBudget {
        let conflict_detected =
            (local.total_units - remote.total_units).abs() > f64::EPSILON
            || (local.consumed - remote.consumed).abs() > f64::EPSILON;

        if !conflict_detected {
            return ResolvedBudget {
                total_units: local.total_units,
                consumed: local.consumed,
                resolution_reason: "No conflict: local and remote budget states are identical".to_string(),
                conflict_detected: false,
            };
        }

        match self.strategy {
            ConflictStrategy::MostRestrictiveWins => {
                // Most restrictive = smaller remaining headroom.
                // Use the lower total and the higher consumed.
                let total_units = local.total_units.min(remote.total_units);
                let consumed = local.consumed.max(remote.consumed);
                ResolvedBudget {
                    total_units,
                    consumed,
                    resolution_reason: format!(
                        "Most restrictive wins: total={:.4} (min of {:.4}, {:.4}), \
                         consumed={:.4} (max of {:.4}, {:.4})",
                        total_units, local.total_units, remote.total_units,
                        consumed, local.consumed, remote.consumed,
                    ),
                    conflict_detected: true,
                }
            }
            ConflictStrategy::MergeAndAudit => {
                // Merge: use remote total (server is authoritative for limits),
                // sum consumed amounts (both sides may have spent independently).
                let total_units = remote.total_units;
                let consumed = local.consumed + remote.consumed;
                ResolvedBudget {
                    total_units,
                    consumed,
                    resolution_reason: format!(
                        "Merge and audit: total={:.4} (from remote), \
                         consumed={:.4} (sum of local {:.4} + remote {:.4})",
                        total_units, consumed, local.consumed, remote.consumed,
                    ),
                    conflict_detected: true,
                }
            }
            ConflictStrategy::RemoteOverridesLocal => {
                ResolvedBudget {
                    total_units: remote.total_units,
                    consumed: remote.consumed,
                    resolution_reason: format!(
                        "Remote overrides local: total={:.4}, consumed={:.4}",
                        remote.total_units, remote.consumed,
                    ),
                    conflict_detected: true,
                }
            }
            ConflictStrategy::LocalOverridesRemote => {
                ResolvedBudget {
                    total_units: local.total_units,
                    consumed: local.consumed,
                    resolution_reason: format!(
                        "Local overrides remote: total={:.4}, consumed={:.4}",
                        local.total_units, local.consumed,
                    ),
                    conflict_detected: true,
                }
            }
        }
    }

    /// Resolve a policy version conflict between local and remote.
    ///
    /// For `MostRestrictiveWins`, the newer version is chosen (higher
    /// version number), under the convention that policy updates are
    /// issued to tighten governance, not relax it.
    pub fn resolve_policy_conflict(
        &self,
        local: &PolicyVersion,
        remote: &PolicyVersion,
    ) -> ResolvedPolicy {
        let conflict_detected = local.version != remote.version || local.hash != remote.hash;

        if !conflict_detected {
            return ResolvedPolicy {
                version: local.version,
                hash: local.hash.clone(),
                payload: local.payload.clone(),
                resolution_reason: "No conflict: local and remote policies are identical".to_string(),
                conflict_detected: false,
            };
        }

        match self.strategy {
            ConflictStrategy::MostRestrictiveWins | ConflictStrategy::MergeAndAudit => {
                // Convention: higher version number = more restrictive.
                if remote.version >= local.version {
                    ResolvedPolicy {
                        version: remote.version,
                        hash: remote.hash.clone(),
                        payload: remote.payload.clone(),
                        resolution_reason: format!(
                            "Newer version wins: chose remote v{} over local v{}",
                            remote.version, local.version,
                        ),
                        conflict_detected: true,
                    }
                } else {
                    ResolvedPolicy {
                        version: local.version,
                        hash: local.hash.clone(),
                        payload: local.payload.clone(),
                        resolution_reason: format!(
                            "Newer version wins: chose local v{} over remote v{}",
                            local.version, remote.version,
                        ),
                        conflict_detected: true,
                    }
                }
            }
            ConflictStrategy::RemoteOverridesLocal => {
                ResolvedPolicy {
                    version: remote.version,
                    hash: remote.hash.clone(),
                    payload: remote.payload.clone(),
                    resolution_reason: format!(
                        "Remote overrides local: chose remote v{} (hash {})",
                        remote.version, &remote.hash[..8.min(remote.hash.len())],
                    ),
                    conflict_detected: true,
                }
            }
            ConflictStrategy::LocalOverridesRemote => {
                ResolvedPolicy {
                    version: local.version,
                    hash: local.hash.clone(),
                    payload: local.payload.clone(),
                    resolution_reason: format!(
                        "Local overrides remote: chose local v{} (hash {})",
                        local.version, &local.hash[..8.min(local.hash.len())],
                    ),
                    conflict_detected: true,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- Trust resolution tests -----------------------------------------------

    #[test]
    fn test_trust_no_conflict() {
        let resolver = ConflictResolver::new(ConflictStrategy::MostRestrictiveWins);
        let result = resolver.resolve_trust_conflict(TrustLevel::Standard, TrustLevel::Standard);
        assert!(!result.conflict_detected);
        assert_eq!(result.value, TrustLevel::Standard);
    }

    #[test]
    fn test_trust_most_restrictive_picks_lower() {
        let resolver = ConflictResolver::new(ConflictStrategy::MostRestrictiveWins);
        let result = resolver.resolve_trust_conflict(TrustLevel::Elevated, TrustLevel::Restricted);
        assert!(result.conflict_detected);
        assert_eq!(result.value, TrustLevel::Restricted);
    }

    #[test]
    fn test_trust_most_restrictive_symmetric() {
        let resolver = ConflictResolver::new(ConflictStrategy::MostRestrictiveWins);
        let result_a = resolver.resolve_trust_conflict(TrustLevel::System, TrustLevel::Standard);
        let result_b = resolver.resolve_trust_conflict(TrustLevel::Standard, TrustLevel::System);
        assert_eq!(result_a.value, TrustLevel::Standard);
        assert_eq!(result_b.value, TrustLevel::Standard);
    }

    #[test]
    fn test_trust_remote_overrides() {
        let resolver = ConflictResolver::new(ConflictStrategy::RemoteOverridesLocal);
        let result = resolver.resolve_trust_conflict(TrustLevel::System, TrustLevel::Restricted);
        assert_eq!(result.value, TrustLevel::Restricted);
        assert!(result.conflict_detected);
    }

    #[test]
    fn test_trust_local_overrides() {
        let resolver = ConflictResolver::new(ConflictStrategy::LocalOverridesRemote);
        let result = resolver.resolve_trust_conflict(TrustLevel::System, TrustLevel::Restricted);
        assert_eq!(result.value, TrustLevel::System);
        assert!(result.conflict_detected);
    }

    #[test]
    fn test_trust_merge_and_audit_picks_lower() {
        let resolver = ConflictResolver::new(ConflictStrategy::MergeAndAudit);
        let result = resolver.resolve_trust_conflict(TrustLevel::Elevated, TrustLevel::Standard);
        assert_eq!(result.value, TrustLevel::Standard);
        assert!(result.conflict_detected);
    }

    // -- Budget resolution tests ----------------------------------------------

    fn make_budget(total: f64, consumed: f64) -> BudgetState {
        BudgetState {
            category: "test-category".to_string(),
            total_units: total,
            consumed,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_budget_no_conflict() {
        let resolver = ConflictResolver::new(ConflictStrategy::MostRestrictiveWins);
        let local = make_budget(100.0, 25.0);
        let remote = make_budget(100.0, 25.0);
        let result = resolver.resolve_budget_conflict(&local, &remote);
        assert!(!result.conflict_detected);
        assert!((result.total_units - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_budget_most_restrictive() {
        let resolver = ConflictResolver::new(ConflictStrategy::MostRestrictiveWins);
        let local = make_budget(100.0, 30.0);
        let remote = make_budget(80.0, 50.0);
        let result = resolver.resolve_budget_conflict(&local, &remote);
        assert!(result.conflict_detected);
        // Most restrictive: lower total (80) and higher consumed (50).
        assert!((result.total_units - 80.0).abs() < f64::EPSILON);
        assert!((result.consumed - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_budget_merge_and_audit_sums_consumed() {
        let resolver = ConflictResolver::new(ConflictStrategy::MergeAndAudit);
        let local = make_budget(100.0, 30.0);
        let remote = make_budget(100.0, 20.0);
        let result = resolver.resolve_budget_conflict(&local, &remote);
        assert!(result.conflict_detected);
        // Merge: remote total (100), consumed = 30 + 20 = 50.
        assert!((result.total_units - 100.0).abs() < f64::EPSILON);
        assert!((result.consumed - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_budget_remote_overrides() {
        let resolver = ConflictResolver::new(ConflictStrategy::RemoteOverridesLocal);
        let local = make_budget(100.0, 80.0);
        let remote = make_budget(200.0, 10.0);
        let result = resolver.resolve_budget_conflict(&local, &remote);
        assert!(result.conflict_detected);
        assert!((result.total_units - 200.0).abs() < f64::EPSILON);
        assert!((result.consumed - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_budget_local_overrides() {
        let resolver = ConflictResolver::new(ConflictStrategy::LocalOverridesRemote);
        let local = make_budget(100.0, 80.0);
        let remote = make_budget(200.0, 10.0);
        let result = resolver.resolve_budget_conflict(&local, &remote);
        assert!(result.conflict_detected);
        assert!((result.total_units - 100.0).abs() < f64::EPSILON);
        assert!((result.consumed - 80.0).abs() < f64::EPSILON);
    }

    // -- Policy resolution tests ----------------------------------------------

    fn make_policy(version: u64, hash: &str) -> PolicyVersion {
        PolicyVersion {
            version,
            hash: hash.to_string(),
            published_at: Utc::now(),
            payload: json!({"version": version}),
        }
    }

    #[test]
    fn test_policy_no_conflict() {
        let resolver = ConflictResolver::new(ConflictStrategy::MostRestrictiveWins);
        let local = make_policy(5, "abc123ff");
        let remote = make_policy(5, "abc123ff");
        let result = resolver.resolve_policy_conflict(&local, &remote);
        assert!(!result.conflict_detected);
        assert_eq!(result.version, 5);
    }

    #[test]
    fn test_policy_most_restrictive_picks_newer() {
        let resolver = ConflictResolver::new(ConflictStrategy::MostRestrictiveWins);
        let local = make_policy(3, "aaa");
        let remote = make_policy(7, "bbb");
        let result = resolver.resolve_policy_conflict(&local, &remote);
        assert!(result.conflict_detected);
        assert_eq!(result.version, 7);
        assert_eq!(result.hash, "bbb");
    }

    #[test]
    fn test_policy_most_restrictive_picks_local_when_newer() {
        let resolver = ConflictResolver::new(ConflictStrategy::MostRestrictiveWins);
        let local = make_policy(10, "aaa");
        let remote = make_policy(7, "bbb");
        let result = resolver.resolve_policy_conflict(&local, &remote);
        assert!(result.conflict_detected);
        assert_eq!(result.version, 10);
    }

    #[test]
    fn test_policy_remote_overrides() {
        let resolver = ConflictResolver::new(ConflictStrategy::RemoteOverridesLocal);
        let local = make_policy(10, "aaa");
        let remote = make_policy(2, "bbb");
        let result = resolver.resolve_policy_conflict(&local, &remote);
        assert!(result.conflict_detected);
        assert_eq!(result.version, 2);
    }

    #[test]
    fn test_policy_local_overrides() {
        let resolver = ConflictResolver::new(ConflictStrategy::LocalOverridesRemote);
        let local = make_policy(3, "aaa");
        let remote = make_policy(10, "bbb");
        let result = resolver.resolve_policy_conflict(&local, &remote);
        assert!(result.conflict_detected);
        assert_eq!(result.version, 3);
    }

    #[test]
    fn test_policy_same_version_different_hash() {
        let resolver = ConflictResolver::new(ConflictStrategy::MostRestrictiveWins);
        let local = make_policy(5, "aaa");
        let remote = make_policy(5, "bbb");
        let result = resolver.resolve_policy_conflict(&local, &remote);
        assert!(result.conflict_detected);
        // Same version, so remote wins (>=).
        assert_eq!(result.hash, "bbb");
    }

    // -- Strategy serialisation tests -----------------------------------------

    #[test]
    fn test_strategy_serialisation_roundtrip() {
        let strategies = [
            ConflictStrategy::MostRestrictiveWins,
            ConflictStrategy::MergeAndAudit,
            ConflictStrategy::RemoteOverridesLocal,
            ConflictStrategy::LocalOverridesRemote,
        ];
        for strategy in &strategies {
            let json = serde_json::to_string(strategy).expect("serialise");
            let parsed: ConflictStrategy = serde_json::from_str(&json).expect("deserialise");
            assert_eq!(*strategy, parsed);
        }
    }

    #[test]
    fn test_default_strategy_is_most_restrictive() {
        assert_eq!(ConflictStrategy::default(), ConflictStrategy::MostRestrictiveWins);
    }

    #[test]
    fn test_resolved_trust_serialises() {
        let resolved = ResolvedTrust {
            value: TrustLevel::Standard,
            resolution_reason: "test".to_string(),
            conflict_detected: true,
        };
        let json = serde_json::to_string(&resolved).expect("serialise");
        assert!(json.contains("standard"));
    }

    #[test]
    fn test_resolved_budget_serialises() {
        let resolved = ResolvedBudget {
            total_units: 100.0,
            consumed: 50.0,
            resolution_reason: "test".to_string(),
            conflict_detected: false,
        };
        let json = serde_json::to_string(&resolved).expect("serialise");
        assert!(json.contains("100"));
    }
}
