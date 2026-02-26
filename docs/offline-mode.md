# Offline Mode

`aumos-edge-runtime` is designed as a **connectivity-optional** runtime. Every
governance check runs locally against configuration and state held on the device.
No network call is made during a governance evaluation. The device can be
fully air-gapped for extended periods without degrading enforcement quality.

---

## What stays local

| Data | Location | Persists across restart? |
|------|----------|--------------------------|
| `EdgeConfig` (rules) | Loaded from TOML at startup | Yes — file on disk |
| Agent trust assignments | Embedded in `EdgeConfig` | Yes |
| Budget usage records | `Storage` backend | Depends on backend |
| Consent grants | `Storage` backend | Depends on backend |
| Audit log | `Storage` backend | Depends on backend |
| Sync queue | `ActionQueue` in `aumos-edge-sync` | Depends on backend |

The default `InMemoryStorage` loses all state when the process exits. For persistent
offline operation, substitute a file-backed or SQLite storage backend.

---

## All checks run locally

### Trust check

`TrustChecker` reads the agent trust map from the in-memory `EdgeConfig`. It never
contacts a server to resolve trust. The trust level assigned to an agent at
configuration time is authoritative until the config is updated via sync.

```
evaluate(action)
    │
    ▼
TrustChecker::check()
    ├── agent_trust_map().get(agent_id)   ← from loaded EdgeConfig
    └── action_trust_map().get(kind)      ← from loaded EdgeConfig
```

### Budget check

`BudgetTracker` reads and writes spending records to the local `Storage` backend.
The rolling window is evaluated against locally recorded timestamps. No server
validation occurs.

Budget limits are static — set in the TOML and updated only when the sync engine
pulls a new config. There is no dynamic adjustment based on usage patterns.

### Consent check

`ConsentStore` reads consent grants from the local `Storage` backend. Consent grants
are recorded by the device operator (e.g., via a local CLI or the Python/Node API)
and stored locally. An action that requires consent will receive a
`RequiresConsent` decision until an operator explicitly records a grant.

---

## Queuing for sync

When connectivity returns, pending local state is pushed to the server via
`SyncEngine.sync()`. Internally, two types of records are queued:

1. **Audit entries** — every governance decision recorded in the audit log namespace
   of local storage.
2. **Budget records** — current usage deltas for each agent.

The `ActionQueue` in `aumos-edge-sync` tracks actions that were completed while
offline. On the next sync cycle, this queue is drained and its contents are included
in the push payload.

```
While offline:
    evaluate(action) → GovernanceDecision
    audit_log.append(entry)         // local storage
    budget_tracker.record(cost)     // local storage
    queue.push(completed_action)    // sync queue

On reconnect:
    sync_engine.sync(server_url)
        → push audit entries
        → push budget records
        → drain action queue
        → pull updated EdgeConfig
```

---

## Configuration staleness

When the device is offline for an extended period, the local `EdgeConfig` may
diverge from the server's current policy. The runtime enforces the last-known-good
config until a sync succeeds.

Operators should account for this when deploying:

- Set conservative defaults in the TOML (e.g., `default_trust_level = "restricted"`)
  so that unknown agents are constrained during extended offline periods.
- Use `deny_unknown_agents = true` if strict policy enforcement during offline
  periods is required.

---

## Storage backend selection

The `Storage` trait is the integration point for persistence:

```rust
pub trait Storage: Send {
    fn get(&self, namespace: &str, key: &str) -> Result<Option<StorageRecord>, EdgeError>;
    fn set(&mut self, record: StorageRecord) -> Result<(), EdgeError>;
    fn delete(&mut self, namespace: &str, key: &str) -> Result<(), EdgeError>;
    fn list_namespace(&self, namespace: &str) -> Result<Vec<StorageRecord>, EdgeError>;
}
```

| Backend | Use case |
|---------|----------|
| `InMemoryStorage` (built-in) | Development, ephemeral jobs, testing |
| SQLite (external crate) | Long-running devices that need persistence across restarts |
| RocksDB (external crate) | High-write-throughput embedded deployments |
| Flat-file JSON (custom) | Minimal-footprint deployments with infrequent writes |

The backend is injected at engine construction time:

```rust
let engine = EdgeGovernanceEngine::with_storage(config, Box::new(MyPersistentStorage::new()));
```

---

## Audit log integrity

The audit log uses a chain-integrity mechanism — each entry includes a hash of the
previous entry. This ensures that the log sequence is tamper-evident. On sync, the
server can verify chain integrity before accepting the pushed records.

If the device is offline for a long period and accumulates many entries, the full
chain is pushed in one sync cycle. There is no partial-push protocol; if a push
fails mid-way, the full set is retried on the next cycle.

---

## Recommended offline deployment checklist

1. Choose a persistent `Storage` backend (SQLite or similar).
2. Pre-provision the TOML config via your deployment pipeline before the device
   goes offline.
3. Set `deny_unknown_agents = true` and explicit agent trust entries to avoid
   open-ended defaults.
4. Ensure the device clock is accurate — budget windows and audit timestamps
   depend on local time.
5. Size the audit log storage to accommodate the maximum expected offline duration
   times your estimated action rate.
6. Implement a reconnect-and-sync loop in your agent runtime to drain the queue
   whenever network access is restored.
