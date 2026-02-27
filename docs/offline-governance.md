<!-- SPDX-License-Identifier: BSL-1.1 -->
<!-- Copyright (c) 2026 MuVeraAI Corporation -->

# Governance Without Internet

## Why Offline Governance Matters

AI agents increasingly operate in environments where reliable internet
connectivity cannot be assumed:

- **Field agents.** Agricultural drones, mining robots, and disaster
  response systems operate in areas with no cellular coverage or
  satellite uplinks that are intermittent at best.
- **Air-gapped networks.** Government, defence, and healthcare
  environments prohibit outbound connectivity from operational networks.
  Governance enforcement must function entirely within the local network
  perimeter.
- **Intermittent connectivity.** Edge deployments on ships, aircraft,
  and remote industrial sites have connectivity windows measured in
  minutes per day. Governance decisions cannot wait for a round-trip to
  the cloud.

In all of these scenarios, the governance engine must be able to evaluate
agent actions locally, record decisions for later audit, and
reconcile state when connectivity is restored.

## Architecture

The AumOS edge runtime implements a three-layer offline governance
architecture:

```text
+----------------------------------------------+
|          Local Governance Engine              |
|  (EdgeGovernanceEngine + EdgeConfig)          |
|  Trust check | Budget check | Consent check  |
+----------------------------------------------+
            |                       |
            v                       v
+-------------------+   +-----------------------+
|  Decision Cache   |   |  Append-Only Audit    |
|  (last N results  |   |  (SHA-256 hash chain) |
|   for replay)     |   |                       |
+-------------------+   +-----------------------+
            |                       |
            v                       v
+----------------------------------------------+
|            Sync Engine                        |
|  Push audit + budget | Pull config + trust    |
|  Conflict resolution on reconnect            |
+----------------------------------------------+
            |
            v
    [ Remote Governance Server ]
```

### Local Governance Engine

The `EdgeGovernanceEngine` evaluates every agent action against locally
cached governance rules. The evaluation pipeline is identical whether
the device is online or offline:

1. **Trust check** -- compare agent's trust level against the action's
   minimum requirement. Trust levels are loaded from the local
   `EdgeConfig` and are never modified at runtime.
2. **Budget check** -- verify that the estimated cost fits within the
   agent's rolling-window budget allocation. Budgets are static; they
   are not rebalanced or predicted.
3. **Consent check** -- verify that explicit consent has been recorded
   for the (agent, resource, action kind) tuple.
4. **Audit log** -- append the decision to the local hash-chained audit
   log regardless of outcome.

### Decision Cache

The engine maintains a buffer of completed actions
(`Vec<CompletedAction>`) that have been evaluated locally but not yet
synced to the remote server. This buffer is drained on the next
successful sync cycle.

### Sync Engine

The `SyncEngine` (`aumos-edge-sync`) manages the push-pull cycle:

1. **Push audit entries** -- upload all local audit records to the
   remote server.
2. **Push budget deltas** -- upload current budget consumption state.
3. **Pull configuration** -- download the latest `EdgeConfig` from the
   server, including any trust level changes made by operators.
4. **Resolve conflicts** -- apply the configured conflict resolution
   strategy to any divergent state.

## Trust Level Enforcement Without Connectivity

Trust levels are assigned by operators and stored in the local
`EdgeConfig`. When the device is offline:

- Trust levels remain static at their last-known values.
- No trust level changes can be received from the server.
- The engine continues to enforce the locally cached trust assignments.
- Trust modifications made by operators on the server are applied on
  the next successful sync pull.

This is safe because trust levels are always set manually by authorised
parties. There is no automatic promotion or demotion that would require
real-time server communication.

## Policy Staleness Detection

Locally cached governance policies can become stale during extended
offline periods. The edge runtime tracks policy freshness through
two mechanisms:

1. **Last sync timestamp.** The `SyncReport::completed_at` field
   records when the last successful sync completed.
2. **Configurable max-age.** Operators can configure a maximum policy
   age. When the time since the last sync exceeds this threshold,
   the engine can be configured to:
   - Continue operating normally (default -- offline-first).
   - Log a warning on every governance decision.
   - Switch to a restrictive fallback mode where only `Restricted`
     trust actions are permitted.

Policy staleness detection is a static comparison against a configured
duration. There is no predictive modelling of when the next sync will
occur.

## Conflict Resolution on Reconnect

When the device reconnects and pulls updated configuration from the
server, conflicts may arise between local and remote state. The
`ConflictResolver` supports four deterministic strategies:

| Strategy                 | Behaviour                                          |
|--------------------------|----------------------------------------------------|
| `MostRestrictiveWins`    | Default. Picks whichever state grants fewer permissions. |
| `MergeAndAudit`          | Merges both sides and logs the conflict for review. |
| `RemoteOverridesLocal`   | Server state always wins.                          |
| `LocalOverridesRemote`   | Local state always wins (useful for air-gapped).   |

All strategies are deterministic and rule-based. There is no ML-driven
merging, no semantic analysis, and no adaptive conflict weighting.

### Most-Restrictive-Wins (Default)

This strategy ensures that governance is never accidentally relaxed by
a sync:

- **Trust levels:** the lower of the two trust levels is chosen.
- **Budget limits:** the smaller remaining allocation is chosen.
- **Consent records:** revocations always win over grants.

### Merge-and-Audit

Both local and remote state are accepted, but a conflict record is
emitted to the audit log so that operators can review the merge
decision post-hoc. This strategy is useful for environments where
audit completeness is more important than strict restriction.

## Operational Considerations

- **Audit log size.** In extended offline periods, the local audit log
  can grow large. The runtime does not automatically truncate the log;
  operators should configure periodic log rotation or a maximum entry
  count.
- **Budget drift.** If the server resets a budget envelope while the
  device is offline, the local spent amount will be out of sync. The
  conflict resolution strategy determines how this is reconciled.
- **Clock drift.** The edge runtime uses the device's local clock for
  timestamps. Significant clock drift can affect rolling-window budget
  enforcement and policy staleness detection. NTP synchronisation is
  recommended where available.
