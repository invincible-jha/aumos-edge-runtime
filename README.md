# aumos-edge-runtime

[![Governance Score](https://img.shields.io/badge/governance-self--assessed-blue)](https://github.com/aumos-ai/aumos-edge-runtime)

On-device governance enforcement for AI agents operating offline or on constrained hardware.

Part of the [Aumos](https://aumos.ai) open-source governance stack (Project Quasar, Phase 4).

## Overview

`aumos-edge-runtime` provides a lightweight, configuration-driven governance engine that
runs entirely on-device — no network required for enforcement decisions. When connectivity
returns, it syncs its local audit trail and picks up updated configuration from a remote server.

### Design Principles

- **Configuration-driven** — all rules come from a static `EdgeConfig` file; no adaptive behavior
- **Append-only audit** — every decision is SHA-256 hash-chained; records cannot be silently modified
- **Offline-first** — enforcement never blocks on network; sync is best-effort
- **Minimal footprint** — the core crate targets constrained hardware; no hidden `std` dependencies
- **Binding-ready** — PyO3 and NAPI-RS stubs let Python and TypeScript consumers call the same engine

## Crates

| Crate | Description |
|---|---|
| [`aumos-edge-core`](crates/aumos-edge-core/) | Core governance engine: trust, budget, consent, audit log |
| [`aumos-edge-sync`](crates/aumos-edge-sync/) | Sync engine: push local state, pull remote config |
| [`aumos-edge-python`](crates/aumos-edge-python/) | PyO3 Python binding stubs |
| [`aumos-edge-node`](crates/aumos-edge-node/) | NAPI-RS TypeScript/Node.js binding stubs |

## Quick Start (Rust)

```rust
use aumos_edge_core::{EdgeGovernanceEngine, GovernanceAction, ActionKind};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = EdgeGovernanceEngine::from_config(Path::new("edge-config.toml"))?;

    let action = GovernanceAction {
        agent_id: "agent-001".to_string(),
        kind: ActionKind::DataRead,
        resource: "user-profile".to_string(),
        estimated_cost: 0.01,
        metadata: Default::default(),
    };

    let decision = engine.evaluate(&action);
    println!("Decision: {:?}", decision.outcome);
    Ok(())
}
```

## Architecture

See [`docs/architecture.md`](docs/architecture.md).

## Offline Mode

See [`docs/offline-mode.md`](docs/offline-mode.md).

## Sync Protocol

See [`docs/sync-protocol.md`](docs/sync-protocol.md).

## Deployment

See [`docs/deployment.md`](docs/deployment.md).

## Fire Line

Hard architectural constraints are documented in [`FIRE_LINE.md`](FIRE_LINE.md).

## License

Business Source License 1.1 — see [`LICENSE`](LICENSE).
