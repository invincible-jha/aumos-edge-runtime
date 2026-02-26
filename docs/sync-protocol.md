# Sync Protocol

The `aumos-edge-sync` crate implements the protocol that keeps an edge device
consistent with the central AumOS server. The protocol is **push-then-pull**:
local changes are uploaded first, then the latest server configuration is
downloaded.

---

## Design principles

- **Simplicity over cleverness.** Push everything; pull everything. There is no
  delta-compression, selective record sync, or ordering optimisation.
- **Non-blocking governance.** A failed sync cycle never blocks a governance
  evaluation. The engine continues using its last-known-good config.
- **Audit-first.** Audit records are pushed before budget records and before
  config is pulled. The server always receives the decision log before it
  receives usage data.
- **Mechanical conflict resolution.** Conflicts between local and remote state
  are resolved by a configurable strategy (`last_write_wins` by default) with
  no semantic reasoning.

---

## Sync cycle steps

A single call to `SyncEngine::sync(server_url)` performs the following steps in
order. Each step is non-fatal: a failure is recorded in `SyncReport::warnings`
and the cycle continues.

### Step 1 — Push audit records

All audit log entries stored in the `audit` namespace of local storage are
serialised and sent to `POST {server_url}/audit`.

The server responds with the number of entries it accepted. The local store is
not cleared after a push — the server maintains its own deduplication by
`action_id`. Future implementations may include an acknowledgement-based
pruning step.

### Step 2 — Push budget records

All budget usage records stored in the `budget` namespace of local storage are
serialised and sent to `POST {server_url}/budget`.

Budget records represent cumulative spending within the current window. The
server uses these to update its view of device-level usage.

### Step 3 — Drain the action queue

`ActionQueue` holds `CompletedAction` entries that were queued by the governance
engine after each allowed decision. The queue is drained and its contents are
included in the audit push payload.

### Step 4 — Pull remote config

The engine calls `GET {server_url}/config/{device_id}` to retrieve the current
`EdgeConfig` for this device.

On success, the downloaded config is parsed and passed to `apply_remote_config()`,
which replaces the local rule set entirely. There is no partial merge of
configuration — the remote config is authoritative.

### Step 5 — Resolve conflicts

For data namespaces that support bidirectional updates (budget, consent), the
`ConflictResolver` picks a winner for any record that was modified on both sides
since the last sync.

The default strategy is `last_write_wins`:

```
local.updated_at > remote.updated_at  →  keep local
remote.updated_at > local.updated_at  →  keep remote
equal timestamps or parse failure     →  keep local (conservative)
```

---

## SyncReport

Every sync cycle returns a `SyncReport` regardless of whether individual steps
succeeded:

```json
{
  "started_at": "2026-02-26T10:00:00Z",
  "completed_at": "2026-02-26T10:00:01Z",
  "audit_entries_pushed": 42,
  "budget_records_pushed": 3,
  "config_pulled": true,
  "conflicts_resolved": 1,
  "warnings": []
}
```

Non-empty `warnings` indicate step-level failures that did not abort the cycle.

---

## Transport

`HttpTransport` is the default transport implementation. It sends synchronous
HTTP requests using a blocking HTTP client (suitable for embedded runtimes
without an async executor).

The transport is injected into `SyncEngine` via the `Storage` abstraction
boundary. Alternative transports (MQTT, BLE, serial) can be implemented by
providing a compatible client with the same push/pull surface.

### Authentication

Every request carries a `Bearer` token in the `Authorization` header:

```
Authorization: Bearer <auth_token>
```

The `auth_token` is set at `SyncEngine` construction time and comes from
device-level credential provisioning (e.g., a hardware TPM, secure enclave, or
secrets manager).

---

## Conflict resolution strategies

Set `conflict_strategy` in `SyncConfig`:

| Strategy | Enum variant | Description |
|----------|-------------|-------------|
| Last-write-wins | `LastWriteWins` | Record with the later `updated_at` wins (default) |
| Local always wins | `LocalAlwaysWins` | Local record always takes precedence |
| Remote always wins | `RemoteAlwaysWins` | Remote record always takes precedence |

Selecting `RemoteAlwaysWins` is appropriate for fleets managed centrally where
the server is the single source of truth. Selecting `LocalAlwaysWins` is
appropriate for standalone devices with infrequent operator intervention.

---

## Retry and backoff

`SyncConfig::max_queue_attempts` controls how many times a queued action is
retried before it is dropped. The default is 5.

Backoff scheduling is the responsibility of the calling application. The
`SyncEngine` itself does not implement timers, retry loops, or exponential
backoff. A typical integration looks like:

```python
import time

while True:
    try:
        report = sync_engine.sync("https://your-server/sync")
        if report["warnings"]:
            print("Sync warnings:", report["warnings"])
    except RuntimeError as exc:
        print(f"Sync failed: {exc}")
    time.sleep(300)  # sync every 5 minutes
```

---

## Server API contract

The remote server must implement the following endpoints:

| Endpoint | Method | Body | Response |
|----------|--------|------|----------|
| `/audit` | POST | `{ device_id, entries: [...] }` | `{ accepted: N }` |
| `/budget` | POST | `{ device_id, records: [...] }` | `{ accepted: N }` |
| `/config/{device_id}` | GET | — | `EdgeConfig` as TOML |

The AumOS server implementation is part of the closed-source AumOS product and
is not included in this repository.

---

## Security considerations

- The `auth_token` should be rotated regularly. The sync engine does not
  implement token rotation — handle this at the provisioning layer.
- All transport connections should use TLS. The `HttpTransport` enforces HTTPS
  in production mode.
- The audit log's chain-integrity hash allows the server to detect if records
  were dropped or reordered before upload.
- Do not expose the sync endpoint directly to agents. Sync is an operator-layer
  operation, not an agent-layer one.
