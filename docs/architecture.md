# aumos-edge-runtime — Architecture

## Overview

`aumos-edge-runtime` is a Rust workspace that provides on-device AI-agent governance
enforcement for constrained or offline environments. Every agent action passes through
a local evaluation pipeline before it is permitted to execute. No network connection is
required for governance decisions.

The runtime is split into four crates with clearly separated concerns:

```
aumos-edge-runtime/
├── crates/
│   ├── aumos-edge-core/    Core governance logic
│   ├── aumos-edge-sync/    Sync engine (push/pull)
│   ├── aumos-edge-python/  PyO3 Python bindings
│   └── aumos-edge-node/    NAPI-RS Node.js/TypeScript bindings
└── examples/
    ├── rust/
    ├── python/
    └── typescript/
```

---

## Crate: aumos-edge-core

The core crate contains all governance logic. It has no runtime dependencies on the
network, a database, or any external service. Its only I/O is reading the TOML
configuration file at startup and reading/writing to a `Storage` backend.

### Governance pipeline

Every proposed action passes through a fixed, sequential pipeline:

```
GovernanceAction
    │
    ▼
┌──────────────┐   Denied / UnknownAgent
│  TrustCheck  │──────────────────────────► GovernanceDecision { Denied }
└──────┬───────┘
       │ Sufficient
       ▼
┌──────────────┐   ExceedsBudget
│ BudgetCheck  │──────────────────────────► GovernanceDecision { Denied }
└──────┬───────┘
       │ WithinBudget
       ▼
┌──────────────┐   Required (no grant)
│ ConsentCheck │──────────────────────────► GovernanceDecision { RequiresConsent }
└──────┬───────┘
       │ Granted / NotRequired
       ▼
┌──────────────┐
│  AuditLog    │  (always appended, including denials)
└──────┬───────┘
       │
       ▼
 GovernanceDecision { Allowed }
```

The pipeline is always run to completion for every action — there are no short-circuit
paths that skip audit logging.

### Modules

| Module | Responsibility |
|--------|----------------|
| `governance` | `EdgeGovernanceEngine` — top-level composition, runs pipeline |
| `trust` | `TrustChecker` — looks up agent trust level against action requirements |
| `budget` | `BudgetTracker` — enforces static per-agent spending limits |
| `consent` | `ConsentStore` — checks and records operator-granted consent |
| `audit` | `AuditLog` — append-only record of every decision |
| `config` | `EdgeConfig` — TOML loader and config accessors |
| `storage` | `Storage` trait + `InMemoryStorage` default implementation |
| `types` | Shared domain types: `GovernanceAction`, `GovernanceDecision`, etc. |

### Storage abstraction

The `Storage` trait decouples governance logic from persistence. The default
implementation (`InMemoryStorage`) holds records in a `HashMap`. Production
deployments can substitute a SQLite, RocksDB, or flat-file backend by implementing
the trait.

```rust
pub trait Storage: Send {
    fn get(&self, namespace: &str, key: &str) -> Result<Option<StorageRecord>, EdgeError>;
    fn set(&mut self, record: StorageRecord) -> Result<(), EdgeError>;
    fn delete(&mut self, namespace: &str, key: &str) -> Result<(), EdgeError>;
    fn list_namespace(&self, namespace: &str) -> Result<Vec<StorageRecord>, EdgeError>;
}
```

### Trust levels

Trust is assigned statically in the TOML configuration and changed only by a human
operator. There is no automatic promotion based on observed behaviour.

| Level | Description |
|-------|-------------|
| `restricted` | Minimal safe action set |
| `standard` | Default for authenticated agents |
| `elevated` | Additional verification steps completed |
| `system` | Internal system agents — full permissions |

---

## Crate: aumos-edge-sync

The sync crate drives communication between the edge device and the remote AumOS
server. Sync is intentionally simple:

1. **Push** all local audit records not yet acknowledged by the server.
2. **Push** current budget usage records.
3. **Pull** the latest `EdgeConfig` from the server.
4. **Resolve** any conflicts using the configured `ResolutionStrategy`.

There is no selective sync, no ML-driven prioritisation, and no semantic
understanding of the records being transferred. Every record is pushed; the
current server config is pulled.

### Conflict resolution strategies

| Strategy | Behaviour |
|----------|-----------|
| `last_write_wins` | Record with the later `updated_at` timestamp wins (default) |
| `local_always_wins` | Local record always wins |
| `remote_always_wins` | Remote record always wins |

### Offline behaviour

When the device cannot reach the server, sync fails with a transport error. The
engine logs the error and continues operating using the last successfully pulled
config. Pending audit and budget records accumulate in local storage and are
pushed on the next successful sync cycle.

See [offline-mode.md](offline-mode.md) for details.

---

## Crate: aumos-edge-python

The Python binding crate uses [PyO3](https://pyo3.rs) and
[maturin](https://github.com/PyO3/maturin) to expose `GovernanceEngine` and
`SyncEngine` to Python.

Build:

```bash
pip install maturin
cd crates/aumos-edge-python
maturin develop --features python
```

The Python package name is `aumos_edge`. A thin Python shim at
`crates/aumos-edge-python/python/aumos_edge/__init__.py` provides:

- `GovernanceEngine.from_config(path)` — load engine from TOML
- `GovernanceEngine.evaluate(agent_id, kind, resource, cost)` — returns JSON string
- `GovernanceEngine.grant_consent(agent_id, resource, kind)` — record consent
- `SyncEngine(device_id, auth_token)` — create sync engine
- `SyncEngine.sync(server_url)` — run a sync cycle, returns JSON string
- `aumos_edge.evaluate(engine, ...)` — convenience wrapper that parses JSON

See [examples/python/edge_example.py](../examples/python/edge_example.py) for a
runnable walkthrough.

---

## Crate: aumos-edge-node

The TypeScript binding crate uses [NAPI-RS](https://napi.rs) to expose
`GovernanceEngine` and `SyncEngine` to Node.js.

Build:

```bash
npm install -g @napi-rs/cli
cd crates/aumos-edge-node
napi build --platform --release --features nodejs
```

The npm package name is `@aumos/edge-runtime`. Public API:

- `GovernanceEngine.fromConfig(configPath)` — load engine from TOML
- `engine.evaluate(agentId, kind, resource, cost)` — returns JSON string
- `new SyncEngine(deviceId, authToken)` — create sync engine
- `syncEngine.sync(serverUrl)` — run a sync cycle, returns JSON string

See [examples/typescript/edge_example.ts](../examples/typescript/edge_example.ts)
for a runnable walkthrough.

---

## Data flow summary

```
TOML config file
       │
       ▼
EdgeGovernanceEngine.from_config()
       │
       │  ┌─────────────────────────┐
       ├──► evaluate(GovernanceAction) ──► GovernanceDecision (JSON to bindings)
       │  └─────────────────────────┘
       │
       │  ┌───────────────────────┐
       └──► SyncEngine.sync(url)  ──► SyncReport (JSON to bindings)
          └───────────────────────┘
               │          ▲
         Push audit     Pull config
         & budget       (EdgeConfig TOML)
               │          │
               ▼          │
          Remote AumOS server
```

All governance decisions — allowed, denied, or pending consent — are written to the
local audit log before the decision is returned to the caller. The audit log is the
source of truth for what the edge device has done.
