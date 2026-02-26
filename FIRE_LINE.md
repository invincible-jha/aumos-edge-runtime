# FIRE LINE — Hard Architectural Constraints

The following boundaries are **non-negotiable** for `aumos-edge-runtime`.
Any PR that crosses a fire line will be rejected without review.

---

## 1. No imports from restricted modules

This crate MUST NOT import from:
- PWM (Personal World Model)
- MAE (Mission Alignment Engine)
- STP (Social Trust Pipeline)
- Any cognitive-loop module

If you need data from those systems, they push it to the edge config file; the edge runtime only reads static configuration.

## 2. No on-device LLM inference or model routing

The edge runtime enforces governance rules derived from configuration.
It does not run neural networks, load model weights, or route inference requests.

## 3. No adaptive behavior

All governance decisions are derived from the loaded `EdgeConfig`.
Rules do not change at runtime based on observed behavior.
There is no feedback loop, no learning, and no self-modification.

## 4. No smart sync prioritization

Sync is strictly **push-all then pull-all**.
No selective syncing, no priority queuing based on semantic importance, no ML-driven ordering.

## 5. No local knowledge graph or semantic reasoning

Storage holds only governance state:
- Trust level assignments
- Budget allocations and consumed amounts
- Consent records
- Append-only audit log entries

No entity graphs, no embedding indexes, no vector stores.

## 6. `no_std` core must not acquire hidden platform dependencies

`aumos-edge-core` is designed to work in `no_std` environments.
Do not add dependencies that pull in `std` implicitly unless gated behind an explicit `std` feature flag.

---

Violations of these constraints represent scope creep into systems that have their own dedicated crates.
When in doubt, keep it simpler.
