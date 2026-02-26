# aumos-edge-runtime — Claude Context

## Project Role

Phase 4, Project 4.6 of Project Quasar.
On-device governance enforcement for AI agents operating offline or on constrained hardware.

## Crate Map

| Crate | Purpose |
|---|---|
| `aumos-edge-core` | Pure governance logic — trust, budget, consent, audit |
| `aumos-edge-sync` | Sync engine — push local state, pull remote config |
| `aumos-edge-python` | PyO3 binding stubs for Python consumers |
| `aumos-edge-node` | NAPI-RS binding stubs for TypeScript/Node consumers |

## Forbidden Identifiers

Never use: `progressLevel`, `promoteLevel`, `computeTrustScore`, `behavioralScore`,
`adaptiveBudget`, `optimizeBudget`, `predictSpending`, `detectAnomaly`,
`generateCounterfactual`, `PersonalWorldModel`, `MissionAlignment`, `SocialTrust`,
`CognitiveLoop`, `AttentionFilter`, `GOVERNANCE_PIPELINE`

## Fire Lines

See `FIRE_LINE.md`. Summary:
- No imports from PWM, MAE, STP, cognitive loops
- No on-device LLM
- No adaptive behavior
- No smart sync
- No semantic reasoning
- Storage = governance state only

## License

BSL 1.1 — stub only, see LICENSE file.
Every Rust source file must have the two-line SPDX header.

## Build

```bash
cargo build --workspace
cargo clippy --workspace -- -D warnings
```

Binding crates (edge-python, edge-node) require optional features and external tooling.
